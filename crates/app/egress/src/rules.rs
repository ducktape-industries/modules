//! What each op checks and writes: an account registering a service and assigning the nodes
//! that may serve it, a member node serving, a program's call and the chain electing its node,
//! the node answering it (whole, or as events and then the stream's end), and the request's own
//! scheduled run expiring it.
use ::program::kernel::error_from;
use ::program::{
    BlobId, Cause, Ctx, Error, ExecCtx, Origin, QueryCtx, code, invalid, not_found, unauthorized,
    unexpected_reply, wrong_state,
};
use abi::role::validators;
use store::{PageRequest, PageResponse};

use crate::state::{ASSIGNED, COUNT, NODES, REQUESTS, SERVICES, SERVING, Stored, TURNS};
use crate::{
    Answer, DEFAULT_WAIT, Delivery, Event, HTTP, NodeRow, Op, PROGRAM, RELAY, RESERVED, Request,
    RequestRow, ServiceRow, StreamEnd, http,
};

fn members(ctx: &QueryCtx) -> Result<Vec<Vec<u8>>, Error> {
    let validators = &ctx.env().role(abi::role::VALIDATORS)?.clone();
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

fn expiry_height(made: u64, wait: u64) -> Result<u64, Error> {
    if wait == 0 {
        return Err(invalid("a request waits at least one block"));
    }
    made.checked_add(wait).ok_or_else(|| {
        invalid(format!(
            "a wait of {wait} blocks from height {made} passes the last height"
        ))
    })
}

pub(crate) async fn request(
    ctx: &Ctx,
    request: Request,
    delivery: Delivery,
    wait: Option<u64>,
) -> Result<(), Error> {
    let requester = ctx.env().sending_program()?;
    let called = matches!(ctx.env().cause, Cause::Call { .. });
    if !called {
        return Err(wrong_state(
            "a request is a call: the program awaits egress with ctx.call",
        ));
    }
    let unscheduled = ctx.env().roles.get(abi::role::SCHEDULER).is_none();
    if unscheduled {
        return Err(Error::new(
            code::UNSUPPORTED,
            "egress expires a request through the scheduler role, and this network binds no \
             scheduler",
        ));
    }
    let made = ctx.env().height;
    let wait = wait.unwrap_or(DEFAULT_WAIT);
    let expires = expiry_height(made, wait)?;
    check(ctx, &request)?;
    let streams_a_relay = matches!(
        (&request, &delivery),
        (Request::Relay { .. }, Delivery::Events { .. })
    );
    if streams_a_relay {
        return Err(invalid(
            "a relay is answered whole: ask it with Delivery::Whole",
        ));
    }
    let node = elect(ctx, request.service())?;
    let id = COUNT.get(ctx)?.unwrap_or(0) + 1;
    COUNT.put(ctx, &id);
    let expiry = ctx.schedule(expires, PROGRAM, &Op::Expire { id }).await?;
    let ticket = ctx.defer()?;
    let row = RequestRow {
        id,
        requester,
        request,
        delivery,
        node,
        made,
        expiry,
    };
    ASSIGNED.insert(ctx, &(row.node.clone(), id));
    REQUESTS.put(ctx, &id, &Stored { row, ticket, wait });
    Ok(())
}

fn open(ctx: &QueryCtx, id: u64) -> Result<Stored, Error> {
    REQUESTS
        .get(ctx, &id)?
        .ok_or_else(|| not_found(format!("no open request {id}")))
}

fn close(ctx: &ExecCtx, stored: Stored, answer: Result<Vec<u8>, Error>) -> Result<(), Error> {
    let row = stored.row;
    REQUESTS.remove(ctx, &row.id);
    ASSIGNED.remove(ctx, &(row.node.clone(), row.id));
    ctx.resolve(stored.ticket, answer)?;
    Ok(())
}

/// Request `id`, open, from the node it was elected to, in a later block than the one that
/// made it: what each of the node's frames for it must be.
fn from_its_node(ctx: &Ctx, id: u64) -> Result<Stored, Error> {
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
    Ok(stored)
}

/// The prefix an events request's requester hears each event under; a whole request has none.
fn streamed(stored: &Stored) -> Result<&[u8], Error> {
    match &stored.row.delivery {
        Delivery::Events { deliver } => Ok(deliver),
        Delivery::Whole => Err(wrong_state(format!(
            "request {} asked for its answer whole",
            stored.row.id
        ))),
    }
}

pub(crate) async fn answer(ctx: &Ctx, id: u64, answer: Answer) -> Result<(), Error> {
    let stored = from_its_node(ctx, id)?;
    ctx.cancel(stored.row.expiry).await?;
    let answered = match answer {
        Answer::Output(output) => Ok(output),
        Answer::Failed(reason) => Err(failed(stored.row.request.service(), &reason)),
    };
    close(ctx, stored, answered)
}

/// `seq` is passed through as the node sent it, never checked: it counts from 0 per call the
/// node makes, so a repeated `seq` is a repeat (a frame sent again beside the one already
/// relayed) and a `seq` back at 0 is the node calling the service again after a restart, which
/// voids what came before. Only the requester can act on either; a check here would hide them
/// (a strict next-`seq` wedges the stream after one refused event, a rising one splices a
/// restarted call onto the old one's prefix).
pub(crate) async fn events(ctx: &Ctx, events: Vec<Event>) -> Result<(), Error> {
    if events.is_empty() {
        return Err(invalid("an Events op carries at least one event"));
    }
    let mut events = events.into_iter().peekable();
    while let Some(first) = events.next() {
        let id = first.id;
        let mut stored = from_its_node(ctx, id)?;
        let deliver = streamed(&stored)?.to_vec();
        let expires = expiry_height(ctx.env().height, stored.wait)?;
        ctx.cancel(stored.row.expiry).await?;
        stored.row.expiry = ctx.schedule(expires, PROGRAM, &Op::Expire { id }).await?;
        let same_request = std::iter::once(first).chain(std::iter::from_fn(|| {
            events.next_if(|event| event.id == id)
        }));
        for event in same_request {
            ctx.emit_raw(
                stored.row.requester.clone(),
                event.payload(&deliver),
                ::program::Reply::None,
            );
        }
        REQUESTS.put(ctx, &id, &stored);
    }
    Ok(())
}

/// An events request's stream ended: its call answers `Complete` as the encoded
/// `http::Response` with the stream's status and headers and an empty body (the events were the
/// body), so an `Http` request's call answers one type whether its reply came whole or as
/// events; a cut answers the service's failure.
pub(crate) async fn end_stream(
    ctx: &Ctx,
    id: u64,
    events: u64,
    end: StreamEnd,
) -> Result<(), Error> {
    let stored = from_its_node(ctx, id)?;
    streamed(&stored)?;
    ctx.cancel(stored.row.expiry).await?;
    let ended = match &end {
        StreamEnd::Complete { status, headers } => Ok(abi::encode(&http::Response {
            status: *status,
            headers: headers.clone(),
            body: Vec::new(),
        })),
        StreamEnd::Cut { reason } => Err(failed(
            stored.row.request.service(),
            &format!("the stream broke off, the node sent {events} events: {reason}"),
        )),
    };
    close(ctx, stored, ended)
}

fn failed(service: &str, reason: &str) -> Error {
    wrong_state(format!("the {service} service failed: {reason}"))
}

pub(crate) fn expire(ctx: &ExecCtx, id: u64) -> Result<(), Error> {
    let env = ctx.env();
    let Cause::Scheduled { entry } = env.cause else {
        return Err(unauthorized(
            "a request expires only by its own scheduled run",
        ));
    };
    let from_itself = env.origin == Origin::Program(env.program.clone());
    if !from_itself {
        return Err(unauthorized(
            "a request expires only by the run egress scheduled",
        ));
    }
    let stored = open(ctx, id)?;
    let its_expiry = stored.row.expiry == entry;
    if !its_expiry {
        return Err(unauthorized(format!(
            "entry {entry} is not request {id}'s expiry"
        )));
    }
    let waited = env.height.saturating_sub(stored.row.made);
    let expired = wrong_state(format!("request {id} expired after {waited} blocks"));
    close(ctx, stored, Err(expired))
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
