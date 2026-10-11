// The rules: who enrolls and leaves, what a validators' vote enacts, the last validator kept
// seated, and no more members than the cap the network was founded with.

use guest::{
    Error, ExecCtx, QueryCtx, Reply, Scheme, capacity, corrupt, invalid, not_found, unauthorized,
    wrong_state,
};
use store::{Item, Map, Set};

use crate::{Genesis, INVITE_NAMESPACE, Invite, Membership, Motion, Op, Role, Voted};

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
    address: String,
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

fn vote(ctx: &ExecCtx, voter: Vec<u8>, motion: Motion) -> Result<(), Error> {
    let validators = validators(ctx)?;
    let is_validator = validators.contains(&voter);
    if !is_validator {
        return Err(unauthorized("only a validator votes"));
    }
    admissible(ctx, &motion)?;
    let record = abi::encode(&motion);
    let mut voters = VOTES.get(ctx, &record)?.unwrap_or_default();
    voters.retain(|held| validators.contains(held));
    if !voters.contains(&voter) {
        voters.push(voter);
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

fn admissible(ctx: &QueryCtx, motion: &Motion) -> Result<(), Error> {
    match motion {
        Motion::Promote { key } => promotable(ctx, key),
        Motion::Demote { key } => demotable(ctx, key),
        Motion::Remove { key } => removable(ctx, key),
        Motion::Door { open } => door_moves(ctx, *open),
        Motion::Registry { .. } => Ok(()),
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
        Motion::Registry { op } => {
            ctx.emit_raw(ctx.env().roles.registry.clone(), op, Reply::None);
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
