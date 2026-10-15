//! The program: its one op handed to its rule. It keeps nothing, so it answers no question.
use ::program::{Awaiting, Ctx, Error, Program, QueryCtx};

use crate::rules::send;
use crate::{Op, PROGRAM};

pub struct Push;

impl Program for Push {
    const NAME: &'static str = PROGRAM;
    type Op = Op;
    type Query = ();
    type Reply = ();

    fn query(_ctx: &QueryCtx, (): ()) -> Result<(), Error> {
        Ok(())
    }
}

impl Awaiting for Push {
    async fn execute(ctx: Ctx, op: Op) -> Result<(), Error> {
        match op {
            Op::Send { notices } => send(&ctx, notices).await,
        }
    }
}

#[cfg(feature = "program")]
::program::export_awaiting!(Push);
