//! The program: every op and every query, each handed to its rule.
use ::program::{Error, ExecCtx, Program, QueryCtx};

use crate::rules::{answer, assigned, providers, reassign, request, request_row, serve, serving};
use crate::{Op, PROGRAM, Query, Reply};

pub struct Provider;

impl Program for Provider {
    const NAME: &'static str = PROGRAM;
    type Op = Op;
    type Query = Query;
    type Reply = Reply;

    fn execute(ctx: &ExecCtx, op: Op) -> Result<(), Error> {
        match op {
            Op::Serve { services } => serve(ctx, services),
            Op::Request(asked) => request(ctx, asked),
            Op::Answer { id, answer: given } => answer(ctx, id, given),
            Op::Reassign { id } => reassign(ctx, id),
        }
    }

    fn query(ctx: &QueryCtx, query: Query) -> Result<Reply, Error> {
        match query {
            Query::Assigned { key, page } => Ok(Reply::Requests(assigned(ctx, key, &page)?)),
            Query::Request { id } => Ok(Reply::Request(request_row(ctx, id)?)),
            Query::Providers { page } => Ok(Reply::Providers(providers(ctx, &page)?)),
            Query::Serving { service } => Ok(Reply::Serving(serving(ctx, &service)?)),
        }
    }
}

#[cfg(feature = "program")]
::program::export!(Provider);
