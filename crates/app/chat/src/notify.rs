//! What a post tells the people it concerns: chat's notification logic, all
//! of it here. A post words its notices on chain as it lands and emits them
//! as events of its own run ([`notices`]); the inbox and the app carry them
//! without knowing chat.
//!
//! Who: in a dm room, the other peer (`Direct`); each account the message
//! names (`Mention`); the author of the thread's root (`Reply`). Never the
//! author, never a program's account (no one reads its notices), and in a
//! members-only room only its owner and seated members (decision D5). A
//! person a post reaches twice is told once, by the first of Direct,
//! Mention, Reply. Never a number no account holds: identity's profile of
//! every recipient is asked, and one it says nobody holds is dropped. A
//! notice carries one kind and one title, so a post
//! emits one notice per kind it has recipients for, all under its one key:
//! a reply that names someone is a Mention to them and a Reply to the
//! root's author.
//!
//! The words are frozen at post: the author's and each named account's
//! name from identity's profiles, asked once each in this frame (one
//! identity refuses or fails to answer is called by its number, and the
//! post goes on), the room's from its row. An edit tells no one; a delete
//! withdraws the post's notices.
use std::collections::BTreeSet;

use abi::role::identity as role;
use guest::{Error, ExecCtx, capacity};
use notices::{Kind, Link, Notice, NoticeEvent};

use crate::state::MEMBERS;
use crate::view::Names;
use crate::{
    Block, ChannelRow, MAX_NAMED, MODULE, Mark, MsgRow, Principal, dm_peers, namespace, read_text,
};

/// The accounts a message names, each once, in the order named; refused
/// past [`MAX_NAMED`], so a notice reaches everyone its post names.
pub(crate) fn named(blocks: &[Block]) -> Result<Vec<u64>, Error> {
    let mut named = Vec::new();
    let marks = blocks
        .iter()
        .flat_map(|block| match block {
            Block::Paragraph(spans) | Block::Quote(spans) => spans.as_slice(),
            Block::Code { .. } | Block::Divider => &[],
        })
        .flat_map(|span| &span.marks);
    for mark in marks {
        if let Mark::Mention(Principal::Account(account)) = mark
            && !named.contains(account)
        {
            if named.len() == MAX_NAMED {
                return Err(capacity(format!(
                    "a message names at most {MAX_NAMED} accounts"
                )));
            }
            named.push(*account);
        }
    }
    Ok(named)
}

/// Emits what `row`, just posted in `channel`, tells the people it
/// concerns. `root` is the author of the thread root it replies under;
/// `named` is [`named`] of its blocks.
pub(crate) fn posted(
    ctx: &ExecCtx,
    channel: &ChannelRow,
    row: &MsgRow,
    root: Option<&Principal>,
    named: &[u64],
) {
    let author = row.author.account();
    let fresh = |account: &u64, told: &[&[u64]]| {
        Some(*account) != author && told.iter().all(|group| !group.contains(account))
    };
    let direct: Vec<u64> = dm_peers(&channel.id)
        .map_or_else(Vec::new, |(a, b)| vec![a, b])
        .into_iter()
        .filter(|peer| fresh(peer, &[]))
        .collect();
    let mention: Vec<u64> = named
        .iter()
        .copied()
        .filter(|account| fresh(account, &[&direct]) && reaches(ctx, channel, *account))
        .collect();
    let reply: Vec<u64> = root
        .and_then(Principal::account)
        .filter(|account| fresh(account, &[&direct, &mention]) && reaches(ctx, channel, *account))
        .into_iter()
        .collect();
    if direct.is_empty() && mention.is_empty() && reply.is_empty() {
        return;
    }
    let (names, nobody) = profiles(
        ctx,
        author
            .into_iter()
            .chain(direct.iter().copied())
            .chain(named.iter().copied())
            .chain(reply.iter().copied()),
    );
    // a number no account holds, and a program's account (forge's, whose
    // review line roots a thread), have no reader to tell
    let read = |to: Vec<u64>| -> Vec<u64> {
        to.into_iter()
            .filter(|account| {
                !nobody.contains(account) && names.module(&Principal::Account(*account)).is_none()
            })
            .collect()
    };
    let (direct, mention, reply) = (read(direct), read(mention), read(reply));
    let who = names.author(&row.author);
    let room = room(channel);
    let body = notices::cut(&read_text(&row.blocks, &names));
    for (kind, to, title) in [
        (Kind::Direct, direct, who.clone()),
        (
            Kind::Mention,
            mention,
            format!("{who} mentioned you in {room}"),
        ),
        (Kind::Reply, reply, format!("{who} replied in {room}")),
    ] {
        if to.is_empty() {
            continue;
        }
        let notice = Notice {
            key: key(&row.channel_id, row.seq),
            to,
            from: author,
            kind,
            title: notices::cut(&title),
            body: body.clone(),
            tag: tag(row),
            link: link(&row.channel_id, row.seq),
        };
        ctx.event(notices::encode(&NoticeEvent::Post(notice)));
    }
}

/// Withdraws the notices the post at `seq` made. A post that made none
/// withdraws nothing: the inbox drops what it holds under the key.
pub(crate) fn deleted(ctx: &ExecCtx, channel: &str, seq: u64) {
    let key = key(channel, seq);
    ctx.event(notices::encode(&NoticeEvent::Retract { key }));
}

/// Whether a notice from `channel` reaches `account`: anyone in an open
/// room; in a members-only room, its owner and seated members (decision
/// D5).
fn reaches(ctx: &ExecCtx, channel: &ChannelRow, account: u64) -> bool {
    let who = Principal::Account(account);
    !channel.members_only()
        || channel.admits(&who, MEMBERS.has(ctx, &(channel.id.clone(), who.clone())))
}

/// Identity's profiles of `accounts`, one query each, and the accounts it
/// says no one holds. One identity refuses or fails to answer is in
/// neither: [`Names`] calls it by its number, and it is still told.
fn profiles(ctx: &ExecCtx, accounts: impl IntoIterator<Item = u64>) -> (Names, BTreeSet<u64>) {
    let identity = &ctx.env().roles.identity;
    let accounts: BTreeSet<u64> = accounts.into_iter().collect();
    let mut nobody = BTreeSet::new();
    let names = Names::from_roster(accounts.into_iter().filter_map(|account| {
        let asked = role::Query::Profile(account);
        match ctx.query::<role::Query, role::Reply>(identity, &asked) {
            Ok(role::Reply::Profile(Some(profile))) => Some(profile),
            Ok(role::Reply::Profile(None)) => {
                nobody.insert(account);
                None
            }
            _ => None,
        }
    }));
    (names, nobody)
}

/// How a title names the room: a people's room by `#name`; a program's own
/// room by its name and its program (forge's `web#3 on forge`).
fn room(channel: &ChannelRow) -> String {
    match namespace::program(&channel.id) {
        Some(program) => format!("{} on {program}", channel.name),
        None => format!("#{}", channel.name),
    }
}

/// A post's notices' key: `<channel>/<seq>`.
fn key(channel: &str, seq: u64) -> String {
    format!("{channel}/{seq}")
}

/// What the app folds a notice under and a view's seen watermark names:
/// the room for a root, `<channel>/<root>` for a reply, so reading a room
/// leaves its threads' notices unread. chat-view names its watermarks with
/// it, so the program and the view cannot drift.
pub fn tag(row: &MsgRow) -> String {
    match row.thread {
        Some(root) => key(&row.channel_id, root),
        None => row.channel_id.clone(),
    }
}

/// Where opening a notice goes: the message in chat, or a program's room
/// where that program shows it (`forge:web:3` is forge's `web/3`). chat-view
/// reads a chat link's route back to the room and message it lands on.
pub fn link(channel: &str, seq: u64) -> Link {
    match namespace::path(channel) {
        Some((program, path)) => Link {
            program: program.into(),
            segments: path.into_iter().map(Into::into).collect(),
        },
        None => Link {
            program: MODULE.into(),
            segments: vec![channel.into(), seq.to_string()],
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::MAX_ID_BYTES;

    /// The longest key, tag and link chat writes fit a notice's caps: a
    /// 64-byte channel at the last seq, a program's room of 64 bytes with
    /// the most segments, and one with the longest program.
    #[test]
    fn the_longest_ids_chat_writes_fit_a_notice() {
        let plain = "c".repeat(MAX_ID_BYTES);
        let most_segments = format!("p{}", ":".repeat(MAX_ID_BYTES - 1));
        let longest_program = format!("{}:x", "p".repeat(MAX_ID_BYTES - 2));
        for channel in [plain, most_segments, longest_program] {
            assert_eq!(channel.len(), MAX_ID_BYTES);
            let event = NoticeEvent::Post(Notice {
                key: key(&channel, u64::MAX),
                to: vec![1; MAX_NAMED],
                from: Some(u64::MAX),
                kind: Kind::Reply,
                title: "t".repeat(notices::MAX_TEXT_BYTES),
                body: "b".repeat(notices::MAX_TEXT_BYTES),
                // a reply's tag: the thread's root under the room
                tag: key(&channel, u64::MAX),
                link: link(&channel, u64::MAX),
            });
            assert_eq!(notices::decode(&notices::encode(&event)), Some(Ok(event)));
        }
    }
}
