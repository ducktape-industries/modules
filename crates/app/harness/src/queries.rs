//! Every read: the agents hired, a thread, an agent's threads newest first, a transcript in
//! order, the input held while a thread's loop runs, a thread's tool calls, the tool catalog,
//! an agent's thread for a place, a model's context window, and the frames left out of the
//! model's view with their size.
use ::program::{Error, QueryCtx, corrupt};
use abi::role::identity::AccountNumber;
use store::{PageRequest, PageResponse};

use crate::state::{AGENTS, BY_AGENT, CALLS, FRAMES, HELD, PLACES, THREADS, WINDOWS, WITHHELD};
use crate::{AgentRow, Body, CallRow, Frame, ThreadRow, WithheldFrame};

pub(crate) fn agent(ctx: &QueryCtx, agent: AccountNumber) -> Result<Option<AgentRow>, Error> {
    AGENTS.get(ctx, &agent)
}

pub(crate) fn agents(ctx: &QueryCtx, page: &PageRequest) -> Result<PageResponse<AgentRow>, Error> {
    Ok(AGENTS.range(ctx, page)?.map(|(_, row)| row))
}

pub(crate) fn thread(ctx: &QueryCtx, id: u64) -> Result<Option<ThreadRow>, Error> {
    THREADS.get(ctx, &id)
}

pub(crate) fn threads(
    ctx: &QueryCtx,
    agent: AccountNumber,
    page: &PageRequest,
) -> Result<PageResponse<ThreadRow>, Error> {
    BY_AGENT.range_of(ctx, &agent, page)?.try_map(|(_, key)| {
        let id = u64::MAX - key;
        THREADS.get(ctx, &id)?.ok_or_else(|| {
            corrupt(
                "thread",
                &id.to_be_bytes(),
                "an agent names a thread that is gone",
            )
        })
    })
}

pub(crate) fn frames(
    ctx: &QueryCtx,
    thread: u64,
    page: &PageRequest,
) -> Result<PageResponse<Frame>, Error> {
    Ok(FRAMES.range_of(ctx, &thread, page)?.map(|(_, frame)| frame))
}

pub(crate) fn calls(
    ctx: &QueryCtx,
    thread: u64,
    page: &PageRequest,
) -> Result<PageResponse<CallRow>, Error> {
    Ok(CALLS.range_of(ctx, &thread, page)?.map(|(_, call)| call))
}

pub(crate) fn thread_at(
    ctx: &QueryCtx,
    agent: AccountNumber,
    place: String,
) -> Result<Option<ThreadRow>, Error> {
    match PLACES.get(ctx, &(agent, place))? {
        Some(id) => THREADS.get(ctx, &id),
        None => Ok(None),
    }
}

pub(crate) fn window(ctx: &QueryCtx, model: String) -> Result<Option<Option<u64>>, Error> {
    WINDOWS.get(ctx, &model)
}

pub(crate) fn withheld(ctx: &QueryCtx, thread: u64) -> Result<Vec<WithheldFrame>, Error> {
    let left_out = WITHHELD.scan(ctx, WITHHELD.prefix_of(&thread))?;
    left_out
        .into_iter()
        .map(|key| {
            let frame = FRAMES.get(ctx, &key)?.ok_or_else(|| {
                corrupt(
                    "withheld",
                    &WITHHELD.key(&key),
                    "names a frame that is gone",
                )
            })?;
            Ok(WithheldFrame {
                seq: key.1,
                size: frame.body.text().len() as u64,
            })
        })
        .collect()
}

pub(crate) fn held(ctx: &QueryCtx, thread: u64) -> Result<Vec<Body>, Error> {
    Ok(HELD
        .scan(ctx, HELD.prefix_of(&thread))?
        .into_iter()
        .map(|(_, body)| body)
        .collect())
}
