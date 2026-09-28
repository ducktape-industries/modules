//! What one member's row says in its Height, Behind and Status cells, from
//! what the view holds. Pure, so every state is a unit test. A resident's
//! cells say nothing: its sync state is not reported.
use ducktape_view_guest::design;
use ducktape_view_guest::methods::NetworkStatus;

use crate::queries::Node;
use crate::recent::Recent;

/// A validator whose vote is this many blocks under the tip still reads
/// In sync: a live one's newest vote may still be in flight.
pub const IN_SYNC: u64 = 2;
/// A validator that voted for (or, without `chain.network`, led) no block
/// for this many blocks per validator reads Quiet.
pub const QUIET_PER_VALIDATOR: u64 = 4;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Status {
    InSync,
    Behind(u64),
    /// a validator that stopped: the last block it voted for (or led), where
    /// known, and without `chain.network` the blocks the strip looked at
    Quiet {
        since: Option<u64>,
        of: u64,
    },
    /// without `chain.network`: of the strip's blocks that name a proposer,
    /// how many this validator led
    Led {
        count: u64,
        of: u64,
    },
    /// a resident
    Blank,
}

impl Status {
    /// The one word (or count) the Status cell reads.
    pub fn word(&self) -> String {
        match self {
            Status::InSync => "In sync".into(),
            Status::Behind(blocks) => format!("{} behind", design::grouped(*blocks)),
            Status::Quiet { .. } => "Quiet".into(),
            Status::Led { count, of } => {
                format!("{} of {}", design::grouped(*count), design::grouped(*of))
            }
            Status::Blank => "—".into(),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Row {
    /// the newest block this node applied that the validator voted to
    /// finalize, as this node heard it
    pub signed: Option<u64>,
    /// the connected node's tip minus that height; never negative, as the
    /// node counts no vote for a block it has not applied
    pub behind: Option<u64>,
    pub status: Status,
}

/// A resident's cells, and a validator's where this node hears no votes.
pub const BLANK: Row = Row {
    signed: None,
    behind: None,
    status: Status::Blank,
};

/// A row as `chain.network` has it: a validator by the newest block it
/// voted to finalize, from the tip the node answered at.
pub fn synced(node: &Node, validators: u64, network: &NetworkStatus) -> Row {
    if !node.validator {
        return BLANK;
    }
    let signed = network
        .members
        .iter()
        .find(|peer| peer.key == node.key)
        .and_then(|peer| peer.signed);
    let behind = signed.map(|signed| network.height.saturating_sub(signed));
    let status = match behind {
        None => Status::Quiet { since: None, of: 0 },
        Some(behind) if behind > validators * QUIET_PER_VALIDATOR => Status::Quiet {
            since: signed,
            of: 0,
        },
        Some(behind) if behind > IN_SYNC => Status::Behind(behind),
        Some(_) => Status::InSync,
    };
    Row {
        signed,
        behind,
        status,
    }
}

/// A row without `chain.network`: a validator is read by the blocks it led
/// of the strip.
pub fn unsynced(node: &Node, head: u64, validators: u64, recent: &Recent) -> Row {
    if !node.validator {
        return BLANK;
    }
    let (count, last, of) = recent.proposed(&node.key, head);
    let silence = last.map_or(of, |last| head - last);
    let status = match silence > validators * QUIET_PER_VALIDATOR {
        true => Status::Quiet { since: last, of },
        false => Status::Led { count, of },
    };
    Row { status, ..BLANK }
}
