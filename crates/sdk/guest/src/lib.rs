//! The minimal module SDK. A module is a type implementing [`Program`] (its
//! name and the three types it speaks, the same trait a view names it by)
//! and [`Module`] (what it does with each), and one [`export!`]; its entry points receive an [`ExecCtx`] or a [`QueryCtx`],
//! whose methods are the whole host surface: `env`, `sender` (the account a
//! write acts as, which the host resolved through the identity role), raw
//! state (`get`, `set`, `delete`, `scan`), blobs, `event`, `set_return_data`,
//! `sha256` and `verify`, and the two ways to another module: `emit` (a write
//! it runs in this frame, replying or not) and `query` (a read). On wasm32
//! they are host calls; natively the same contexts run over a [`MockHost`],
//! and several modules run together, a submission as one frame with its
//! messages and replies as the kernel runs it, on a [`MockChain`].
//! [`Env`] adds the origin checks (`signer`, `sending_module`, `sent_by`) and
//! `authority`, a stub that admits anyone for now.
//! A message emitted with [`Reply::Wanted`] comes back to its emitter in
//! the same frame as [`Module::reply`], with its id and outcome; a module
//! that never wants a reply leaves `reply` at its default.
//!
//! ```ignore
//! use guest::{Error, ExecCtx, Module, Program, QueryCtx};
//!
//! pub struct Counter;
//!
//! impl Program for Counter {
//!     const NAME: &'static str = "counter";
//!     type Op = u64;
//!     type Query = ();
//!     type Reply = u64;
//! }
//!
//! impl Module for Counter {
//!     fn execute(ctx: &ExecCtx, by: u64) -> Result<(), Error> {
//!         let n: u64 = ctx.record("n")?.unwrap_or(0);
//!         ctx.put("n", &(n + by));
//!         Ok(())
//!     }
//!
//!     fn query(ctx: &QueryCtx, (): ()) -> Result<u64, Error> {
//!         Ok(ctx.record("n")?.unwrap_or(0))
//!     }
//! }
//!
//! guest::export!(Counter);
//! ```
//!
//! `store` is the optional typed layer on top (`Map`/`Set`/`Item`, pages).

#[cfg(not(target_arch = "wasm32"))]
mod chain;
mod ctx;
mod identity;
pub mod kernel;
#[cfg(not(target_arch = "wasm32"))]
mod mock;
mod module;
mod origin;
pub mod refuse;

pub use abi;
pub use abi::{
    Blob, BlobHeader, BlobId, CryptoOp, CryptoReply, Entry, HashKind, Message, Root, Scheme,
};
#[cfg(not(target_arch = "wasm32"))]
pub use chain::{MAX_DEPTH, MockChain, Roster};
pub use ctx::{ExecCtx, QueryCtx, Reply};
pub use kernel::{
    AccountNumber, Cause, Env, Error, MessageId, ModuleId, Order, Origin, Outcome, Principal,
    Range, Roles, code,
};
#[cfg(not(target_arch = "wasm32"))]
pub use mock::{MockHost, MockState, Sibling, Verifier, blob_id, identity_role};
#[cfg(target_arch = "wasm32")]
pub use module::exports;
pub use module::{Module, Program, execute, query};
pub use refuse::{
    already_exists, capacity, corrupt, decoded, invalid, not_found, stale, unauthorized,
    unexpected_reply, wrong_state,
};
