//! Everything egress keeps: each registered service with its owner and assigned nodes, which
//! node serves which service, each service's next turn in the election, and every open request
//! indexed by its node and its deadline.
use borsh::{BorshDeserialize, BorshSerialize};
use store::{Item, Map, Set};

use ::program::Ticket;

use crate::{NodeRow, RequestRow, ServiceRow};

#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct Stored {
    pub row: RequestRow,
    pub ticket: Ticket,
}

pub const SERVICES: Map<String, ServiceRow> = Map::new("service/");
pub const NODES: Map<Vec<u8>, NodeRow> = Map::new("node/");
pub const SERVING: Set<(String, Vec<u8>)> = Set::new("serving/");
pub const TURNS: Map<String, u64> = Map::new("turn/");
pub const REQUESTS: Map<u64, Stored> = Map::new("request/");
pub const ASSIGNED: Set<(Vec<u8>, u64)> = Set::new("assigned/");
pub const DEADLINES: Set<(u64, u64)> = Set::new("deadline/");
pub const COUNT: Item<u64> = Item::new("count");
