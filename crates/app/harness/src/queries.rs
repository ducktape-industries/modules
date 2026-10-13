//! Every read: the agents hired, a thread, an agent's threads newest first, a transcript in
//! order, the input held while the model answers, a thread's tool calls, and the tool catalog.
use abi::role::identity::AccountNumber;
use guest::{Error, QueryCtx, corrupt};
use store::{PageRequest, PageResponse};

use crate::state::{AGENTS, BY_AGENT, CALLS, FRAMES, HELD, THREADS};
use crate::{AgentRow, Body, CallRow, Frame, ThreadRow};

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

pub(crate) fn held(ctx: &QueryCtx, thread: u64) -> Result<Vec<Body>, Error> {
    Ok(HELD
        .scan(ctx, HELD.prefix_of(&thread))?
        .into_iter()
        .map(|(_, body)| body)
        .collect())
}
