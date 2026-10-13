//! What each op checks and writes: a member node serving, a module requesting and the
//! chain electing its provider, the provider answering, and an overdue request expiring.
use abi::role::validators;
use guest::{
    BlobId, Error, ExecCtx, QueryCtx, Reply as Wanted, capacity, invalid, not_found, unauthorized,
    unexpected_reply, wrong_state,
};
use store::{PageRequest, PageResponse};

use crate::state::{ASSIGNED, COUNT, DEADLINES, PROVIDERS, REQUESTS, SERVING, Stored, TURNS};
use crate::{
    Answer, Delivery, EXPIRY_BLOCKS, MAX_BODY_BYTES, MAX_DELIVER_BYTES, MAX_INPUT_BYTES,
    MAX_OUTPUT_BYTES, MAX_PARTS, MAX_REASON_BYTES, MAX_SERVICE_BYTES, MAX_SERVICES, Outcome,
    ProviderRow, Request, RequestRow,
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
        && service.len() <= MAX_SERVICE_BYTES
        && service.bytes().all(|byte| {
            byte.is_ascii_lowercase() || byte.is_ascii_digit() || b"-_.".contains(&byte)
        });
    if !named {
        return Err(invalid(format!(
            "a service is 1 to {MAX_SERVICE_BYTES} bytes of lowercase letters, digits, -, _ and ."
        )));
    }
    Ok(())
}

pub(crate) fn serve(ctx: &ExecCtx, mut services: Vec<String>) -> Result<(), Error> {
    let key = ctx.env().signer()?;
    services.sort();
    services.dedup();
    if services.len() > MAX_SERVICES {
        return Err(capacity(format!(
            "a provider serves at most {MAX_SERVICES} services"
        )));
    }
    for service in &services {
        service_name(service)?;
    }
    let stopping = services.is_empty();
    if !stopping && !members(ctx)?.contains(&key) {
        return Err(unauthorized("only a member node of this network provides"));
    }
    let since = match PROVIDERS.get(ctx, &key)? {
        Some(previous) => {
            for service in previous.services {
                SERVING.remove(ctx, &(service, key.clone()));
            }
            previous.since
        }
        None => ctx.env().height,
    };
    if stopping {
        PROVIDERS.remove(ctx, &key);
        return Ok(());
    }
    for service in &services {
        SERVING.insert(ctx, &(service.clone(), key.clone()));
    }
    PROVIDERS.put(
        ctx,
        &key,
        &ProviderRow {
            key: key.clone(),
            services,
            since,
        },
    );
    Ok(())
}

fn check_parts(ctx: &QueryCtx, parts: &[BlobId]) -> Result<(), Error> {
    if parts.len() > MAX_PARTS {
        return Err(capacity(format!("a request has at most {MAX_PARTS} parts")));
    }
    let mut total = 0u64;
    for part in parts {
        let Some(header) = ctx.blob_stat(*part) else {
            return Err(not_found(format!(
                "part {} is not on the network: put it first",
                abi::hex(part.digest())
            )));
        };
        total = total.saturating_add(header.len);
    }
    if total > MAX_BODY_BYTES {
        return Err(capacity(format!(
            "a request's parts are at most {MAX_BODY_BYTES} bytes"
        )));
    }
    Ok(())
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
    let requester = ctx.env().sending_module()?;
    service_name(&request.service)?;
    if request.input.len() > MAX_INPUT_BYTES {
        return Err(capacity(format!(
            "a request's input is at most {MAX_INPUT_BYTES} bytes"
        )));
    }
    if request.deliver.len() > MAX_DELIVER_BYTES {
        return Err(capacity(format!(
            "a request's delivery prefix is at most {MAX_DELIVER_BYTES} bytes"
        )));
    }
    check_parts(ctx, &request.parts)?;
    let provider = elect(ctx, &request.service)?;
    let id = COUNT.get(ctx)?.unwrap_or(0) + 1;
    let made = ctx.env().height;
    let row = RequestRow {
        id,
        requester,
        service: request.service,
        input: request.input,
        parts: request.parts,
        provider,
        made,
        deadline: made + EXPIRY_BLOCKS,
    };
    COUNT.put(ctx, &id);
    ASSIGNED.insert(ctx, &(row.provider.clone(), id));
    DEADLINES.insert(ctx, &(row.deadline, id));
    REQUESTS.put(
        ctx,
        &id,
        &Stored {
            row,
            deliver: request.deliver,
        },
    );
    ctx.set_return_data(abi::encode(&id));
    Ok(())
}

fn open(ctx: &QueryCtx, id: u64) -> Result<Stored, Error> {
    REQUESTS
        .get(ctx, &id)?
        .ok_or_else(|| not_found(format!("no open request {id}")))
}

fn close(ctx: &ExecCtx, stored: Stored, outcome: Outcome) {
    let row = stored.row;
    REQUESTS.remove(ctx, &row.id);
    ASSIGNED.remove(ctx, &(row.provider.clone(), row.id));
    DEADLINES.remove(ctx, &(row.deadline, row.id));
    let delivery = Delivery {
        id: row.id,
        service: row.service,
        outcome,
    };
    ctx.emit_raw(
        row.requester,
        Delivery::payload(&stored.deliver, &delivery),
        Wanted::Wanted,
    );
}

pub(crate) fn answer(ctx: &ExecCtx, id: u64, answer: Answer) -> Result<(), Error> {
    let key = ctx.env().signer()?;
    let stored = open(ctx, id)?;
    if stored.row.provider != key {
        return Err(unauthorized(format!(
            "request {id} was elected to another provider"
        )));
    }
    if ctx.env().height > stored.row.deadline {
        return Err(wrong_state(format!(
            "request {id} expired at height {}",
            stored.row.deadline
        )));
    }
    let outcome = match answer {
        Answer::Output(output) if output.len() > MAX_OUTPUT_BYTES => {
            return Err(capacity(format!(
                "an answer's output is at most {MAX_OUTPUT_BYTES} bytes"
            )));
        }
        Answer::Failed(reason) if reason.len() > MAX_REASON_BYTES => {
            return Err(capacity(format!(
                "a failure's reason is at most {MAX_REASON_BYTES} bytes"
            )));
        }
        Answer::Output(output) => Outcome::Answered(output),
        Answer::Failed(reason) => Outcome::Failed(reason),
    };
    close(ctx, stored, outcome);
    Ok(())
}

pub(crate) fn expire(ctx: &ExecCtx, id: u64) -> Result<(), Error> {
    let stored = open(ctx, id)?;
    if ctx.env().height <= stored.row.deadline {
        return Err(wrong_state(format!(
            "request {id} is open until height {}",
            stored.row.deadline
        )));
    }
    close(ctx, stored, Outcome::Expired);
    Ok(())
}

pub(crate) fn assigned(
    ctx: &QueryCtx,
    key: Vec<u8>,
    page: &PageRequest,
) -> Result<PageResponse<RequestRow>, Error> {
    ASSIGNED
        .range_of(ctx, &key, page)?
        .try_map(|(_, id)| Ok(open(ctx, id)?.row))
}

pub(crate) fn request_row(ctx: &QueryCtx, id: u64) -> Result<Option<RequestRow>, Error> {
    Ok(REQUESTS.get(ctx, &id)?.map(|stored| stored.row))
}

pub(crate) fn overdue(
    ctx: &QueryCtx,
    page: &PageRequest,
) -> Result<PageResponse<RequestRow>, Error> {
    let height = ctx.env().height;
    let mut due = DEADLINES.range(ctx, page)?;
    let past: Vec<(u64, u64)> = due
        .items
        .iter()
        .copied()
        .take_while(|(deadline, _)| *deadline < height)
        .collect();
    if past.len() < due.items.len() {
        due.next = None;
    }
    due.items = past;
    due.try_map(|(_, id)| Ok(open(ctx, id)?.row))
}

pub(crate) fn providers(
    ctx: &QueryCtx,
    page: &PageRequest,
) -> Result<PageResponse<ProviderRow>, Error> {
    Ok(PROVIDERS.range(ctx, page)?.map(|(_, row)| row))
}
