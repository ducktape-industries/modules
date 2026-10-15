//! Everything harness keeps: the agents it runs, their threads by agent and by place, every
//! transcript frame, input held while a thread's loop runs, sealed request parts, tool calls,
//! each model's context window as the models endpoint reported it, and the frames left out of
//! the model's view because they alone do not fit it.
use abi::role::identity::AccountNumber;
use attribution::Source;
use store::{Item, Map, Set};

use crate::{AgentRow, Body, CallRow, Frame, Sealed, ThreadRow};

pub const AGENTS: Map<AccountNumber, AgentRow> = Map::new("agent/");
pub const LAST: Map<AccountNumber, (Source, u64)> = Map::new("last/");
pub const THREADS: Map<u64, ThreadRow> = Map::new("thread/");
pub const BY_AGENT: Set<(AccountNumber, u64)> = Set::new("by-agent/");
pub const PLACES: Map<(AccountNumber, String), u64> = Map::new("place/");
pub const FRAMES: Map<(u64, u64), Frame> = Map::new("frame/");
pub const HELD: Map<(u64, u64), Body> = Map::new("held/");
pub const SEALED: Map<(u64, u64), Sealed> = Map::new("sealed/");
pub const CALLS: Map<(u64, String), CallRow> = Map::new("call/");
pub const WINDOWS: Map<String, Option<u64>> = Map::new("window/");
pub const WITHHELD: Set<(u64, u64)> = Set::new("withheld/");
pub const COUNT: Item<u64> = Item::new("count");

pub(crate) fn newest_first(id: u64) -> u64 {
    u64::MAX - id
}
