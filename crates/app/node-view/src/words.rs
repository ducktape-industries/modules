//! Every word both of the view's trees show, once (Expo
//! `src/network/words.ts` and the Nodes screens' own strings): the reads'
//! states, the section heads, the pulse, the epoch, the vote's labels and
//! captions, why the validators' cells are empty, and the footnote. The
//! desktop's sheet (`ui.rs`, `table.rs`, `motions.rs`) and the phone's
//! screens (`ui/phone/`) read them here; a sentence only one tree shows
//! stays in that tree. What a motion does in words is `voting.rs`'s, and a
//! validator's status word `row.rs`'s.
use programs::Pending as Scheduled;
use valset::Pending;
use view::prelude::*;

use crate::Unheard;
use crate::queries::Schedule;
use crate::voting::{self, Voting};

/// The view's title until the node names its network.
pub(crate) const NODES: &str = "Nodes";
/// A fact the node has not given: an address, a vote, how far behind.
pub(crate) const NO_VALUE: &str = "—";
pub(crate) const READING_STATUS: &str = "Reading node status…";
pub(crate) const READING_MEMBERS: &str = "Reading the members…";
pub(crate) const NO_MEMBERS: &str = "No members";
pub(crate) const NO_MEMBERS_WHY: &str = "The validator set of this network is empty.";
/// A member's facts, as the desktop's columns and the phone's member page
/// name them, and the node's block time beside its height.
pub(crate) const KEY: &str = "Key";
pub(crate) const ADDRESS: &str = "Address";
pub(crate) const HEIGHT: &str = "Height";
pub(crate) const BEHIND: &str = "Behind";
pub(crate) const STATUS: &str = "Status";
pub(crate) const BLOCK_TIME: &str = "Block time";
pub(crate) const VALIDATORS: &str = "Validators";
pub(crate) const RESIDENTS: &str = "Residents";
/// After the connected node's address, and its key on the desktop.
pub(crate) const THIS_NODE: &str = "this node";
/// The blocks a validator led, and a resident's place there.
pub(crate) const PROPOSED: &str = "Proposed";
pub(crate) const DOESNT_PROPOSE: &str = "Doesn't propose";
/// A validator's height: the newest block it voted to finalize.
pub(crate) const VOTED: &str = "voted";
/// What a node without `chain.network` shows over the members.
pub(crate) const NO_NETWORK: &str =
    "This node doesn't report its validators' votes. Update the node.";
/// What a node that hears no votes shows over the members.
pub(crate) const NOT_VOTING: &str =
    "This node isn't voting right now, so it can't see the validators' votes.";

pub(crate) const MOTIONS: &str = "Motions";
pub(crate) const READING_MOTIONS: &str = "Reading the motions…";
pub(crate) const NO_MOTION: &str = "No motion waits for votes.";
pub(crate) const VOTE: &str = "Vote";
pub(crate) const SCHEDULED: &str = "Scheduled changes";
pub(crate) const READING_SCHEDULE: &str = "Reading the schedule…";
pub(crate) const NO_CHANGE: &str = "No change is scheduled.";
pub(crate) const PROPOSE_CANCEL: &str = "Propose cancel";
pub(crate) const CHANGE_ELSEWHERE: &str = "Change a program in Explorer, Programs.";
/// What a press that enacts asks before it votes, and its two answers.
pub(crate) const ENACTS_NOW: &str =
    "Your vote reaches the quorum: this happens now, and a vote can't be taken back.";
pub(crate) const VOTE_AND_ENACT: &str = "Vote and enact";
pub(crate) const NOT_NOW: &str = "Not now";
/// Clears the last vote's outcome.
pub(crate) const DISMISS: &str = "Dismiss";

/// The door's motion as a press names it, and what it means.
pub(crate) fn door(open: bool) -> (&'static str, &'static str) {
    match open {
        true => ("Open the door", "anyone enrolls without an invite"),
        false => ("Close the door", "enrolling takes an invite"),
    }
}

/// Whether the connected node answers.
pub(crate) fn pulse(answering: bool) -> &'static str {
    match answering {
        true => "In sync",
        false => "Not answering",
    }
}

/// How long the node has been silent.
pub(crate) fn last_answer(seconds: u64) -> String {
    format!("Last answer {} ago", design::ago(seconds * 1000, 0))
}

/// Why the validators' cells are empty.
pub(crate) fn unheard(why: Unheard) -> &'static str {
    match why {
        Unheard::NotVoting => NOT_VOTING,
        Unheard::NoNetwork => NO_NETWORK,
    }
}

/// What a validator's cells mean, under the members.
pub(crate) fn footnote() -> String {
    format!(
        "Height: the newest block the validator voted to finalize. Quiet: none for {}.",
        design::plural(crate::row::QUIET, "block", "blocks")
    )
}

/// Which blocks the strip of blocks led spans: `last 64 blocks · 8,349 →
/// 8,412`.
pub(crate) fn span(span: &std::ops::RangeInclusive<u64>) -> String {
    format!(
        "last {} · {} → {}",
        design::plural(span.end() - span.start() + 1, "block", "blocks"),
        design::grouped(*span.start()),
        design::grouped(*span.end()),
    )
}

/// The network's name before its salt (`<label>#<salt>`).
pub(crate) fn network(chain_id: &str) -> &str {
    chain_id
        .split_once('#')
        .map_or(chain_id, |(label, _)| label)
}

/// Where the tip is in its epoch (Expo `words.ts` `epochPlace`). A block
/// closes epoch `e` when `(height + 1)` reaches `(e + 1)` epochs, so the
/// tip is `(height + 1) % length` blocks into its epoch.
pub(crate) struct EpochPlace {
    pub(crate) epoch: u64,
    /// blocks of this epoch gone
    pub(crate) into: u64,
    pub(crate) length: u64,
}

impl EpochPlace {
    /// The tip's place, on a network with epochs.
    pub(crate) fn of(status: &NodeStatus) -> Option<Self> {
        let length = status.epoch_length;
        (length > 0).then(|| Self {
            epoch: status.epoch,
            into: (status.height + 1) % length,
            length,
        })
    }

    /// `Epoch 131 · 29 / 64`
    pub(crate) fn place(&self) -> String {
        format!(
            "Epoch {} · {} / {}",
            design::grouped(self.epoch),
            self.into,
            self.length
        )
    }

    /// `Next epoch in 35 blocks`
    pub(crate) fn next(&self) -> String {
        format!(
            "Next epoch in {}",
            design::plural(self.length - self.into, "block", "blocks")
        )
    }
}

/// A waiting motion's votes of the quorum, and which of the reader's
/// validators voted: `1 of 2 · yours has not voted`.
pub(crate) fn pending_caption(voting: &Voting, pending: &Pending, needed: u64) -> String {
    let cast = voting.voices.len() - voting.uncast(&pending.motion);
    format!(
        "{} of {needed} · {}",
        pending.voters.len(),
        yours(cast, voting.voices.len())
    )
}

/// Which of the reader's validators voted: "yours voted", "1 of 2 of yours".
fn yours(cast: usize, voices: usize) -> String {
    match (cast, voices) {
        (0, 1) => "yours has not voted".into(),
        (1, 1) => "yours voted".into(),
        (cast, voices) => format!("{cast} of your {voices} voted"),
    }
}

/// Who scheduled a change, and how long the vote has to cancel it: `by
/// vote · in 300 blocks`, `by Dev · validators can cancel for 5 min`; who
/// alone until the node's status is read.
pub(crate) fn scheduled_caption(
    change: &Scheduled,
    schedule: &Schedule,
    status: Option<&NodeStatus>,
) -> String {
    let by = match change.by {
        Some(account) => voting::named(&schedule.names, account),
        None => "vote".into(),
    };
    let Some(status) = status else {
        return format!("by {by}");
    };
    let blocks = change.height.saturating_sub(status.height);
    match change.by {
        Some(_) => format!(
            "by {by} · validators can cancel for {} min",
            (blocks * status.block_time_ms).div_ceil(60_000)
        ),
        None => format!("by {by} · in {}", design::plural(blocks, "block", "blocks")),
    }
}
