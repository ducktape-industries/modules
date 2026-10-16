//! Everything egress keeps: each registered service with its owner and assigned nodes, which
//! node serves which service, each service's next turn in the election, and every open request,
//! with the caller's ticket, its wait and its scheduled expiry, indexed by its node.
use borsh::{BorshDeserialize, BorshSerialize};
use store::{Item, Map, Set};

use ::program::Ticket;

use crate::{NodeRow, RequestRow, ServiceRow};

#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct Stored {
    pub row: RequestRow,
    pub ticket: Ticket,
    /// The blocks the request waits for its answer, and an events request for each next
    /// event: the expiry moves this far past every event.
    pub wait: u64,
}

pub const SERVICES: Map<String, ServiceRow> = Map::new("service/");
pub const NODES: Map<Vec<u8>, NodeRow> = Map::new("node/");
pub const SERVING: Set<(String, Vec<u8>)> = Set::new("serving/");
pub const TURNS: Map<String, u64> = Map::new("turn/");
pub const REQUESTS: Map<u64, Stored> = Map::new("request/");
pub const ASSIGNED: Set<(Vec<u8>, u64)> = Set::new("assigned/");
pub const COUNT: Item<u64> = Item::new("count");
