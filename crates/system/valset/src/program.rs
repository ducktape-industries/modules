//! The module: every op and every query, each handed to its rule.

use guest::{Error, ExecCtx, Module, Program, QueryCtx, decoded};

use crate::rules::{MEMBERS, execute, init, memberships, validators};
use crate::{Genesis, MODULE, Membership, Op, Query, Reply};

pub struct Valset;

impl Program for Valset {
    const NAME: &'static str = MODULE;
    type Op = Op;
    type Query = Query;
    type Reply = Reply;
}

impl Module for Valset {
    fn init(ctx: &ExecCtx, params: &[u8]) -> Result<(), Error> {
        init(ctx, decoded::<Genesis>(MODULE, "Genesis", params)?)
    }

    fn execute(ctx: &ExecCtx, op: Op) -> Result<(), Error> {
        execute(ctx, op)
    }

    fn query(ctx: &QueryCtx, query: Query) -> Result<Reply, Error> {
        Ok(match query {
            Query::Validators => Reply::Validators(validators(ctx)?),
            Query::Members => {
                Reply::Members(memberships(ctx)?.iter().map(Membership::member).collect())
            }
            Query::Memberships { page } => {
                Reply::Memberships(MEMBERS.range(ctx, &page)?.map(|(_, membership)| membership))
            }
            Query::Membership { key } => Reply::Membership(MEMBERS.get(ctx, &key)?),
        })
    }
}

#[cfg(feature = "module")]
guest::export!(Valset);
