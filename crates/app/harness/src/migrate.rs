//! The one-time move of a network whose agents' answer lists sat in identity: the vote copies
//! each program account's list that identity still answers into the access program, under the
//! agent's key here. Deleted once every network ran it.
use ::program::{Error, ExecCtx, Ownership, Reply, unexpected_reply};
use store::PageRequest;

use crate::{PROGRAM, agent_key};

pub(crate) fn migrate(ctx: &ExecCtx) -> Result<(), Error> {
    let env = ctx.env();
    env.authority()?;
    let identity = env.role(abi::role::IDENTITY)?.clone();
    let checker = env.role(abi::role::ACCESS)?.clone();
    let mut page = PageRequest::default();
    loop {
        let asked = identity::Query::List { page: page.clone() };
        let listed = match ctx.query::<identity::Query, identity::Reply>(&identity, &asked)? {
            identity::Reply::Accounts(listed) => listed,
            other => return Err(unexpected_reply(&identity, "Accounts", &other)),
        };
        for account in &listed.items {
            let managed = matches!(account.control, identity::Control::Program { .. });
            if !managed {
                continue;
            }
            let asked = identity::Query::Ownership {
                number: account.number,
            };
            let list = match ctx.query::<identity::Query, identity::Reply>(&identity, &asked)? {
                identity::Reply::Ownership(list) => list,
                other => return Err(unexpected_reply(&identity, "Ownership", &other)),
            };
            if list == Ownership::Owner {
                continue;
            }
            let set = access::Op::Set {
                program: PROGRAM.into(),
                key: agent_key(account.number),
                list: Some(list),
            };
            ctx.emit(checker.clone(), &set, Reply::None);
        }
        let Some(next) = listed.next else {
            return Ok(());
        };
        page = PageRequest::resume(Some(next), page.limit());
    }
}
