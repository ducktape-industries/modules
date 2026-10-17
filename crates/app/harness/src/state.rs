//! Everything harness keeps: model records, threads by agent, each agent's current thread and
//! the thread each place last reached, transcript frames, held input, sealed request parts, tool
//! calls, frames left out of the model's view, and the reply a model call is streaming back.
use abi::role::identity::AccountNumber;
use attribution::Source;
use store::{Item, Map, Set};

use crate::{Body, CallRow, Frame, ModelRow, Partial, Sealed, ThreadRow};

pub const MODELS: Map<String, ModelRow> = Map::new("model/");
pub const LAST: Map<AccountNumber, (Source, u64)> = Map::new("last/");
pub const THREADS: Map<u64, ThreadRow> = Map::new("thread/");
pub const BY_AGENT: Set<(AccountNumber, u64)> = Set::new("by-agent/");
pub const PLACES: Map<(AccountNumber, String), u64> = Map::new("place/");
pub const CURRENT: Map<AccountNumber, u64> = Map::new("current/");
pub const FRAMES: Map<(u64, u64), Frame> = Map::new("frame/");
pub const HELD: Map<(u64, u64), Body> = Map::new("held/");
pub const SEALED: Map<(u64, u64), Sealed> = Map::new("sealed/");
pub const CALLS: Map<(u64, String), CallRow> = Map::new("call/");
pub const WITHHELD: Set<(u64, u64)> = Set::new("withheld/");
pub const PARTIALS: Map<u64, Partial> = Map::new("partial/");
pub const COUNT: Item<u64> = Item::new("count");

pub(crate) fn newest_first(id: u64) -> u64 {
    u64::MAX - id
}
