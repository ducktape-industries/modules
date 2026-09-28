//! What one member's row says in its Height, Behind, Heard and Status
//! cells, from what the view holds. Pure, so every state is a unit test.
use ducktape_view_guest::design;
use ducktape_view_guest::methods::{NetworkStatus, Report};

use crate::queries::Node;
use crate::recent::Recent;

/// A member whose height is this many blocks from the tip, either way,
/// still reads In sync: a live validator may miss a certificate's quorum.
pub const IN_SYNC: u64 = 2;
/// A member whose last answer is older than this (ms) reads Not answering.
pub const ANSWER_MS: u64 = 3_000;
/// A `chain.network` reply this long (ms, the node's clock) after the one
/// before it, or the first, still carries answer times from before the
/// node asked again: it cannot call anyone Not answering yet.
pub const SETTLE_MS: u64 = 10_000;
/// A validator that led no block for this many blocks per validator reads
/// Quiet (where there is no `chain.network` to say more).
pub const QUIET_PER_VALIDATOR: u64 = 4;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Status {
    InSync,
    Behind(u64),
    Ahead(u64),
    NotAnswering,
    Withheld,
    NotReported,
    /// the first reply, or the first after a gap: not yet known to answer
    Checking,
    /// no `chain.network`: of the strip's blocks that name a proposer,
    /// how many this validator led
    Led {
        count: u64,
        of: u64,
    },
    /// no `chain.network`: a validator that led none for a while, and the
    /// last block it led in the strip
    Quiet {
        since: Option<u64>,
        of: u64,
    },
}

impl Status {
    /// The one word (or count) the Status cell reads.
    pub fn word(&self) -> String {
        match self {
            Status::InSync => "In sync".into(),
            Status::Behind(blocks) => format!("{} behind", design::grouped(*blocks)),
            Status::Ahead(blocks) => format!("{} ahead", design::grouped(*blocks)),
            Status::NotAnswering => "Not answering".into(),
            Status::Withheld => "Withheld".into(),
            Status::NotReported => "Not reported".into(),
            Status::Checking => "Checking".into(),
            Status::Led { count, of } => {
                format!("{} of {}", design::grouped(*count), design::grouped(*of))
            }
            Status::Quiet { .. } => "Quiet".into(),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Row {
    /// the height and what backs it: `signed`, `reported`, or nothing for
    /// this node's own tip
    pub height: Option<(u64, &'static str)>,
    /// the connected node's tip minus that height
    pub behind: Option<i64>,
    pub heard: String,
    pub status: Status,
}

/// A row as `chain.network` has it, from the tip it answered at.
pub fn synced(node: &Node, this: bool, network: &NetworkStatus) -> Row {
    let peer = network.members.iter().find(|peer| peer.key == node.key);
    let said = peer.and_then(|peer| peer.said.as_ref());
    let reported = said.and_then(|said| match said.report {
        Report::Height { height, .. } => Some(height),
        Report::Withheld => None,
    });
    let height = match (node.validator, peer.and_then(|peer| peer.signed), reported) {
        (true, Some(signed), _) => Some((signed, "signed")),
        (_, _, Some(reported)) => Some((reported, "reported")),
        _ if this => Some((network.height, "")),
        _ => None,
    };
    let behind = height.map(|(height, _)| network.height as i64 - height as i64);
    let heard = match (this, said) {
        (true, _) => "this node".into(),
        (false, Some(said)) => format!("{} ago", design::ago(network.at, said.at)),
        (false, None) => "—".into(),
    };
    let silent = said.is_none_or(|said| network.at.saturating_sub(said.at) > ANSWER_MS);
    let status = match (said.map(|said| &said.report), behind) {
        (Some(Report::Withheld), _) if !this => Status::Withheld,
        _ if silent && !this => Status::NotAnswering,
        (_, Some(behind)) if behind > IN_SYNC as i64 => Status::Behind(behind as u64),
        (_, Some(behind)) if behind < -(IN_SYNC as i64) => Status::Ahead(behind.unsigned_abs()),
        (_, Some(_)) => Status::InSync,
        (_, None) => Status::NotReported,
    };
    Row {
        height,
        behind,
        heard,
        status,
    }
}

/// The row on screen: as the reply has it, except that a member it calls
/// Not answering before the replies settle (`settled`: this reply came
/// within [`SETTLE_MS`] of the one before) reads as the reply before had
/// it, or Checking where there is none.
pub fn shown(
    node: &Node,
    this: bool,
    network: &NetworkStatus,
    settled: bool,
    earlier: Option<&NetworkStatus>,
) -> Row {
    let row = synced(node, this, network);
    if settled || row.status != Status::NotAnswering {
        return row;
    }
    match earlier {
        Some(earlier) => synced(node, this, earlier),
        None => Row {
            status: Status::Checking,
            ..row
        },
    }
}

/// A row without `chain.network`: this node knows its own tip; a validator
/// is read by the blocks it led of the strip; a resident says nothing.
pub fn unsynced(node: &Node, this: bool, head: u64, validators: u64, recent: &Recent) -> Row {
    let status = match node.validator {
        true => {
            let (count, last, of) = recent.proposed(&node.key, head);
            let silence = last.map_or(of, |last| head - last);
            match silence > validators * QUIET_PER_VALIDATOR {
                true => Status::Quiet { since: last, of },
                false => Status::Led { count, of },
            }
        }
        false if this => Status::InSync,
        false => Status::NotReported,
    };
    Row {
        height: this.then_some((head, "")),
        behind: this.then_some(0),
        heard: if this { "this node" } else { "—" }.into(),
        status,
    }
}
