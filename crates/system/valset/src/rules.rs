// The rules: who enrolls and leaves, who votes for which validator, what a validators' vote
// enacts, the last validator kept seated, and no more members than the cap the network was
// founded with.

use ::program::{
    AccountNumber, Error, ExecCtx, Principal, QueryCtx, Reply, Scheme, capacity, code, corrupt,
    invalid, not_found, unauthorized, unexpected_reply, wrong_state,
};
use abi::role::identity::{self, Kind, Standing};
use store::{Item, Map, PageRequest, Set};

use crate::{Genesis, INVITE_NAMESPACE, Invite, Membership, Motion, Op, Pending, Role, Voted};

pub(crate) const MEMBERS: Map<Vec<u8>, Membership> = Map::new("m/");
const CAP: Item<u32> = Item::new("cap");
const DOOR: Item<bool> = Item::new("door");
const SPENT: Set<Vec<u8>> = Set::new("spent/");
const VOTES: Map<Vec<u8>, Vec<Vec<u8>>> = Map::new("votes/");
const KEY_LEN: usize = 32;

pub(crate) fn init(ctx: &ExecCtx, genesis: Genesis) -> Result<(), Error> {
    CAP.put(ctx, &genesis.member_cap);
    for member in genesis.validators {
        let membership = Membership {
            key: member.key,
            address: member.address,
            role: Role::Validator,
        };
        fits(ctx, &membership.key)?;
        MEMBERS.put(ctx, &membership.key, &membership);
    }
    Ok(())
}

pub(crate) fn execute(ctx: &ExecCtx, op: Op) -> Result<(), Error> {
    let signer = ctx.env().signer()?;
    match op {
        Op::Enroll { address, invite } => enroll(ctx, signer, address, invite),
        Op::Leave => leave(ctx, signer),
        Op::Vote(motion) => vote(ctx, signer, motion),
    }
}

pub(crate) fn memberships(ctx: &QueryCtx) -> Result<Vec<Membership>, Error> {
    Ok(MEMBERS
        .all(ctx)?
        .into_iter()
        .map(|(_, membership)| membership)
        .collect())
}

pub(crate) fn validators(ctx: &QueryCtx) -> Result<Vec<Vec<u8>>, Error> {
    Ok(memberships(ctx)?
        .into_iter()
        .filter(|membership| membership.role == Role::Validator)
        .map(|membership| membership.key)
        .collect())
}

fn enroll(
    ctx: &ExecCtx,
    key: Vec<u8>,
    address: Option<String>,
    invite: Option<Invite>,
) -> Result<(), Error> {
    if let Some(held) = MEMBERS.get(ctx, &key)? {
        MEMBERS.put(ctx, &key, &Membership { address, ..held });
        return Ok(());
    }
    fits(ctx, &key)?;
    admitted(ctx, invite)?;
    let membership = Membership {
        key,
        address,
        role: Role::Resident,
    };
    MEMBERS.put(ctx, &membership.key, &membership);
    Ok(())
}

fn fits(ctx: &QueryCtx, key: &[u8]) -> Result<(), Error> {
    let key_is_ed25519 = key.len() == KEY_LEN;
    if !key_is_ed25519 {
        return Err(invalid("a member key is a 32-byte ed25519 public key"));
    }
    room(ctx)
}

fn admitted(ctx: &ExecCtx, invite: Option<Invite>) -> Result<(), Error> {
    if door_open(ctx)? {
        return Ok(());
    }
    let Some(invite) = invite else {
        return Err(unauthorized(
            "the door is closed: enrolling takes an invite",
        ));
    };
    redeem(ctx, &invite)
}

fn redeem(ctx: &ExecCtx, invite: &Invite) -> Result<(), Error> {
    let env = ctx.env();
    let grant = &invite.grant;
    let for_this_network = grant.network == env.chain_id;
    if !for_this_network {
        return Err(invalid("the invite names another network"));
    }
    let expired = grant.expires <= env.time;
    if expired {
        return Err(invalid("the invite expired"));
    }
    let signed = ctx.verify(
        Scheme::Ed25519,
        invite.issuer.clone(),
        INVITE_NAMESPACE,
        grant.preimage(),
        invite.signature.clone(),
    )?;
    if !signed {
        return Err(unauthorized("the invite's signature does not verify"));
    }
    let issued_by_validator = holds(ctx, &invite.issuer, Role::Validator)?;
    if !issued_by_validator {
        return Err(unauthorized("the invite's issuer is not a validator"));
    }
    let nonce = [invite.issuer.as_slice(), &grant.nonce].concat();
    if SPENT.has(ctx, &nonce) {
        return Err(wrong_state("the invite was already redeemed"));
    }
    SPENT.insert(ctx, &nonce);
    Ok(())
}

fn leave(ctx: &ExecCtx, key: Vec<u8>) -> Result<(), Error> {
    let held = member(ctx, &key)?;
    unseatable(ctx, &held)?;
    remove(ctx, key);
    Ok(())
}

fn vote(ctx: &ExecCtx, signer: Vec<u8>, motion: Motion) -> Result<(), Error> {
    // first, so a motion that cannot pass asks identity nothing
    admissible(ctx, &motion)?;
    let validators = validators(ctx)?;
    let sender = ctx.env().sender.as_ref().and_then(Principal::account);
    let voices = voices(ctx, &validators, &signer, || Ok(sender))?;
    if voices.is_empty() {
        return Err(unauthorized(
            "only a validator, or the manager of one, votes",
        ));
    }
    let record = abi::encode(&motion);
    let mut voters = VOTES.get(ctx, &record)?.unwrap_or_default();
    voters.retain(|held| validators.contains(held));
    for voice in voices {
        if !voters.contains(&voice) {
            voters.push(voice);
        }
    }
    let needed = quorum(validators.len());
    let passed = voters.len() >= needed;
    if !passed {
        VOTES.put(ctx, &record, &voters);
        let counted = Voted::Counted {
            votes: voters.len() as u64,
            needed: needed as u64,
        };
        ctx.set_return_data(abi::encode(&counted));
        return Ok(());
    }
    VOTES.remove(ctx, &record);
    enact(ctx, motion)?;
    ctx.set_return_data(abi::encode(&Voted::Enacted));
    Ok(())
}

/// The validators a frame signed by `signer` votes for: itself when it is
/// one, asking identity nothing; else each validator whose node key acts
/// as an agent that `me` (the frame's account, asked only here) manages
/// and keeps active. A node key sits on an agent, never on its operator's
/// own account, so the operator's devices vote through the agent.
pub(crate) fn voices(
    ctx: &QueryCtx,
    validators: &[Vec<u8>],
    signer: &[u8],
    me: impl FnOnce() -> Result<Option<AccountNumber>, Error>,
) -> Result<Vec<Vec<u8>>, Error> {
    let seated = validators.iter().any(|validator| validator == signer);
    if seated {
        return Ok(vec![signer.to_vec()]);
    }
    let Some(me) = me()? else {
        return Ok(Vec::new());
    };
    let mut voices = Vec::new();
    for validator in validators {
        if managed_by(ctx, validator, me)? {
            voices.push(validator.clone());
        }
    }
    Ok(voices)
}

/// Whether `key` acts as an agent `manager` manages and keeps active, as
/// the identity role says.
fn managed_by(ctx: &QueryCtx, key: &[u8], manager: AccountNumber) -> Result<bool, Error> {
    let Some(account) = account_of(ctx, key)? else {
        return Ok(false);
    };
    let bound = &ctx.env().roles.identity;
    let profile = match ctx.query(bound, &identity::Query::Profile(account))? {
        identity::Reply::Profile(profile) => profile,
        other => return Err(unexpected_reply(bound, "Profile", &other)),
    };
    Ok(profile.is_some_and(|profile| {
        matches!(
            profile.kind,
            Kind::Managed { manager: by, standing: Standing::Active, .. } if by == manager
        )
    }))
}

/// The account `key` acts as, as the identity role answers it: `None` for
/// a key that holds none, or whose account does not act now (identity
/// refuses it `unauthorized`). Any other refusal fails the caller, so an
/// identity that misanswers drops no one's voice quietly.
pub(crate) fn account_of(ctx: &QueryCtx, key: &[u8]) -> Result<Option<AccountNumber>, Error> {
    let bound = &ctx.env().roles.identity;
    match ctx.query(bound, &identity::Query::Account(key.to_vec())) {
        Ok(identity::Reply::Account(account)) => Ok(account),
        Ok(other) => Err(unexpected_reply(bound, "Account", &other)),
        Err(refusal) if refusal.code == code::UNAUTHORIZED => Ok(None),
        Err(refusal) => Err(refusal),
    }
}

/// A page of the motions waiting for votes, each with its voters still
/// seated, as [`vote`] tallies them. A `programs` op too late to land (what
/// [`timely`] refuses) is left out before the page is cut, so it takes no
/// slot of the page; a vote can never clear it, since a refused frame
/// writes nothing.
pub(crate) fn motions(ctx: &QueryCtx, page: &PageRequest) -> Result<crate::Reply, Error> {
    let validators = validators(ctx)?;
    let prefix = VOTES.key(&());
    let listing = page.listing(ctx, prefix.clone())?;
    let mut scan = listing.scan(&prefix);
    // ponytail: reads every motion past the cursor to fill one page; a
    // handful wait at once (each needs a validator's vote to be stored)
    scan.limit = None;
    let mut rows = Vec::new();
    for (record, mut voters) in VOTES.scan(ctx, scan)? {
        let motion: Motion =
            abi::decode(&record).map_err(|refusal| corrupt("votes/", &record, refusal.sentence))?;
        if let Motion::Programs { op } = &motion
            && programs_motion(ctx, op).is_err()
        {
            continue;
        }
        voters.retain(|voter| validators.contains(voter));
        rows.push((VOTES.key(&record), Pending { motion, voters }));
    }
    Ok(crate::Reply::Motions {
        needed: quorum(validators.len()) as u64,
        page: listing.reply(rows),
    })
}

fn admissible(ctx: &QueryCtx, motion: &Motion) -> Result<(), Error> {
    match motion {
        Motion::Promote { key } => promotable(ctx, key),
        Motion::Demote { key } => demotable(ctx, key),
        Motion::Remove { key } => removable(ctx, key),
        Motion::Door { open } => door_moves(ctx, *open),
        Motion::Programs { op } => {
            let op = programs_motion(ctx, op)?;
            grantable(ctx, &op)
        }
    }
}

fn programs_motion(ctx: &QueryCtx, op: &[u8]) -> Result<programs::Op, Error> {
    let op = programs_op(op)?;
    votable(&op)?;
    timely(ctx, &op)?;
    Ok(op)
}

fn votable(op: &programs::Op) -> Result<(), Error> {
    match op {
        programs::Op::Publish { .. } => Err(invalid(
            "a publish is a frame of its own that attaches the code: the vote schedules published code by its id",
        )),
        programs::Op::Schedule(_)
        | programs::Op::Cancel { .. }
        | programs::Op::Grant(_)
        | programs::Op::Revoke(_) => Ok(()),
    }
}

/// A `Programs` motion's op, as the programs role reads it.
fn programs_op(op: &[u8]) -> Result<programs::Op, Error> {
    abi::decode(op).map_err(|refusal| {
        invalid(format!(
            "not an op of the programs role: {}",
            refusal.sentence
        ))
    })
}

/// A `Programs` motion's `Schedule` or `Cancel` names a later block than
/// this one: a change lands at a later block (`programs`'s
/// `schedule`), and one whose block has come has landed, so there is
/// nothing left to cancel. Refused at the first vote: refused only at
/// enactment, the vote that reached the quorum would fail inside the
/// programs, and the motion would sit one vote short for good.
fn timely(ctx: &QueryCtx, op: &programs::Op) -> Result<(), Error> {
    let at = match op {
        programs::Op::Publish { .. } | programs::Op::Grant(_) | programs::Op::Revoke(_) => {
            return Ok(());
        }
        programs::Op::Schedule(scheduled) => scheduled.height,
        programs::Op::Cancel { height, .. } => *height,
    };
    let height = ctx.env().height;
    if at <= height {
        return Err(invalid(format!(
            "too late: block {at} is not past block {height}"
        )));
    }
    Ok(())
}

/// A `Grant` the programs role would refuse (`programs::grantable`: a
/// bad id, a role, no such account, a program's), refused at the first
/// vote for `timely`'s reason. Asked here, not in `timely`, so reading the
/// motions asks identity nothing: what it checks never changes once true.
fn grantable(ctx: &QueryCtx, op: &programs::Op) -> Result<(), Error> {
    match op {
        programs::Op::Grant(grant) => programs::grantable(ctx, grant),
        _ => Ok(()),
    }
}

fn enact(ctx: &ExecCtx, motion: Motion) -> Result<(), Error> {
    match motion {
        Motion::Promote { key } => reseat(ctx, key, Role::Validator),
        Motion::Demote { key } => reseat(ctx, key, Role::Resident),
        Motion::Remove { key } => {
            remove(ctx, key);
            Ok(())
        }
        Motion::Door { open } => {
            DOOR.put(ctx, &open);
            Ok(())
        }
        Motion::Programs { op } => {
            ctx.emit_raw(ctx.env().roles.programs.clone(), op, Reply::None);
            Ok(())
        }
    }
}

fn promotable(ctx: &QueryCtx, key: &[u8]) -> Result<(), Error> {
    holding(ctx, key, Role::Resident).map(drop)
}

fn demotable(ctx: &QueryCtx, key: &[u8]) -> Result<(), Error> {
    let held = holding(ctx, key, Role::Validator)?;
    unseatable(ctx, &held)
}

fn removable(ctx: &QueryCtx, key: &[u8]) -> Result<(), Error> {
    let held = member(ctx, key)?;
    unseatable(ctx, &held)
}

fn member(ctx: &QueryCtx, key: &[u8]) -> Result<Membership, Error> {
    MEMBERS
        .get(ctx, &key.to_vec())?
        .ok_or_else(|| not_found("the key is not a member"))
}

fn holds(ctx: &QueryCtx, key: &[u8], role: Role) -> Result<bool, Error> {
    let held = MEMBERS.get(ctx, &key.to_vec())?;
    Ok(held.is_some_and(|held| held.role == role))
}

fn holding(ctx: &QueryCtx, key: &[u8], role: Role) -> Result<Membership, Error> {
    let held = member(ctx, key)?;
    let holds_it = held.role == role;
    if !holds_it {
        return Err(wrong_state(format!(
            "the member is a {:?}, not a {role:?}",
            held.role
        )));
    }
    Ok(held)
}

fn unseatable(ctx: &QueryCtx, held: &Membership) -> Result<(), Error> {
    let seated = held.role == Role::Validator;
    let last = seated && validators(ctx)?.len() == 1;
    if last {
        return Err(wrong_state("the last validator cannot be unseated"));
    }
    Ok(())
}

fn door_moves(ctx: &QueryCtx, open: bool) -> Result<(), Error> {
    let unchanged = door_open(ctx)? == open;
    if unchanged {
        return Err(wrong_state(format!(
            "the door is already {}",
            door_word(open)
        )));
    }
    Ok(())
}

fn reseat(ctx: &ExecCtx, key: Vec<u8>, role: Role) -> Result<(), Error> {
    let held = member(ctx, &key)?;
    MEMBERS.put(ctx, &key, &Membership { role, ..held });
    Ok(())
}

fn remove(ctx: &ExecCtx, key: Vec<u8>) {
    for motion in [
        Motion::Promote { key: key.clone() },
        Motion::Demote { key: key.clone() },
        Motion::Remove { key: key.clone() },
    ] {
        VOTES.remove(ctx, &abi::encode(&motion));
    }
    MEMBERS.remove(ctx, &key);
}

fn room(ctx: &QueryCtx) -> Result<(), Error> {
    let cap = CAP
        .get(ctx)?
        .ok_or_else(|| corrupt("cap", b"", "valset records no member cap"))?;
    if MEMBERS.all(ctx)?.len() >= cap as usize {
        return Err(capacity(format!(
            "the network is full: it holds at most {cap} members"
        )));
    }
    Ok(())
}

fn door_open(ctx: &QueryCtx) -> Result<bool, Error> {
    Ok(DOOR.get(ctx)?.unwrap_or(false))
}

fn quorum(validators: usize) -> usize {
    validators - (validators - 1) / 3
}

fn door_word(open: bool) -> &'static str {
    match open {
        true => "open",
        false => "closed",
    }
}
