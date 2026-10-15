//! Everything provider keeps: who serves what, each service's next turn in the election,
//! and every open request indexed by its provider.
use store::{Item, Map, Set};

use crate::{ProviderRow, RequestRow};

pub const PROVIDERS: Map<Vec<u8>, ProviderRow> = Map::new("provider/");
pub const SERVING: Set<(String, Vec<u8>)> = Set::new("serving/");
pub const TURNS: Map<String, u64> = Map::new("turn/");
pub const REQUESTS: Map<u64, RequestRow> = Map::new("request/");
pub const ASSIGNED: Set<(Vec<u8>, u64)> = Set::new("assigned/");
pub const COUNT: Item<u64> = Item::new("count");
