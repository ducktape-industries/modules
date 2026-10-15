//! What each op checks and writes: an account registering a service and assigning the nodes
//! that may serve it, a member node serving, a program's call and the chain electing its node,
//! the node answering it, and an overdue call expiring.
use ::program::kernel::error_from;
use ::program::{
    BlobId, Cause, Error, ExecCtx, QueryCtx, invalid, not_found, unauthorized, unexpected_reply,
    wrong_state,
};
use abi::role::{awaits, validators};
use store::{PageRequest, PageResponse};

use crate::state::{ASSIGNED, COUNT, DEADLINES, NODES, REQUESTS, SERVICES, SERVING, Stored, TURNS};
use crate::{
    Answer, EXPIRY_BLOCKS, HTTP, NodeRow, RELAY, RESERVED, Request, RequestRow, ServiceRow,
};

fn members(ctx: &QueryCtx) -> Result<Vec<Vec<u8>>, Error> {
    let validators = &ctx.env().roles.validators;
    match ctx
        .query::<validators::Query, validators::Reply>(validators, &validators::Query::Members)?
    {
        validators::Reply::Members(members) => {
            Ok(members.into_iter().map(|member| member.key).collect())
        }
        other => Err(unexpected_reply(validators, "Members", &other)),
    }
}

fn service_name(service: &str) -> Result<(), Error> {
    let named = !service.is_empty()
        && service.bytes().all(|byte| {
            byte.is_ascii_lowercase() || byte.is_ascii_digit() || b"-_.".contains(&byte)
        });
    if !named {
        return Err(invalid(
            "a service is lowercase letters, digits, -, _ and .",
        ));
    }
    Ok(())
}

fn registered(ctx: &QueryCtx, name: &str) -> Result<ServiceRow, Error> {
    SERVICES
        .get(ctx, &name.to_owned())?
        .ok_or_else(|| not_found(format!("no service named {name} is registered")))
}

pub(crate) fn register(ctx: &ExecCtx, name: String) -> Result<(), Error> {
    let owner = ctx.sender_account()?;
    service_name(&name)?;
    if RESERVED.contains(&name.as_str()) {
        return Err(invalid(format!(
            "{name} is egress's own service, never registered"
        )));
    }
    if SERVICES.has(ctx, &name) {
        return Err(wrong_state(format!("service {name} is registered")));
    }
    let row = ServiceRow {
        name: name.clone(),
        owner,
        nodes: Vec::new(),
        registered: ctx.env().height,
    };
    SERVICES.put(ctx, &name, &row);
    Ok(())
}

pub(crate) fn assign(ctx: &ExecCtx, name: String, mut nodes: Vec<Vec<u8>>) -> Result<(), Error> {
    let account = ctx.sender_account()?;
    let mut row = registered(ctx, &name)?;
    if row.owner != account {
        return Err(unauthorized(format!(
            "service {name} is account {}'s to assign",
            row.owner
        )));
    }
    nodes.sort();
    nodes.dedup();
    row.nodes = nodes;
    SERVICES.put(ctx, &name, &row);
    Ok(())
}

pub(crate) fn serve(ctx: &ExecCtx, mut services: Vec<String>) -> Result<(), Error> {
    let key = ctx.env().signer()?;
    services.sort();
    services.dedup();
    for service in &services {
        service_name(service)?;
    }
    let stopping = services.is_empty();
    if !stopping && !members(ctx)?.contains(&key) {
        return Err(unauthorized("only a member node of this network serves"));
    }
    let since = match NODES.get(ctx, &key)? {
        Some(previous) => {
            for service in previous.services {
                SERVING.remove(ctx, &(service, key.clone()));
            }
            previous.since
        }
        None => ctx.env().height,
    };
    if stopping {
        NODES.remove(ctx, &key);
        return Ok(());
    }
    for service in &services {
        SERVING.insert(ctx, &(service.clone(), key.clone()));
    }
    NODES.put(
        ctx,
        &key,
        &NodeRow {
            key: key.clone(),
            services,
            since,
        },
    );
    Ok(())
}

fn check_parts(ctx: &QueryCtx, parts: &[BlobId]) -> Result<(), Error> {
    let missing = parts.iter().find(|part| ctx.blob_stat(**part).is_none());
    match missing {
        Some(part) => Err(not_found(format!(
            "part {} is not on the network: put it first",
            abi::hex(part.digest())
        ))),
        None => Ok(()),
    }
}

fn check(ctx: &QueryCtx, request: &Request) -> Result<(), Error> {
    let Request::Http {
        service,
        head,
        body,
    } = request
    else {
        return Ok(());
    };
    match service.as_str() {
        HTTP => head.check_url().map_err(error_from)?,
        RELAY => return Err(invalid("relay is asked with Request::Relay")),
        named => {
            registered(ctx, named)?;
            head.check_path().map_err(error_from)?;
        }
    }
    check_parts(ctx, body)
}

pub(crate) fn serving(ctx: &QueryCtx, service: &str) -> Result<Vec<Vec<u8>>, Error> {
    let members = members(ctx)?;
    let reserved = RESERVED.contains(&service);
    let assigned = match reserved {
        true => None,
        false => Some(
            SERVICES
                .get(ctx, &service.to_owned())?
                .map(|row| row.nodes)
                .unwrap_or_default(),
        ),
    };
    let may_serve = |key: &Vec<u8>| {
        let member = members.contains(key);
        let allowed = assigned.as_ref().is_none_or(|nodes| nodes.contains(key));
        member && allowed
    };
    Ok(SERVING
        .scan(ctx, SERVING.prefix_of(&service.to_owned()))?
        .into_iter()
        .map(|(_, key)| key)
        .filter(may_serve)
        .collect())
}

fn elect(ctx: &ExecCtx, service: &str) -> Result<Vec<u8>, Error> {
    let eligible = serving(ctx, service)?;
    if eligible.is_empty() {
        return Err(not_found(format!("no member node serves {service}")));
    }
    let turn = TURNS.get(ctx, &service.to_owned())?.unwrap_or(0);
    TURNS.put(ctx, &service.to_owned(), &(turn + 1));
    Ok(eligible[(turn % eligible.len() as u64) as usize].clone())
}

pub(crate) fn request(ctx: &ExecCtx, request: Request) -> Result<(), Error> {
    let requester = ctx.env().sending_program()?;
    let called = matches!(ctx.env().cause, Cause::Call { .. });
    if !called {
        return Err(wrong_state(
            "a request is a call: the program awaits egress with ctx.call",
        ));
    }
    check(ctx, &request)?;
    let node = elect(ctx, request.service())?;
    let ticket = ctx.defer()?;
    let id = COUNT.get(ctx)?.unwrap_or(0) + 1;
    let made = ctx.env().height;
    let row = RequestRow {
        id,
        requester,
        request,
        node,
        made,
        deadline: made + EXPIRY_BLOCKS,
    };
    COUNT.put(ctx, &id);
    ASSIGNED.insert(ctx, &(row.node.clone(), id));
    DEADLINES.insert(ctx, &(row.deadline, id));
    REQUESTS.put(ctx, &id, &Stored { row, ticket });
    Ok(())
}

fn open(ctx: &QueryCtx, id: u64) -> Result<Stored, Error> {
    REQUESTS
        .get(ctx, &id)?
        .ok_or_else(|| not_found(format!("no open request {id}")))
}

fn close(ctx: &ExecCtx, stored: Stored, answer: Result<Vec<u8>, Error>) {
    let row = stored.row;
    REQUESTS.remove(ctx, &row.id);
    ASSIGNED.remove(ctx, &(row.node.clone(), row.id));
    DEADLINES.remove(ctx, &(row.deadline, row.id));
    ctx.resolve(stored.ticket, answer);
}

pub(crate) fn answer(ctx: &ExecCtx, id: u64, answer: Answer) -> Result<(), Error> {
    let key = ctx.env().signer()?;
    let stored = open(ctx, id)?;
    if stored.row.node != key {
        return Err(unauthorized(format!(
            "request {id} was elected to another node"
        )));
    }
    let same_block = ctx.env().height <= stored.row.made;
    if same_block {
        return Err(wrong_state(format!(
            "request {id} is answered in a later block than the one that made it"
        )));
    }
    if ctx.env().height > stored.row.deadline {
        return Err(wrong_state(format!(
            "request {id} expired at height {}",
            stored.row.deadline
        )));
    }
    let answered = match answer {
        Answer::Output(output) => Ok(output),
        Answer::Failed(reason) => Err(failed(stored.row.request.service(), &reason)),
    };
    close(ctx, stored, answered);
    Ok(())
}

fn failed(service: &str, reason: &str) -> Error {
    wrong_state(format!("the {service} service failed: {reason}"))
}

pub(crate) fn expire(ctx: &ExecCtx, id: u64) -> Result<(), Error> {
    let stored = open(ctx, id)?;
    if ctx.env().height <= stored.row.deadline {
        return Err(wrong_state(format!(
            "request {id} is open until height {}",
            stored.row.deadline
        )));
    }
    let expired = awaits::expired(stored.ticket, ctx.env().height);
    close(ctx, stored, Err(error_from(expired)));
    Ok(())
}

pub(crate) fn assigned(ctx: &QueryCtx, key: &Vec<u8>) -> Result<Vec<RequestRow>, Error> {
    ASSIGNED
        .scan(ctx, ASSIGNED.prefix_of(key))?
        .into_iter()
        .map(|(_, id)| Ok(open(ctx, id)?.row))
        .collect()
}

pub(crate) fn request_row(ctx: &QueryCtx, id: u64) -> Result<Option<RequestRow>, Error> {
    Ok(REQUESTS.get(ctx, &id)?.map(|stored| stored.row))
}

pub(crate) fn overdue(ctx: &QueryCtx) -> Result<Vec<RequestRow>, Error> {
    let height = ctx.env().height;
    DEADLINES
        .all(ctx)?
        .into_iter()
        .take_while(|(deadline, _)| *deadline < height)
        .map(|(_, id)| Ok(open(ctx, id)?.row))
        .collect()
}

pub(crate) fn node(ctx: &QueryCtx, key: &Vec<u8>) -> Result<Option<NodeRow>, Error> {
    NODES.get(ctx, key)
}

pub(crate) fn nodes(ctx: &QueryCtx, page: &PageRequest) -> Result<PageResponse<NodeRow>, Error> {
    Ok(NODES.range(ctx, page)?.map(|(_, row)| row))
}

pub(crate) fn service(ctx: &QueryCtx, name: &str) -> Result<Option<ServiceRow>, Error> {
    SERVICES.get(ctx, &name.to_owned())
}

pub(crate) fn services(
    ctx: &QueryCtx,
    page: &PageRequest,
) -> Result<PageResponse<ServiceRow>, Error> {
    Ok(SERVICES.range(ctx, page)?.map(|(_, row)| row))
}
