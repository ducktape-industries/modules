//! The one-time move of a network whose repositories kept their writers in forge: the vote
//! moves each repository's writers into its list in the access program. Deleted once every
//! network ran it.
use std::collections::BTreeMap;

use ::program::{Error, ExecCtx, Ownership, Party, Principal, Reply};
use store::Set;

use crate::contract::repo_key;
use crate::state::load_repo;

const WRITERS: Set<(String, Principal)> = Set::new("w/");

pub(crate) fn migrate(ctx: &ExecCtx) -> Result<(), Error> {
    let env = ctx.env();
    env.authority()?;
    let rows = WRITERS.all(ctx)?;
    let mut lists: BTreeMap<String, Vec<Party>> = BTreeMap::new();
    for (repo, principal) in &rows {
        let owner = load_repo(ctx, repo)?.owner;
        let Some(number) = principal.account().filter(|_| *principal != owner) else {
            continue;
        };
        lists
            .entry(repo.clone())
            .or_default()
            .push(Party::Account(number));
    }
    let access = env.role(abi::role::ACCESS)?.clone();
    for (repo, parties) in lists {
        let set = access::Op::Set {
            program: crate::PROGRAM.into(),
            key: repo_key(&repo),
            list: Some(Ownership::Listed(parties)),
        };
        ctx.emit(access.clone(), &set, Reply::None);
    }
    rows.iter().for_each(|row| WRITERS.remove(ctx, row));
    Ok(())
}
