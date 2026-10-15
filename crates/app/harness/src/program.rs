//! The program: every op and query handed to its function. An attribution may start a thread's
//! loop, which awaits the model and each tool call.
use ::program::{Awaiting, Ctx, Error, Program, QueryCtx};

use crate::agents::{attributed, close, dismiss, hire};
use crate::{Op, PROGRAM, Query, Reply, queries, tools};

pub struct Harness;

impl Program for Harness {
    const NAME: &'static str = PROGRAM;
    type Op = Op;
    type Query = Query;
    type Reply = Reply;

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

impl Awaiting for Harness {
    async fn execute(ctx: Ctx, op: Op) -> Result<(), Error> {
        match op {
            Op::Hire {
                agent,
                system,
                model,
                max_tokens,
            } => hire(&ctx, agent, system, model, max_tokens),
            Op::Dismiss { agent } => dismiss(&ctx, agent),
            Op::Close { thread } => close(&ctx, thread),
            Op::Attributed(change) => attributed(ctx, *change).await,
        }
    }
}

#[cfg(feature = "program")]
::program::export_awaiting!(Harness);
