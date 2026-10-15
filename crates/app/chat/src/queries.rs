//! One function per [`Query`](crate::Query), each named by [`Chat::query`](crate::Chat)'s
//! match and each a read of the tables in `state.rs`. Chat's listings only grow, so a page cursor from any height
//! resumes where it left off.
use ::program::Range;
use ::program::{Error, QueryCtx, invalid};
use store::{KeyCodec, PageRequest, PageResponse};

use crate::state::{
    ANSWERED, CHANNELS, HEADS, MESSAGE_IDS, MESSAGES, REACTIONS, REPLIES, ROOTS, Stored, message,
    messages, newest_first,
};
use crate::text::{fold, tag_label};
use crate::{ChannelInfo, ChannelRow, MessageHits, MsgRow, Principal, Reply, plain_text, tags};

/// `reply` as `viewer` reads it: each reaction learns whether one of
/// `viewer` chose it.
pub(crate) fn seen_by(
    ctx: &QueryCtx,
    mut reply: Reply,
    viewer: Vec<Principal>,
) -> Result<Reply, Error> {
    for row in rows_in(&mut reply) {
        mark_reacted(ctx, &viewer, row);
    }
    Ok(reply)
}

/// The `Roots` cursor that resumes below `seq`: `PageRequest::after` for the page
/// of roots older than the one on screen.
pub fn roots_below(channel_id: &str, seq: u64) -> Vec<u8> {
    let channel_id = channel_id.to_owned();
    abi::encode(&store::Cursor {
        height: 0,
        scope: ROOTS.key(&channel_id),
        after: ROOTS.key(&(channel_id.clone(), newest_first(seq))),
    })
}

fn info(ctx: &QueryCtx, channel: ChannelRow) -> Result<ChannelInfo, Error> {
    let head_seq = HEADS.get(ctx, &channel.id)?.unwrap_or(0);
    Ok(ChannelInfo { channel, head_seq })
}

pub(crate) fn channels(
    ctx: &QueryCtx,
    page: &PageRequest,
) -> Result<PageResponse<ChannelInfo>, Error> {
    CHANNELS
        .range(ctx, page)?
        .try_map(|(_, channel)| info(ctx, channel))
}

pub(crate) fn channel(ctx: &QueryCtx, id: &String) -> Result<Option<ChannelInfo>, Error> {
    CHANNELS
        .get(ctx, id)?
        .map(|channel| info(ctx, channel))
        .transpose()
}

pub(crate) fn by_id(ctx: &QueryCtx, message_id: &String) -> Result<Option<MsgRow>, Error> {
    MESSAGE_IDS
        .get(ctx, message_id)?
        .map(|(channel_id, seq)| message(ctx, &channel_id, seq))
        .transpose()
}

/// The root of the author's most recently answered thread.
pub(crate) fn attention(
    ctx: &QueryCtx,
    channel_id: &str,
    author: Principal,
) -> Result<Option<MsgRow>, Error> {
    let channel_id = channel_id.to_owned();
    let newest = ANSWERED.prefix_of(&(channel_id.clone(), author)).limit(1);
    ANSWERED
        .scan(ctx, newest)?
        .first()
        .map(|(_, root)| message(ctx, &channel_id, *root))
        .transpose()
}

pub(crate) fn roots(
    ctx: &QueryCtx,
    channel_id: String,
    page: &PageRequest,
) -> Result<PageResponse<MsgRow>, Error> {
    let keys = ROOTS.range_of(ctx, &channel_id, page)?;
    rows_at(
        ctx,
        keys.map(|(channel_id, newest)| (channel_id, newest_first(newest))),
    )
}

pub(crate) fn thread(
    ctx: &QueryCtx,
    channel_id: String,
    root: u64,
    page: &PageRequest,
) -> Result<Reply, Error> {
    let keys = REPLIES.range_of(ctx, &(channel_id.clone(), root), page)?;
    Ok(Reply::Thread {
        root: MESSAGES.get(ctx, &(channel_id, root))?.map(Stored::row),
        replies: rows_at(ctx, keys.map(|(channel_id, _, reply)| (channel_id, reply)))?,
    })
}

/// Up to `page.limit` messages, half before `seq` and half from it.
pub(crate) fn around(
    ctx: &QueryCtx,
    channel_id: String,
    seq: u64,
    page: &PageRequest,
) -> Result<Vec<MsgRow>, Error> {
    let half = page.limit() / 2;
    let lo = MESSAGES.key(&(channel_id.clone(), seq.saturating_sub(half)));
    let hi = MESSAGES.key(&(channel_id, seq.saturating_add(half + 1)));
    let rows = MESSAGES.scan(ctx, Range::new(lo, Some(hi)))?;
    Ok(rows.into_iter().map(|(_, row)| row.row()).collect())
}

/// The messages holding every whitespace-separated term of `text` as a
/// substring of their flat text, both [`fold`]ed: newest first, at most
/// `page.limit`, `capped` when more matched. No index: each channel is
/// walked from its newest message until it has a page or reaches its first.
pub(crate) fn search(
    ctx: &QueryCtx,
    text: &str,
    channel_id: Option<String>,
    page: &PageRequest,
) -> Result<MessageHits, Error> {
    let text = fold(text);
    let terms: Vec<&str> = text.split_whitespace().collect();
    if terms.is_empty() {
        return Err(invalid("nothing to search for"));
    }
    let holds_every_term = |row: &Stored| {
        let flat = fold(&plain_text(&row.blocks));
        terms.iter().all(|term| flat.contains(term))
    };
    let limit = usize::try_from(page.limit()).unwrap_or(usize::MAX);
    let mut hits = newest(
        ctx,
        channel_id,
        None,
        limit.saturating_add(1),
        holds_every_term,
    )?;
    let capped = hits.len() > limit;
    hits.truncate(limit);
    Ok(MessageHits {
        hits: hits.into_iter().map(Stored::row).collect(),
        capped,
    })
}

/// One page of the messages tagged `tag`, newest first. No index: each
/// channel is walked from its newest message, or from below the page's
/// cursor.
pub(crate) fn tagged(
    ctx: &QueryCtx,
    tag: &str,
    channel_id: Option<String>,
    page: &PageRequest,
) -> Result<PageResponse<MsgRow>, Error> {
    let label = tag_label(tag);
    let scope = PageRequest::scope_of(&(&label, &channel_id));
    let listing = page.listing(ctx, scope.clone())?;
    let after = match page.open(&scope)? {
        None => None,
        Some(cursor) => Some(
            Place::decode_key(&mut cursor.after.as_slice())
                .ok_or_else(|| invalid("a tag page cursor does not decode"))?,
        ),
    };
    let want = usize::try_from(listing.limit()).unwrap_or(usize::MAX);
    let tagged = |row: &Stored| tags(&row.blocks).contains(&label);
    let rows = newest(
        ctx,
        channel_id,
        after.as_ref(),
        want.saturating_add(1),
        tagged,
    )?;
    Ok(listing.reply((rows.into_iter()).map(|row| (place(&row).key_bytes(), row.row()))))
}

/// Where a row sorts in a newest-first listing across channels:
/// `(newest_first(time), newest_first(seq), channel)`. A block's time never
/// goes back, so one channel's rows newest first by seq are in this order.
type Place = (u64, u64, String);

fn place(row: &Stored) -> Place {
    let channel_id = row.channel_id.clone();
    (newest_first(row.time), newest_first(row.seq), channel_id)
}

/// Up to `want` rows that `keep` admits and that sort after `after`, in
/// [`Place`] order: `channel_id`, or every channel that has a message,
/// each [`walk`]ed newest first.
fn newest(
    ctx: &QueryCtx,
    channel_id: Option<String>,
    after: Option<&Place>,
    want: usize,
    keep: impl Fn(&Stored) -> bool,
) -> Result<Vec<Stored>, Error> {
    let heads = match channel_id {
        Some(id) => HEADS
            .get(ctx, &id)?
            .map(|head| (id, head))
            .into_iter()
            .collect(),
        None => HEADS.all(ctx)?,
    };
    let mut found = Vec::new();
    for (channel_id, head) in heads {
        // ponytail: a page across channels walks every channel but the
        // cursor's from its head again; find each one's place by a binary
        // search on time if deep pages across channels ever matter.
        let from = match after {
            Some((_, seq, at)) if *at == channel_id => newest_first(*seq).saturating_sub(1),
            _ => head,
        };
        let admitted = |row: &Result<Stored, Error>| match row {
            Ok(row) => after.is_none_or(|after| place(row) > *after) && keep(row),
            Err(_) => true,
        };
        let hits = walk(ctx, &channel_id, from).filter(admitted).take(want);
        found.extend(hits.collect::<Result<Vec<_>, _>>()?);
    }
    found.sort_by_cached_key(place);
    found.truncate(want);
    Ok(found)
}

/// How many seqs one scan of a [`walk`] reads: a batch, not a bound.
const STRIDE: u64 = 128;

/// `channel_id`'s messages from seq `from` down to its first, newest
/// first. The store scans forward only, so the walk reads a [`STRIDE`] of
/// seqs at a time, each below the last.
fn walk<'a>(
    ctx: &'a QueryCtx,
    channel_id: &'a str,
    from: u64,
) -> impl Iterator<Item = Result<Stored, Error>> + 'a {
    let mut below = from.saturating_add(1);
    let mut batch: Vec<Stored> = Vec::new();
    std::iter::from_fn(move || {
        while batch.is_empty() && below > 1 {
            let lo = below.saturating_sub(STRIDE).max(1);
            let range = Range::new(
                MESSAGES.key(&(channel_id.to_owned(), lo)),
                Some(MESSAGES.key(&(channel_id.to_owned(), below))),
            );
            below = lo;
            match MESSAGES.scan(ctx, range) {
                Ok(rows) => batch = rows.into_iter().map(|(_, row)| row).collect(),
                Err(fault) => {
                    below = 0;
                    return Some(Err(fault));
                }
            }
        }
        batch.pop().map(Ok)
    })
}

/// A page of message addresses as the page of rows they name.
fn rows_at(
    ctx: &QueryCtx,
    keys: PageResponse<(String, u64)>,
) -> Result<PageResponse<MsgRow>, Error> {
    Ok(PageResponse {
        height: keys.height,
        items: messages(ctx, keys.items)?,
        next: keys.next,
    })
}

/// Every message row a reply carries.
fn rows_in(reply: &mut Reply) -> Vec<&mut MsgRow> {
    match reply {
        Reply::Roots(page) | Reply::TagHits(page) => page.items.iter_mut().collect(),
        Reply::Messages(rows) | Reply::Hits(MessageHits { hits: rows, .. }) => {
            rows.iter_mut().collect()
        }
        Reply::Thread { root, replies } => root.iter_mut().chain(&mut replies.items).collect(),
        Reply::Message(row) | Reply::Attention(row) => row.iter_mut().collect(),
        Reply::Channels(_) | Reply::Channel(_) | Reply::Members(_) | Reply::Accounts(_) => vec![],
    }
}

/// Each reaction on `row` learns whether one of `viewer` chose it.
fn mark_reacted(ctx: &QueryCtx, viewer: &[Principal], row: &mut MsgRow) {
    for reaction in &mut row.reactions {
        reaction.reacted_by_me = viewer.iter().any(|principal| {
            let key = (
                row.channel_id.clone(),
                row.seq,
                reaction.emoji.clone(),
                principal.clone(),
            );
            REACTIONS.has(ctx, &key)
        });
    }
}
