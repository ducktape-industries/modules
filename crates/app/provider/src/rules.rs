//! What each op checks and writes: a member node serving, a program's call and the chain
//! electing its provider, the next provider elected when one stops serving or leaves, the
//! provider answering it, and a request forgotten once the awaits role no longer waits on it.
use ::program::{
    BlobId, Cause, Error, ExecCtx, QueryCtx, invalid, not_found, unauthorized, unexpected_reply,
    wrong_state,
};
use abi::role::{awaits, validators};
use store::{PageRequest, PageResponse};

use crate::state::{ASSIGNED, COUNT, PROVIDERS, REQUESTS, SERVING, TURNS};
use crate::{Answer, ProviderRow, Request, RequestRow};

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

pub(crate) fn serve(ctx: &ExecCtx, mut services: Vec<String>) -> Result<(), Error> {
    let key = ctx.env().signer()?;
    services.sort();
    services.dedup();
    for service in &services {
        service_name(service)?;
    }
    let stopping = services.is_empty();
    if !stopping && !members(ctx)?.contains(&key) {
        return Err(unauthorized("only a member node of this network provides"));
    }
    let previous = PROVIDERS.get(ctx, &key)?;
    let since = previous
        .as_ref()
        .map_or(ctx.env().height, |previous| previous.since);
    let served = previous
        .map(|previous| previous.services)
        .unwrap_or_default();
    for service in &served {
        SERVING.remove(ctx, &(service.clone(), key.clone()));
    }
    for service in &services {
        SERVING.insert(ctx, &(service.clone(), key.clone()));
    }
    let dropped: Vec<String> = served
        .into_iter()
        .filter(|service| !services.contains(service))
        .collect();
    match stopping {
        true => PROVIDERS.remove(ctx, &key),
        false => PROVIDERS.put(
            ctx,
            &key,
            &ProviderRow {
                key: key.clone(),
                services,
                since,
            },
        ),
    }
    hand_on_dropped(ctx, &key, &dropped)
}

fn hand_on_dropped(ctx: &ExecCtx, key: &[u8], dropped: &[String]) -> Result<(), Error> {
    for (_, id) in ASSIGNED.scan(ctx, ASSIGNED.prefix_of(&key.to_vec()))? {
        let row = open(ctx, id)?;
        let unserved = dropped.contains(&row.service);
        if unserved {
            move_on(ctx, row)?;
        }
    }
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

pub(crate) fn serving(ctx: &QueryCtx, service: &str) -> Result<Vec<Vec<u8>>, Error> {
    let members = members(ctx)?;
    Ok(SERVING
        .scan(ctx, SERVING.prefix_of(&service.to_owned()))?
        .into_iter()
        .map(|(_, key)| key)
        .filter(|key| members.contains(key))
        .collect())
}

enum Election {
    Elected { provider: Vec<u8>, next_turn: u64 },
    Unserved,
}

fn election(ctx: &QueryCtx, service: &str) -> Result<Election, Error> {
    let eligible = serving(ctx, service)?;
    let unserved = eligible.is_empty();
    if unserved {
        return Ok(Election::Unserved);
    }
    let turn = TURNS.get(ctx, &service.to_owned())?.unwrap_or(0);
    let provider = eligible[(turn % eligible.len() as u64) as usize].clone();
    Ok(Election::Elected {
        provider,
        next_turn: turn + 1,
    })
}

fn unserved(service: &str) -> Error {
    not_found(format!("no member node serves {service}"))
}

fn take_turn(ctx: &ExecCtx, service: &str, next_turn: u64) {
    TURNS.put(ctx, &service.to_owned(), &next_turn);
}

pub(crate) fn request(ctx: &ExecCtx, request: Request) -> Result<(), Error> {
    let requester = ctx.env().sending_program()?;
    let called = matches!(ctx.env().cause, Cause::Call { .. });
    if !called {
        return Err(wrong_state(
            "a request is a call: the program awaits provider with ctx.call",
        ));
    }
    service_name(&request.service)?;
    check_parts(ctx, &request.parts)?;
    let Election::Elected {
        provider,
        next_turn,
    } = election(ctx, &request.service)?
    else {
        return Err(unserved(&request.service));
    };
    take_turn(ctx, &request.service, next_turn);
    let ticket = ctx.defer()?;
    let id = COUNT.get(ctx)?.unwrap_or(0) + 1;
    let row = RequestRow {
        id,
        requester,
        service: request.service,
        input: request.input,
        parts: request.parts,
        provider,
        made: ctx.env().height,
        ticket,
    };
    COUNT.put(ctx, &id);
    ASSIGNED.insert(ctx, &(row.provider.clone(), id));
    REQUESTS.put(ctx, &id, &row);
    Ok(())
}

fn open(ctx: &QueryCtx, id: u64) -> Result<RequestRow, Error> {
    REQUESTS
        .get(ctx, &id)?
        .ok_or_else(|| not_found(format!("no open request {id}")))
}

enum Standing {
    Waiting,
    Over,
}

fn standing(ctx: &QueryCtx, row: &RequestRow) -> Result<Standing, Error> {
    let role = &ctx.env().roles.awaits;
    let asked = awaits::Query::Ticket { ticket: row.ticket };
    let held = match ctx.query::<awaits::Query, awaits::Reply>(role, &asked)? {
        awaits::Reply::Ticket(held) => held,
        other => return Err(unexpected_reply(role, "Ticket", &other)),
    };
    let Some(held) = held else {
        return Ok(Standing::Over);
    };
    let late = ctx.env().height > held.deadline;
    match late {
        true => Ok(Standing::Over),
        false => Ok(Standing::Waiting),
    }
}

fn forget(ctx: &ExecCtx, row: &RequestRow) {
    REQUESTS.remove(ctx, &row.id);
    ASSIGNED.remove(ctx, &(row.provider.clone(), row.id));
}

fn close(ctx: &ExecCtx, row: RequestRow, answer: Result<Vec<u8>, Error>) {
    forget(ctx, &row);
    ctx.resolve(row.ticket, answer);
}

fn move_on(ctx: &ExecCtx, row: RequestRow) -> Result<(), Error> {
    match standing(ctx, &row)? {
        Standing::Waiting => reelect(ctx, row)?,
        Standing::Over => forget(ctx, &row),
    }
    Ok(())
}

fn reelect(ctx: &ExecCtx, row: RequestRow) -> Result<(), Error> {
    match election(ctx, &row.service)? {
        Election::Elected {
            provider,
            next_turn,
        } => hand_on(ctx, row, provider, next_turn),
        Election::Unserved => fail_unserved(ctx, row),
    }
    Ok(())
}

fn hand_on(ctx: &ExecCtx, mut row: RequestRow, provider: Vec<u8>, next_turn: u64) {
    take_turn(ctx, &row.service, next_turn);
    ASSIGNED.remove(ctx, &(row.provider.clone(), row.id));
    ASSIGNED.insert(ctx, &(provider.clone(), row.id));
    row.provider = provider;
    REQUESTS.put(ctx, &row.id, &row);
}

fn fail_unserved(ctx: &ExecCtx, row: RequestRow) {
    let refusal = unserved(&row.service);
    close(ctx, row, Err(refusal));
}

pub(crate) fn reassign(ctx: &ExecCtx, id: u64) -> Result<(), Error> {
    let row = open(ctx, id)?;
    match standing(ctx, &row)? {
        Standing::Waiting => reassign_waiting(ctx, row)?,
        Standing::Over => forget(ctx, &row),
    }
    Ok(())
}

fn reassign_waiting(ctx: &ExecCtx, row: RequestRow) -> Result<(), Error> {
    let still_serving = serving(ctx, &row.service)?.contains(&row.provider);
    if still_serving {
        return Err(wrong_state(format!(
            "request {}'s provider is a member node that still serves {}: it answers, or the awaits role expires the call",
            row.id, row.service
        )));
    }
    reelect(ctx, row)
}

pub(crate) fn answer(ctx: &ExecCtx, id: u64, answer: Answer) -> Result<(), Error> {
    let key = ctx.env().signer()?;
    let row = open(ctx, id)?;
    if row.provider != key {
        return Err(unauthorized(format!(
            "request {id} was elected to another provider"
        )));
    }
    let same_block = ctx.env().height <= row.made;
    if same_block {
        return Err(wrong_state(format!(
            "request {id} is answered in a later block than the one that made it"
        )));
    }
    match standing(ctx, &row)? {
        Standing::Waiting => deliver(ctx, row, answer),
        Standing::Over => forget(ctx, &row),
    }
    Ok(())
}

fn deliver(ctx: &ExecCtx, row: RequestRow, answer: Answer) {
    let answered = match answer {
        Answer::Output(output) => Ok(output),
        Answer::Failed(reason) => Err(failed(&row.service, &reason)),
    };
    close(ctx, row, answered);
}

fn failed(service: &str, reason: &str) -> Error {
    wrong_state(format!("the {service} provider failed: {reason}"))
}

pub(crate) fn assigned(
    ctx: &QueryCtx,
    key: Vec<u8>,
    page: &PageRequest,
) -> Result<PageResponse<RequestRow>, Error> {
    ASSIGNED
        .range_of(ctx, &key, page)?
        .try_map(|(_, id)| open(ctx, id))
}

pub(crate) fn request_row(ctx: &QueryCtx, id: u64) -> Result<Option<RequestRow>, Error> {
    REQUESTS.get(ctx, &id)
}

pub(crate) fn providers(
    ctx: &QueryCtx,
    page: &PageRequest,
) -> Result<PageResponse<ProviderRow>, Error> {
    Ok(PROVIDERS.range(ctx, page)?.map(|(_, row)| row))
}
