//! The one-time move of a network whose agents kept whom they answer here, each list stored as
//! an owner, account numbers or anyone: `Query::Ownership` answers the old list until harness
//! copies it into the access program, then the vote's `Migrate` drops the rows. Deleted once
//! every network ran it.
use ::program::{Error, ExecCtx, Ownership, Party, QueryCtx};
use borsh::{BorshDeserialize, BorshSerialize};
use store::Map;

use crate::AccountNumber;
use crate::rules::account;

#[derive(BorshSerialize, BorshDeserialize)]
pub(crate) enum Stored {
    Owner,
    Accounts(Vec<AccountNumber>),
    Anyone,
}

pub(crate) const OWNERSHIPS: Map<AccountNumber, Stored> = Map::new("u/");

pub(crate) fn ownership(ctx: &QueryCtx, number: AccountNumber) -> Result<Ownership, Error> {
    account(ctx, number)?;
    let listed = match OWNERSHIPS.get(ctx, &number)? {
        None | Some(Stored::Owner) => Ownership::Owner,
        Some(Stored::Accounts(accounts)) => {
            Ownership::Listed(accounts.into_iter().map(Party::Account).collect())
        }
        Some(Stored::Anyone) => Ownership::Anyone,
    };
    Ok(listed)
}

pub(crate) fn migrate(ctx: &ExecCtx) -> Result<(), Error> {
    ctx.env().authority()?;
    for (number, _) in OWNERSHIPS.all(ctx)? {
        OWNERSHIPS.remove(ctx, &number);
    }
    Ok(())
}
