//! The module: every frame the kernel tells it of, and every query, each handed to its rule.

use guest::{Error, ExecCtx, Module, Program, QueryCtx, code, hook};

use crate::rules::{acted, keep, signed, used};
use crate::{MODULE, Op, Query, Reply};

pub struct Activity;

impl Program for Activity {
    const NAME: &'static str = MODULE;
    type Op = Op;
    type Query = Query;
    type Reply = Reply;
}

impl Module for Activity {
    fn execute(_ctx: &ExecCtx, (): Op) -> Result<(), Error> {
        Err(Error::new(
            code::UNSUPPORTED,
            "activity takes no op: the kernel tells it of each frame",
        ))
    }

    fn hook(ctx: &ExecCtx, record: hook::Record) -> Result<(), Error> {
        keep(ctx, record)
    }

    fn query(ctx: &QueryCtx, query: Query) -> Result<Reply, Error> {
        Ok(match query {
            Query::Acted { account, page } => Reply::Frames(acted(ctx, account, &page)?),
            Query::Signed { key, page } => Reply::Frames(signed(ctx, key, &page)?),
            Query::Used { page } => Reply::Used(used(ctx, &page)?),
        })
    }
}

#[cfg(feature = "module")]
guest::export!(Activity);
