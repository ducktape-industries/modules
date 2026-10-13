//! What chat tells attribution as a message is posted, edited or deleted: the accounts it
//! mentions, and in a direct message the counterpart it is written to.
use std::collections::BTreeSet;

use attribution::{Actor, Object, Reason, Relation, Update};
use guest::{ExecCtx, Reply};

use crate::{AccountNumber, Block, Mark, MsgRow, Principal, dm_peers};

pub const MESSAGE: &str = "message";
pub const DIRECT: &str = "direct";

fn mentioned(blocks: &[Block]) -> impl Iterator<Item = AccountNumber> + '_ {
    blocks
        .iter()
        .flat_map(|block| match block {
            Block::Paragraph(spans) | Block::Quote(spans) => spans.as_slice(),
            Block::Code { .. } | Block::Divider => &[],
        })
        .flat_map(|span| &span.marks)
        .filter_map(|mark| match mark {
            Mark::Mention(Principal::Account(number)) => Some(*number),
            _ => None,
        })
}

pub fn relations(row: &MsgRow) -> Vec<Relation> {
    if row.deleted {
        return Vec::new();
    }
    let mut held = BTreeSet::new();
    for number in mentioned(&row.blocks) {
        held.insert((number, Reason::Mention));
    }
    if let Some((lo, hi)) = dm_peers(&row.channel_id) {
        for peer in [lo, hi] {
            held.insert((peer, Reason::Defined(DIRECT.into())));
        }
    }
    held.into_iter()
        .filter(|(number, _)| *number != 0 && row.author != Principal::Account(*number))
        .map(|(recipient, reason)| Relation {
            recipient,
            reason,
            detail: Vec::new(),
        })
        .collect()
}

pub fn revision(row: &MsgRow) -> u64 {
    2 * u64::from(row.rev) + u64::from(row.deleted)
}

pub(crate) fn report(ctx: &ExecCtx, sender: &Principal, old: Option<&MsgRow>, row: &MsgRow) {
    let after = relations(row);
    let unrelated = after.is_empty() && old.is_none_or(|old| relations(old).is_empty());
    if unrelated {
        return;
    }
    let actor = match sender {
        Principal::Account(number) => Actor::Account(*number),
        Principal::Root => Actor::System,
    };
    let update = Update {
        object: Object {
            kind: MESSAGE.into(),
            id: row.message_id.clone(),
        },
        revision: revision(row),
        actor,
        relations: after,
        transfers: Vec::new(),
    };
    ctx.emit(
        attribution::MODULE,
        &attribution::Op::Attribute(update),
        Reply::Wanted,
    );
}
