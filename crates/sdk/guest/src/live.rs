//! Live grants: who may publish and subscribe to a room's live tracks
//! through a node. The program that owns the room decides in its own code,
//! where it already decides who is a member, and writes the decision as a
//! row ([`abi::live`]); the node reads the row and enforces it, and a row
//! removed ends what it covered on the next block. No row for an account
//! and no `*` row: the account does nothing in the room.
//!
//! ```ignore
//! // chat's SetMembership: the members of a channel talk live in it
//! guest::live::grant(ctx, &channel, Some(account), Grant::all());
//! // a lobby open to everyone who acts on the chain
//! guest::live::grant(ctx, "lobby", None, Grant::all());
//! ```
pub use abi::live::{Grant, TABLE, key};

use crate::{AccountNumber, ExecCtx};

/// Lets `who` (one account, or with `None` every account that acts on the
/// chain) do what `grant` says in `room`; replaces the row it had.
pub fn grant(ctx: &ExecCtx, room: &str, who: Option<AccountNumber>, grant: Grant) {
    ctx.put(key(room, who), &grant);
}

/// Takes `who`'s row in `room` away: what it covered ends on the next
/// block, unless a `*` row covers it.
pub fn revoke(ctx: &ExecCtx, room: &str, who: Option<AccountNumber>) {
    ctx.delete(key(room, who));
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::MockHost;

    #[test]
    fn a_grant_is_the_row_the_node_reads_and_a_revoke_removes_it() {
        let host = MockHost::default();
        let ctx = host.exec(MockHost::env("echo"));
        grant(&ctx, "lobby", Some(42), Grant::all());
        grant(&ctx, "lobby", None, Grant::default());
        let row: Option<Grant> = ctx.record(b"live/lobby/42").unwrap();
        assert_eq!(row, Some(Grant::all()));
        let open: Option<Grant> = ctx.record(b"live/lobby/*").unwrap();
        assert_eq!(open, Some(Grant::default()));
        revoke(&ctx, "lobby", Some(42));
        assert_eq!(ctx.record::<Grant>(b"live/lobby/42").unwrap(), None);
    }
}
