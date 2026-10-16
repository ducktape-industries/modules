//! Everything chat keeps, declared once: each table's key and value types,
//! and the one place a message row is stored and read back. Keys are
//! typed (`store::KeyCodec`: integers big-endian, strings and principals
//! NUL-terminated, so names list by name), values borsh. A tuple key scans by its leading
//! elements, which is how every "in this channel" read works.
use ::program::{Error, ExecCtx, QueryCtx, not_found};
use borsh::{BorshDeserialize, BorshSerialize};
use store::{Map, Set};

use crate::{Block, ChannelRow, MemberRow, MsgRow, Principal, Reaction, plain_text, tags};

type ChannelId = String;
type Seq = u64;

pub const CHANNELS: Map<ChannelId, ChannelRow> = Map::new("channel/");
/// The newest message's seq, per channel.
pub const HEADS: Map<ChannelId, Seq> = Map::new("head/");
pub const MESSAGES: Map<(ChannelId, Seq), Stored> = Map::new("message/");
/// Where each message id lives: ids are unique across channels.
pub const MESSAGE_IDS: Map<String, (ChannelId, Seq)> = Map::new("message-id/");
pub const MEMBERS: Map<(ChannelId, Principal), MemberRow> = Map::new("member/");
/// Who chose which emoji: `(channel, seq, emoji, principal)`.
pub const REACTIONS: Set<(ChannelId, Seq, String, Principal)> = Set::new("reaction/");

/// Timeline roots, newest first: `(channel, newest_first(seq))`.
pub const ROOTS: Set<(ChannelId, Seq)> = Set::new("root/");
/// Thread replies in post order: `(channel, root, reply)`.
pub const REPLIES: Set<(ChannelId, Seq, Seq)> = Set::new("reply/");
/// Each author's answered threads, newest answer first:
/// `(channel, root author, newest_first(last reply))` → the root's seq.
pub const ANSWERED: Map<(ChannelId, Principal, Seq), Seq> = Map::new("answered/");

/// A message as chat keeps it: the [`MsgRow`] a read answers, less what a
/// read derives (its flat text and `#tags`, from `blocks`; whether the
/// reader chose a reaction). [`put_message`] and [`Stored::row`] are the one
/// mapping between the two.
#[derive(BorshSerialize, BorshDeserialize)]
pub struct Stored {
    pub(crate) channel_id: ChannelId,
    pub(crate) seq: Seq,
    message_id: String,
    author: Principal,
    height: u64,
    pub(crate) time: u64,
    pub(crate) blocks: Vec<Block>,
    deleted: bool,
    edited: bool,
    rev: u32,
    edited_at: Option<u64>,
    base_rev: Option<u32>,
    thread: Option<Seq>,
    reply_count: u64,
    last_reply_seq: Option<Seq>,
    /// each emoji chosen and how many chose it, in emoji order
    reactions: Vec<(String, u64)>,
}

impl Stored {
    /// The row a read answers.
    pub(crate) fn row(self) -> MsgRow {
        MsgRow {
            text: plain_text(&self.blocks),
            tags: tags(&self.blocks),
            reactions: (self.reactions.into_iter())
                .map(|(emoji, count)| Reaction {
                    emoji,
                    count,
                    reacted_by_me: false,
                })
                .collect(),
            channel_id: self.channel_id,
            seq: self.seq,
            message_id: self.message_id,
            author: self.author,
            height: self.height,
            time: self.time,
            blocks: self.blocks,
            deleted: self.deleted,
            edited: self.edited,
            rev: self.rev,
            edited_at: self.edited_at,
            base_rev: self.base_rev,
            thread: self.thread,
            reply_count: self.reply_count,
            last_reply_seq: self.last_reply_seq,
        }
    }
}

/// A key part that scans newest first. Its own inverse.
pub(crate) fn newest_first(n: u64) -> u64 {
    u64::MAX - n
}

pub(crate) fn channel(ctx: &QueryCtx, id: &str) -> Result<ChannelRow, Error> {
    CHANNELS
        .get(ctx, &id.to_owned())?
        .ok_or_else(|| not_found(format!("no channel {id}")))
}

pub(crate) fn message(ctx: &QueryCtx, channel_id: &str, seq: Seq) -> Result<MsgRow, Error> {
    MESSAGES
        .get(ctx, &(channel_id.to_owned(), seq))?
        .map(Stored::row)
        .ok_or_else(|| not_found(format!("no message {channel_id}/{seq}")))
}

/// The rows at these addresses; one gone (never, in chat) is skipped.
pub(crate) fn messages(
    ctx: &QueryCtx,
    at: impl IntoIterator<Item = (ChannelId, Seq)>,
) -> Result<Vec<MsgRow>, Error> {
    at.into_iter()
        .filter_map(|key| MESSAGES.get(ctx, &key).transpose())
        .map(|row| row.map(Stored::row))
        .collect()
}

/// Stores `row` as chat keeps it ([`Stored`]): what a read derives from
/// the rest is never stored, so a row's `text`, `tags` and
/// `reacted_by_me` are dropped here, whatever they hold.
pub(crate) fn put_message(ctx: &ExecCtx, row: &MsgRow) {
    let MsgRow {
        channel_id,
        seq,
        message_id,
        author,
        height,
        time,
        blocks,
        text: _,
        deleted,
        edited,
        rev,
        edited_at,
        base_rev,
        thread,
        reply_count,
        last_reply_seq,
        reactions,
        tags: _,
    } = row.clone();
    let stored = Stored {
        channel_id,
        seq,
        message_id,
        author,
        height,
        time,
        blocks,
        deleted,
        edited,
        rev,
        edited_at,
        base_rev,
        thread,
        reply_count,
        last_reply_seq,
        reactions: (reactions.into_iter())
            .map(|reaction| (reaction.emoji, reaction.count))
            .collect(),
    };
    MESSAGES.put(ctx, &(row.channel_id.clone(), row.seq), &stored);
}

pub(crate) fn toggle<K: store::KeyCodec>(ctx: &ExecCtx, set: &Set<K>, key: &K, on: bool) {
    if on {
        set.insert(ctx, key);
    } else {
        set.remove(ctx, key);
    }
}
