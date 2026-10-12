//! The node's `/v1` wire as a client reads it, one copy for every client
//! (the app, the inbox): the borsh types `noded` serves, field for field
//! with core's `crates/noded/src/wire.rs` and the `host` types it names,
//! and, behind the `frame` feature, the signed frame every write and query
//! carries (core `crates/kernel/node/src/frame.rs`). Types and signing
//! only: the HTTP client is each consumer's. Core's `noded` is never
//! linked: it carries the whole kernel.
//!
//! Drift: a client compares [`NODE_CONTRACT`] with [`Status::contract`] at
//! connect, and the byte tests below pin each shape to the bytes core
//! encodes; they are rechecked when the core rev in the workspace moves.
//!
//! `frame` re-exports core's `keyscheme` and `commonware_cryptography` at
//! the rev this workspace pins. A consumer uses these re-exports, never its
//! own pin, so one `KeyScheme` and one `ed25519::PrivateKey` exist in its
//! graph.

use abi::{Outcome, ProgramId, Roles, Root};
use borsh::{BorshDeserialize, BorshSerialize};

#[cfg(feature = "frame")]
mod frame;
#[cfg(feature = "frame")]
pub use frame::{Body, FRAME_NAMESPACE, Frame};
#[cfg(feature = "frame")]
pub use {commonware_cryptography, keyscheme};

/// What a client and the node agree on: [`Status::contract`] equals it.
pub const NODE_CONTRACT: u32 = 3;

/// The most blocks one `/v1/blocks` page answers.
pub const MAX_BLOCKS: u32 = 100;

pub mod route {
    pub const STATUS: &str = "/v1/status";
    pub const SUBMIT: &str = "/v1/submit";
    pub const QUERY: &str = "/v1/query";
    pub const BLOB_GET: &str = "/v1/blob/get";
    pub const CHANGES: &str = "/v1/changes";
    pub const BLOCKS: &str = "/v1/blocks";
    pub const BLOCK: &str = "/v1/block";
    pub const NETWORK: &str = "/v1/network";
}

/// `host::Layer`: which state a read sees.
#[derive(Clone, Copy, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub enum Layer {
    Confirmed,
    Preconfirmed,
}

/// `host::Tip`: a block by height and id, what a frame anchors to.
#[derive(Clone, Copy, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct Tip {
    pub height: u64,
    pub id: [u8; 32],
}

#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct Status {
    pub network: String,
    /// The chain's founding time (its genesis block's), unix ms, not now.
    pub time: u64,
    pub block_time_ms: u64,
    pub epoch_length: u64,
    pub height: u64,
    pub tip: [u8; 32],
    pub root: Root,
    pub epoch: u64,
    pub identity: Vec<u8>,
    pub contract: u32,
    /// The genesis block's digest: with the network's name, the chain's id
    /// (`<network>#<salt>`).
    pub genesis: [u8; 32],
    pub roles: Roles,
}

impl Status {
    pub fn anchor(&self) -> Tip {
        Tip {
            height: self.height,
            id: self.tip,
        }
    }
}

/// A signed frame whose target answers a query, at `layer`.
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct Query {
    pub layer: Layer,
    pub frame: Vec<u8>,
}

/// `host::Receipt`: a run as the node recorded it. A nested receipt's
/// `Applied` and events stand only when every receipt above it was applied
/// too: an ancestor's rejection undid it.
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct Receipt {
    pub program: ProgramId,
    pub outcome: Outcome,
    pub events: Vec<Vec<u8>>,
    /// The runs this one's messages caused, in order.
    pub nested: Vec<Receipt>,
}

/// One block's writes to one program, as `/v1/changes/<program>` streams them.
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct Change {
    pub height: u64,
    pub root: Root,
    pub writes: Vec<(Vec<u8>, Option<Vec<u8>>)>,
}

/// A page of finalized blocks, newest first: those below `before` (from the
/// applied tip when `None`), at most `limit` (capped at [`MAX_BLOCKS`]).
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct Blocks {
    pub before: Option<u64>,
    pub limit: u32,
}

/// One finalized block, by height or by its id.
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub enum BlockRef {
    Height(u64),
    Id([u8; 32]),
}

/// One frame a block carries. `receipt` is node-local: `None` where this
/// node never ran the block (below its state-sync anchor) or stopped
/// between applying it and keeping its receipts.
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct Tx {
    pub hash: [u8; 32],
    pub signer: Vec<u8>,
    pub anchor: Tip,
    pub target: String,
    pub payload: Vec<u8>,
    pub receipt: Option<Receipt>,
}

/// A finalized block as the node's archive keeps it.
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct Finalized {
    pub height: u64,
    pub id: [u8; 32],
    pub parent: [u8; 32],
    pub time: u64,
    pub epoch: u64,
    pub proposer: Option<Vec<u8>>,
    pub txs: Vec<Tx>,
}

/// Every member of the current epoch as the node sees it (`/v1/network`):
/// its applied tip when it answered, and the members in key order.
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct Network {
    pub height: u64,
    pub members: Vec<PeerStatus>,
}

/// One member: the newest block the node applied that this key sent a
/// finalize vote for; `None` for a resident, a validator not heard yet, and
/// every member while the node itself does not validate.
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct PeerStatus {
    pub key: Vec<u8>,
    pub signed: Option<u64>,
}

#[cfg(test)]
mod tests;
