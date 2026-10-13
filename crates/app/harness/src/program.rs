//! The module: every op and query handed to its function, and the outcome of each write a
//! tool call made handed back to the loop as that call's result.
use guest::{Error, ExecCtx, MessageId, Module, Outcome, Program, QueryCtx};

use crate::agents::{attributed, close, dismiss, hire};
use crate::turns::{relayed, turned, written};
use crate::{MODULE, Op, Query, Reply, queries, tools};

pub struct Harness;

impl Program for Harness {
    const NAME: &'static str = MODULE;
    type Op = Op;
    type Query = Query;
    type Reply = Reply;
}

impl Module for Harness {
    fn execute(ctx: &ExecCtx, op: Op) -> Result<(), Error> {
        match op {
            Op::Hire {
                agent,
                system,
                model,
                max_tokens,
            } => hire(ctx, agent, system, model, max_tokens),
            Op::Dismiss { agent } => dismiss(ctx, agent),
            Op::Close { thread } => close(ctx, thread),
            Op::Turned { thread, delivery } => turned(ctx, thread, delivery),
            Op::Relayed {
                thread,
                call,
                delivery,
            } => relayed(ctx, thread, call, delivery),
            Op::Attributed(change) => attributed(ctx, *change),
        }
    }

    fn reply(ctx: &ExecCtx, id: &MessageId, outcome: &Outcome) -> Result<(), Error> {
        written(ctx, id.seq, outcome)
    }

    fn query(ctx: &QueryCtx, query: Query) -> Result<Reply, Error> {
        match query {
            Query::Agent { agent } => Ok(Reply::Agent(queries::agent(ctx, agent)?)),
            Query::Agents { page } => Ok(Reply::Agents(queries::agents(ctx, &page)?)),
            Query::Thread { thread } => Ok(Reply::Thread(queries::thread(ctx, thread)?)),
            Query::Threads { agent, page } => {
                Ok(Reply::Threads(queries::threads(ctx, agent, &page)?))
            }
            Query::Frames { thread, page } => {
                Ok(Reply::Frames(queries::frames(ctx, thread, &page)?))
            }
            Query::Held { thread } => Ok(Reply::Held(queries::held(ctx, thread)?)),
            Query::Calls { thread, page } => Ok(Reply::Calls(queries::calls(ctx, thread, &page)?)),
            Query::Tools => Ok(Reply::Tools(tools::infos())),
        }
    }
}

#[cfg(feature = "module")]
guest::export!(Harness);
