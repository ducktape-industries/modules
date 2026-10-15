// The rules: a lane is listed under a valid name at the next id, changed or unlisted only by
// name, and its id is never handed out again; the 256 ids a lane byte holds are all there are.

use ::program::{
    Error, ExecCtx, QueryCtx, already_exists, capacity, invalid, not_found, unexpected_reply,
};
use store::{Item, Map};

use crate::{Class, Genesis, Lane, LaneId, MAX_NAME, Op, Query, Reply};

const LANES: Map<String, Lane> = Map::new("l/");
/// The next id ever handed out; past `LaneId::MAX` every id is spent.
const NEXT: Item<u16> = Item::new("next");

pub(crate) fn init(ctx: &ExecCtx, genesis: Genesis) -> Result<(), Error> {
    for (name, cap) in genesis.lanes {
        list(ctx, name, cap, 0)?;
    }
    Ok(())
}

pub(crate) fn open(ctx: &ExecCtx, name: String, cap: Class) -> Result<(), Error> {
    list(ctx, name, cap, ctx.env().height)
}

pub(crate) fn set(ctx: &ExecCtx, name: String, cap: Class, enabled: bool) -> Result<(), Error> {
    let set = Lane {
        cap,
        enabled,
        ..listed(ctx, &name)?
    };
    LANES.put(ctx, &name, &set);
    Ok(())
}

pub(crate) fn close(ctx: &ExecCtx, name: String) -> Result<(), Error> {
    listed(ctx, &name)?;
    LANES.remove(ctx, &name);
    Ok(())
}

pub(crate) fn lanes(ctx: &QueryCtx) -> Result<Vec<Lane>, Error> {
    let mut lanes: Vec<Lane> = LANES.all(ctx)?.into_iter().map(|(_, lane)| lane).collect();
    lanes.sort_by_key(|lane| lane.id);
    Ok(lanes)
}

pub(crate) fn lane(ctx: &QueryCtx, name: &str) -> Result<Option<Lane>, Error> {
    LANES.get(ctx, &name.to_owned())
}

/// What `execute` refuses of `op` that a vote can know first: a bad name, a name already
/// listed for `Open`, an unlisted one for `Set` and `Close`. The validators' program asks it
/// at each vote, before counting, under its own ctx, so it reads the table through the halo
/// role. Every id spent is `execute`'s alone to refuse.
pub fn admissible(ctx: &QueryCtx, op: &Op) -> Result<(), Error> {
    match op {
        Op::Open { name, .. } => {
            named(name)?;
            let listed = lane_by_role(ctx, name)?.is_some();
            if listed {
                return Err(already_listed(name));
            }
            Ok(())
        }
        Op::Set { name, .. } | Op::Close { name } => {
            lane_by_role(ctx, name)?.ok_or_else(|| unlisted(name))?;
            Ok(())
        }
    }
}

fn list(ctx: &ExecCtx, name: String, cap: Class, since: u64) -> Result<(), Error> {
    named(&name)?;
    if LANES.has(ctx, &name) {
        return Err(already_listed(&name));
    }
    let next = NEXT.get(ctx)?.unwrap_or(0);
    let Ok(id) = LaneId::try_from(next) else {
        return Err(capacity("every lane id was handed out"));
    };
    let lane = Lane {
        name,
        id,
        cap,
        enabled: true,
        since,
    };
    LANES.put(ctx, &lane.name, &lane);
    NEXT.put(ctx, &(next + 1));
    Ok(())
}

fn listed(ctx: &QueryCtx, name: &str) -> Result<Lane, Error> {
    lane(ctx, name)?.ok_or_else(|| unlisted(name))
}

/// A lane's name: 1 to `MAX_NAME` printable ASCII bytes, so a log line carries it as it is.
fn named(name: &str) -> Result<(), Error> {
    let sized = (1..=MAX_NAME).contains(&name.len());
    let printable = name.bytes().all(|byte| byte.is_ascii_graphic());
    if !(sized && printable) {
        return Err(invalid(format!(
            "a lane's name is 1 to {MAX_NAME} printable ASCII bytes, not {name:?}"
        )));
    }
    Ok(())
}

fn lane_by_role(ctx: &QueryCtx, name: &str) -> Result<Option<Lane>, Error> {
    let role = ctx.env().roles.halo.clone();
    let asked = Query::Lane {
        name: name.to_owned(),
    };
    match ctx.query(role.clone(), &asked)? {
        Reply::Lane(lane) => Ok(lane),
        other => Err(unexpected_reply(&role, "Lane", &other)),
    }
}

fn already_listed(name: &str) -> Error {
    already_exists(format!("the lane {name} is listed"))
}

fn unlisted(name: &str) -> Error {
    not_found(format!("no lane {name}"))
}
