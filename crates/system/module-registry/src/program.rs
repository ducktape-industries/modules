//! The module: the schedule folded at each block, then every op and every
//! query, each handed to its rule.

use guest::{Error, ExecCtx, Module, Program, QueryCtx, decoded};

use crate::rules::{SCHEDULE, at, cancel, fold, init, publish, schedule, views_at};
use crate::{Genesis, MODULE, Op, Query, Reply, Scheduled};

pub struct Modules;

impl Program for Modules {
    const NAME: &'static str = MODULE;
    type Op = Op;
    type Query = Query;
    type Reply = Reply;
}

impl Module for Modules {
    fn init(ctx: &ExecCtx, params: &[u8]) -> Result<(), Error> {
        init(ctx, decoded::<Genesis>(MODULE, "Genesis", params)?);
        Ok(())
    }

    fn execute(ctx: &ExecCtx, op: Op) -> Result<(), Error> {
        fold(ctx, ctx.env().height)?;
        match op {
            Op::Publish { body } => publish(ctx, body),
            Op::Schedule(scheduled) => schedule(ctx, scheduled),
            Op::Cancel { height, program } => cancel(ctx, height, program),
        }
    }

    fn query(ctx: &QueryCtx, query: Query) -> Result<Reply, Error> {
        let height = ctx.env().height;
        Ok(match query {
            Query::At(height) => Reply::Programs(at(ctx, height)?),
            Query::Views(height) => Reply::Views(views_at(ctx, height)?),
            Query::Scheduled { page } => Reply::Scheduled(
                SCHEDULE
                    .range(ctx, &page, height)?
                    .map(|((height, _), change)| Scheduled { height, change }),
            ),
            Query::Program(program) => Reply::Program {
                height,
                entry: at(ctx, height)?
                    .into_iter()
                    .find(|entry| entry.program == program),
            },
        })
    }
}

#[cfg(feature = "module")]
guest::export!(Modules);
