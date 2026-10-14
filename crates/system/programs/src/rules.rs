// The rules: the schedule folded at each block, then the ops and queries.
// A change is made by the validators' vote or by a maintainer's grant
// (`actor`); every cascade a change causes happens in its own op, after
// its last check, never at `fold`: `At(h)` reads `roster` unfolded, so a
// fold-time filter would make what the kernel runs disagree with the state.

use std::collections::{BTreeMap, BTreeSet};

use ::program::{AccountNumber, BlobId, Env, Origin, Principal, ProgramId};
use ::program::{
    Error, ExecCtx, QueryCtx, already_exists, capacity, invalid, not_found, unauthorized,
    unexpected_reply, wrong_state,
};
use abi::role::identity;
use store::{Item, KeyCodec, Map, Set};

use crate::{
    CODE_KIND, Change, Entry, Genesis, Grant, MAX_PENDING, NOTICE, Scheduled, Scope, View,
    grant_refused, role_bound,
};

const PROGRAMS: Map<ProgramId, Entry> = Map::new("p/");
const VIEWS: Map<ProgramId, View> = Map::new("v/");
const HOOKS: Set<ProgramId> = Set::new("h/");
type At = (u64, ProgramId);
/// The value stays a bare `Change`, so a `programs` swapped in over pending
/// entries still reads them; who made one is `BY`.
pub(crate) const SCHEDULE: Map<At, Change> = Map::new("s/");
const FOLDED: Item<u64> = Item::new("folded");
/// A grant as its table keys it: by scope, then account.
type Held = (Scope, AccountNumber);
pub(crate) const GRANTS: Set<Held> = Set::new("g/");
/// The account whose grant made a pending change; none: the vote made it.
pub(crate) const BY: Map<At, AccountNumber> = Map::new("b/");

/// A scope in a table key: a tag byte, then a program's id; grants list by
/// program name, then the ones of every program.
impl KeyCodec for Scope {
    fn encode_key(&self, out: &mut Vec<u8>) {
        match self {
            Scope::Program(id) => {
                out.push(0);
                id.encode_key(out);
            }
            Scope::All => out.push(1),
        }
    }
    fn decode_key(bytes: &mut &[u8]) -> Option<Self> {
        let (tag, rest) = bytes.split_first()?;
        *bytes = rest;
        match tag {
            0 => ProgramId::decode_key(bytes).map(Scope::Program),
            1 => Some(Scope::All),
            _ => None,
        }
    }
}

pub(crate) fn init(ctx: &ExecCtx, genesis: Genesis) {
    for entry in genesis.programs {
        PROGRAMS.put(ctx, &entry.program, &entry);
    }
    for view in genesis.views {
        VIEWS.put(ctx, &view.name, &view);
    }
    for hook in genesis.hooks {
        HOOKS.insert(ctx, &hook);
    }
    FOLDED.put(ctx, &0);
}

fn held_code(ctx: &QueryCtx, code: BlobId) -> Result<(), Error> {
    let role = ctx.env().roles.blobs.clone();
    let holding = match ctx.query(role.clone(), &blob::contract::Query::Held { blob: code })? {
        blob::contract::Reply::Held(holding) => holding,
        other => return Err(unexpected_reply(&role, "Held", &other)),
    };
    let Some(holding) = holding else {
        return Err(not_found(format!(
            "code {code:?} is not held: put it as a {CODE_KIND:?} blob and wait for a quorum of validators to hold it"
        )));
    };
    let program = holding.kind == CODE_KIND;
    if !program {
        return Err(invalid(format!(
            "code {code:?} is held as a {:?} blob, not {CODE_KIND:?}",
            holding.kind
        )));
    }
    Ok(())
}

/// Who changes `name` in this frame: the validators' vote (`None`), or the
/// account a signed frame acts as (`Some`), which still has to hold a grant.
/// A program's message never acts by grant: its sender is its own account.
fn actor(env: &Env, name: &str) -> Result<Option<AccountNumber>, Error> {
    if env.authority().is_ok() {
        return Ok(None);
    }
    let signed = matches!(env.origin, Origin::Signed(_));
    match env.sender.as_ref().and_then(Principal::account) {
        Some(account) if signed => Ok(Some(account)),
        _ => Err(unauthorized(format!(
            "only a signed frame of an account with a grant, or the validators' vote, may change {name}"
        ))),
    }
}

/// Refused unless `account` maintains `name`: a grant of `name` or of every
/// program, and `name` fills no role.
fn maintains(ctx: &QueryCtx, account: AccountNumber, name: &str) -> Result<(), Error> {
    if role_bound(&ctx.env().roles, name) {
        return Err(unauthorized(format!(
            "{name} fills a role the kernel calls: only the validators' vote changes it"
        )));
    }
    let held = [Scope::All, Scope::Program(name.to_owned())]
        .into_iter()
        .any(|scope| GRANTS.has(ctx, &(scope, account)));
    if !held {
        return Err(unauthorized(format!(
            "account {account} may not change {name}: no grant, and not the validators' vote"
        )));
    }
    Ok(())
}

/// A maintainer's change, before the checks every change meets: no hook,
/// a name it maintains, [`NOTICE`] blocks ahead, no change of the vote's
/// waiting on the name, no claim on a name a maintainer's removal frees,
/// and fewer than [`MAX_PENDING`] of its own.
fn by_grant(ctx: &QueryCtx, account: AccountNumber, scheduled: &Scheduled) -> Result<(), Error> {
    let name = scheduled.change.program();
    if matches!(scheduled.change, Change::SetHook(_) | Change::RemoveHook(_)) {
        return Err(unauthorized(format!(
            "only the validators' vote sets or removes a hook, as on {name}"
        )));
    }
    maintains(ctx, account, name)?;
    let earliest = ctx.env().height.checked_add(NOTICE);
    if earliest.is_none_or(|earliest| scheduled.height < earliest) {
        let min = earliest.map_or_else(|| "past the last".into(), |h| h.to_string());
        return Err(invalid(format!(
            "a change by a maintainer lands at block {min} or later"
        )));
    }
    if let Some(height) = voted(ctx, name)? {
        return Err(wrong_state(format!(
            "the validators' vote changes {name} at {height}: wait until it lands or is cancelled"
        )));
    }
    // a cascade drops a maintainer's removal with no guard, so a name it
    // frees is claimed only once it lands: never a program and a view at once
    let frees: Option<fn(&Change) -> bool> = match scheduled.change {
        Change::Set(_) => Some(|change| matches!(change, Change::RemoveView(_))),
        Change::SetView(_) => Some(|change| matches!(change, Change::Remove(_))),
        _ => None,
    };
    if let Some(frees) = frees
        && waits(ctx, name, &[], frees)?
    {
        return Err(wrong_state(format!(
            "{name} is held until a maintainer's removal lands: wait, or cancel it"
        )));
    }
    // a row with no change behind it (a `programs` rolled back) holds nothing
    let waiting = BY
        .all(ctx)?
        .iter()
        .filter(|(at, by)| *by == account && SCHEDULE.has(ctx, at))
        .count();
    if waiting >= MAX_PENDING {
        return Err(capacity(format!(
            "account {account} has {MAX_PENDING} changes waiting"
        )));
    }
    Ok(())
}

/// The height of a change the vote made to `name` that still waits.
fn voted(ctx: &QueryCtx, name: &str) -> Result<Option<u64>, Error> {
    Ok(SCHEDULE
        .all(ctx)?
        .into_iter()
        .map(|(key, _)| key)
        .find(|key| key.1 == name && !BY.has(ctx, key))
        .map(|key| key.0))
}

pub(crate) fn schedule(ctx: &ExecCtx, scheduled: Scheduled) -> Result<(), Error> {
    let env = ctx.env();
    let by = actor(&env, scheduled.change.program())?;
    // the vote's change on a name takes every change a grant made to it
    // along, and its removal the name's grants too: the checks below read
    // the schedule without them, and they go once every check has passed
    let (grants, changes) = match by {
        Some(account) => {
            by_grant(ctx, account, &scheduled)?;
            (Vec::new(), Vec::new())
        }
        None => overruled(ctx, &scheduled.change)?,
    };
    let in_the_future = scheduled.height > env.height;
    if !in_the_future {
        return Err(invalid(format!(
            "a change lands at a later block than {}",
            env.height
        )));
    }
    if let Some(code) = scheduled.change.code() {
        held_code(ctx, code)?;
    }
    let name = scheduled.change.program();
    // an id lands in its one spelling (`program::is_name`), so no id reads
    // as another's; a removal names what is there, however it is spelled
    let lands = matches!(scheduled.change, Change::Set(_) | Change::SetView(_));
    if lands && !::program::is_name(name) {
        return Err(invalid(format!(
            "{name:?} is not a program id: 1..=64 of [a-z0-9_-]"
        )));
    }
    // the kernel calls these by their genesis binding: with one gone every
    // frame is refused (identity) or the chain halts (validators), and no
    // change could land to bring it back
    let bound = role_bound(&env.roles, name);
    if matches!(scheduled.change, Change::Remove(_)) && bound {
        return Err(invalid(format!("{name} fills a role the kernel calls")));
    }
    let Roster {
        programs,
        views,
        hooks,
    } = roster(ctx, scheduled.height, &changes)?;
    // the vote's removal of a name nothing holds by then has its goal met:
    // it takes the name's grants along and waits as no entry
    let gone = by.is_none()
        && matches!(scheduled.change, Change::Remove(_) | Change::RemoveView(_))
        && !programs.contains_key(name)
        && !views.contains_key(name);
    let clash = match &scheduled.change {
        Change::Set(_) => {
            views.contains_key(name)
                || waits(ctx, name, &changes, |c| matches!(c, Change::SetView(_)))?
        }
        Change::SetView(_) => {
            programs.contains_key(name)
                || waits(ctx, name, &changes, |c| matches!(c, Change::Set(_)))?
        }
        _ if gone => false,
        Change::Remove(_) | Change::SetHook(_) if !programs.contains_key(name) => {
            return Err(not_found(format!(
                "no program {name} runs at {}",
                scheduled.height
            )));
        }
        Change::RemoveView(_) if !views.contains_key(name) => {
            return Err(not_found(format!(
                "no view {name} is listed at {}",
                scheduled.height
            )));
        }
        Change::RemoveHook(_) if !hooks.contains(name) => {
            return Err(not_found(format!(
                "{name} is no hook at {}",
                scheduled.height
            )));
        }
        Change::Remove(_) | Change::RemoveView(_) | Change::SetHook(_) | Change::RemoveHook(_) => {
            false
        }
    };
    if clash {
        return Err(already_exists(format!(
            "{name} already names a program or a view"
        )));
    }
    // a removal by grant would drop the vote's hook for good: a program
    // set again is no hook again
    let unhooks = matches!(scheduled.change, Change::Remove(_)) && hooks.contains(name);
    if by.is_some() && unhooks {
        return Err(unauthorized(format!(
            "{name} is a hook at {}: only the validators' vote removes it",
            scheduled.height
        )));
    }
    let key = (scheduled.height, scheduled.change.program().to_owned());
    if !gone && SCHEDULE.has(ctx, &key) && !changes.contains(&key) {
        return Err(already_exists(format!(
            "{} already changes at {}",
            key.1, key.0
        )));
    }
    grants.iter().for_each(|grant| GRANTS.remove(ctx, grant));
    // first, so a maker left at this very key (a `programs` rolled back)
    // takes nothing of the vote's own entry along
    changes.iter().for_each(|at| drop_change(ctx, at));
    if gone {
        return Ok(());
    }
    SCHEDULE.put(ctx, &key, &scheduled.change);
    if let Some(account) = by {
        BY.put(ctx, &key, &account);
    }
    Ok(())
}

/// What the vote's `change` takes along: every change a grant made to its
/// name, and for a removal every `Program(name)` grant. Read before any write.
fn overruled(ctx: &QueryCtx, change: &Change) -> Result<(Vec<Held>, Vec<At>), Error> {
    let name = change.program();
    let grants = match change {
        Change::Remove(_) | Change::RemoveView(_) => {
            GRANTS.scan(ctx, GRANTS.prefix_of(&Scope::Program(name.to_owned())))?
        }
        _ => Vec::new(),
    };
    let changes = BY
        .all(ctx)?
        .into_iter()
        .map(|(at, _)| at)
        .filter(|at| at.1 == name)
        .collect();
    Ok((grants, changes))
}

/// A pending change gone, with who made it: directly, without `cancel`'s
/// guard on a removal's reinstated name.
fn drop_change(ctx: &ExecCtx, at: &At) {
    SCHEDULE.remove(ctx, at);
    BY.remove(ctx, at);
}

/// The vote makes a grant, once identity profiles its account.
pub(crate) fn grant(ctx: &ExecCtx, grant: Grant) -> Result<(), Error> {
    ctx.env().authority()?;
    grantable(ctx, &grant)?;
    GRANTS.insert(ctx, &(grant.scope, grant.account));
    Ok(())
}

/// Refused unless the vote may make `grant` ([`grant_refused`]), asking
/// identity for its account's profile: the rule's check, and valset's at
/// the first vote.
pub fn grantable(ctx: &QueryCtx, grant: &Grant) -> Result<(), Error> {
    let bound = &ctx.env().roles.identity;
    let asked = identity::Query::Profile(grant.account);
    let profile = match ctx.query(bound, &asked)? {
        identity::Reply::Profile(profile) => profile,
        other => return Err(unexpected_reply(bound, "Profile", &other)),
    };
    match grant_refused(grant, &ctx.env().roles, profile.as_ref()) {
        Some(refusal) => Err(invalid(refusal)),
        None => Ok(()),
    }
}

/// The vote takes a grant back, or its holder steps down: a grant not held
/// is no error, so a vote never sits one short of a done thing. Every
/// change the account made that it no longer maintains goes with it.
pub(crate) fn revoke(ctx: &ExecCtx, grant: Grant) -> Result<(), Error> {
    let env = ctx.env();
    let account = grant.account;
    if let Some(by) = actor(&env, &grant.scope.to_string())?
        && by != account
    {
        return Err(unauthorized(format!(
            "account {by} may step down only from its own grant, not account {account}'s"
        )));
    }
    let made = BY.all(ctx)?;
    GRANTS.remove(ctx, &(grant.scope, account));
    for (at, by) in made {
        let still = [Scope::All, Scope::Program(at.1.clone())]
            .into_iter()
            .any(|scope| GRANTS.has(ctx, &(scope, account)));
        if by == account && !still {
            drop_change(ctx, &at);
        }
    }
    Ok(())
}

/// The vote cancels any change; a maintainer of the name, one a grant made.
pub(crate) fn cancel(ctx: &ExecCtx, height: u64, program: ProgramId) -> Result<(), Error> {
    let by = actor(&ctx.env(), &program)?;
    if let Some(account) = by {
        maintains(ctx, account, &program)?;
    }
    let key = (height, program);
    let Some(change) = SCHEDULE.get(ctx, &key)? else {
        // as with Revoke: a vote never sits one short of a done thing
        if by.is_none() {
            return Ok(());
        }
        return Err(not_found(format!("{} does not change at {height}", key.1)));
    };
    if by.is_some() && !BY.has(ctx, &key) {
        return Err(unauthorized(format!(
            "the validators' vote changes {} at {height}: only the vote cancels it",
            key.1
        )));
    }
    // A removal cancelled keeps its name held, which a pending set of the
    // other kind may since have claimed.
    let reinstated: Option<fn(&Change) -> bool> = match change {
        Change::Remove(_) => Some(|change| matches!(change, Change::SetView(_))),
        Change::RemoveView(_) => Some(|change| matches!(change, Change::Set(_))),
        Change::Set(_) | Change::SetView(_) | Change::SetHook(_) | Change::RemoveHook(_) => None,
    };
    if let Some(other) = reinstated
        && waits(ctx, &key.1, &[], other)?
    {
        return Err(already_exists(format!(
            "{} is claimed by a pending change of the other kind",
            key.1
        )));
    }
    drop_change(ctx, &key);
    Ok(())
}

pub(crate) fn fold(ctx: &ExecCtx, height: u64) -> Result<(), Error> {
    let folded = FOLDED.get(ctx)?.unwrap_or(0);
    let nothing_new = folded >= height;
    if nothing_new {
        return Ok(());
    }
    for (key, change) in due(ctx, height)? {
        match &change {
            Change::Set(entry) => PROGRAMS.put(ctx, &entry.program, entry),
            Change::Remove(program) => {
                PROGRAMS.remove(ctx, program);
                HOOKS.remove(ctx, program);
            }
            Change::SetView(view) => VIEWS.put(ctx, &view.name, view),
            Change::RemoveView(name) => VIEWS.remove(ctx, name),
            Change::SetHook(program) => {
                if PROGRAMS.has(ctx, program) {
                    HOOKS.insert(ctx, program);
                }
            }
            Change::RemoveHook(program) => HOOKS.remove(ctx, program),
        }
        drop_change(ctx, &key);
    }
    FOLDED.put(ctx, &height);
    Ok(())
}

fn due(ctx: &QueryCtx, height: u64) -> Result<Vec<(At, Change)>, Error> {
    SCHEDULE.scan(ctx, SCHEDULE.below(&(height + 1)))
}

pub(crate) fn at(ctx: &QueryCtx, height: u64) -> Result<Vec<Entry>, Error> {
    Ok(roster(ctx, height, &[])?.programs.into_values().collect())
}

pub(crate) fn views_at(ctx: &QueryCtx, height: u64) -> Result<Vec<View>, Error> {
    Ok(roster(ctx, height, &[])?.views.into_values().collect())
}

pub(crate) fn hooks_at(ctx: &QueryCtx, height: u64) -> Result<Vec<ProgramId>, Error> {
    Ok(roster(ctx, height, &[])?.hooks.into_iter().collect())
}

pub(crate) struct Roster {
    pub(crate) programs: BTreeMap<ProgramId, Entry>,
    pub(crate) views: BTreeMap<ProgramId, View>,
    pub(crate) hooks: BTreeSet<ProgramId>,
}

/// Every list at a height, by name: what is folded, and every change due
/// by then but the `overruled` ones.
pub(crate) fn roster(ctx: &QueryCtx, height: u64, overruled: &[At]) -> Result<Roster, Error> {
    let mut programs: BTreeMap<_, _> = PROGRAMS.all(ctx)?.into_iter().collect();
    let mut views: BTreeMap<_, _> = VIEWS.all(ctx)?.into_iter().collect();
    let mut hooks: BTreeSet<_> = HOOKS.all(ctx)?.into_iter().collect();
    let due = due(ctx, height)?;
    let due = due.into_iter().filter(|(at, _)| !overruled.contains(at));
    for (_, change) in due {
        match change {
            Change::Set(entry) => {
                programs.insert(entry.program.clone(), entry);
            }
            Change::Remove(program) => {
                programs.remove(&program);
                hooks.remove(&program);
            }
            Change::SetView(view) => {
                views.insert(view.name.clone(), view);
            }
            Change::RemoveView(name) => {
                views.remove(&name);
            }
            Change::SetHook(program) => {
                if programs.contains_key(&program) {
                    hooks.insert(program);
                }
            }
            Change::RemoveHook(program) => {
                hooks.remove(&program);
            }
        }
    }
    Ok(Roster {
        programs,
        views,
        hooks,
    })
}

/// Whether a change to `name` that `this` picks waits anywhere in the
/// schedule, at any height, the `overruled` ones aside.
fn waits(
    ctx: &QueryCtx,
    name: &str,
    overruled: &[At],
    this: fn(&Change) -> bool,
) -> Result<bool, Error> {
    Ok(SCHEDULE
        .all(ctx)?
        .into_iter()
        .any(|(at, change)| at.1 == name && !overruled.contains(&at) && this(&change)))
}
