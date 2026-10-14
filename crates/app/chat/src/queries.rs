//! One function per [`Query`](crate::Query), each named by [`Chat::query`](crate::Chat)'s
//! match and each a read of the tables in `state.rs`. Chat's listings only grow, so a page cursor from any height
//! resumes where it left off.
use ::program::Range;
use ::program::{Error, QueryCtx, capacity, invalid};
use store::{PAGE_BYTES, PageRequest, PageResponse, within};

use crate::state::{
    ANSWERED, CHANNEL_TAGS, CHANNELS, HEADS, MESSAGE_IDS, MESSAGES, REACTIONS, REPLIES, ROOTS,
    TAGS, WORDS, message, messages, newest_first,
};
use crate::text::tag_label;
use crate::{
    ChannelInfo, ChannelRow, MAX_VIEWERS, MessageHits, MsgRow, Principal, Reply,
    SEARCH_POSTING_CAP, tokens,
};

/// `reply` as `viewer` reads it: each reaction learns whether one of
/// `viewer` chose it.
pub(crate) fn seen_by(
    ctx: &QueryCtx,
    mut reply: Reply,
    viewer: Vec<Principal>,
) -> Result<Reply, Error> {
    if viewer.len() > MAX_VIEWERS {
        return Err(capacity(format!(
            "a read names at most {MAX_VIEWERS} viewers, not {}",
            viewer.len()
        )));
    }
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
    let rows = ROOTS.range_within(
        ctx,
        &channel_id,
        page,
        PAGE_BYTES,
        |(channel_id, newest)| MESSAGES.get(ctx, &(channel_id, newest_first(newest))),
    )?;
    Ok(rows.flatten())
}

pub(crate) fn thread(
    ctx: &QueryCtx,
    channel_id: String,
    root: u64,
    page: &PageRequest,
) -> Result<Reply, Error> {
    let thread = (channel_id.clone(), root);
    let replies =
        REPLIES.range_within(ctx, &thread, page, PAGE_BYTES, |(channel_id, _, reply)| {
            MESSAGES.get(ctx, &(channel_id, reply))
        })?;
    Ok(Reply::Thread {
        root: MESSAGES.get(ctx, &thread)?,
        replies: replies.flatten(),
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
    let mut rows: Vec<MsgRow> = MESSAGES
        .scan(ctx, Range::new(lo, Some(hi)))?
        .into_iter()
        .map(|(_, row)| row)
        .collect();
    rows.sort_by_key(|row| (row.seq.abs_diff(seq), row.seq < seq));
    let (mut nearest, _) = within(rows, PAGE_BYTES);
    nearest.sort_by_key(|row| row.seq);
    Ok(nearest)
}

/// The messages holding every word of `text`, newest first. The first
/// word's postings are read (at most [`SEARCH_POSTING_CAP`]) and the rest
/// checked on each row.
pub(crate) fn search(
    ctx: &QueryCtx,
    text: &str,
    channel_id: Option<String>,
    page: &PageRequest,
) -> Result<MessageHits, Error> {
    let wanted = tokens(text);
    let Some(first) = wanted.first().cloned() else {
        return Err(invalid("nothing to search for"));
    };
    // ponytail: one posting list scanned, the rest filtered on the row;
    // intersect postings if search volume ever matters.
    let postings = match channel_id {
        Some(channel_id) => WORDS.prefix_of(&(first, channel_id)),
        None => WORDS.prefix_of(&first),
    };
    let postings = WORDS.scan(ctx, postings.limit(SEARCH_POSTING_CAP as u64 + 1))?;
    let capped = postings.len() > SEARCH_POSTING_CAP;
    let at = postings
        .into_iter()
        .map(|(_, channel_id, seq)| (channel_id, seq));
    let mut hits: Vec<MsgRow> = messages(ctx, at)?
        .into_iter()
        .filter(|row| wanted.is_subset(&tokens(&row.text)))
        .collect();
    hits.sort_by(|a, b| b.time.cmp(&a.time).then(b.seq.cmp(&a.seq)));
    let limit = page.limit() as usize;
    let capped = capped || hits.len() > limit;
    hits.truncate(limit);
    let (hits, cut) = within(hits, PAGE_BYTES);
    Ok(MessageHits {
        hits,
        capped: capped || cut,
    })
}

/// One page of the messages tagged `tag`, newest first.
pub(crate) fn tagged(
    ctx: &QueryCtx,
    tag: &str,
    channel_id: Option<String>,
    page: &PageRequest,
) -> Result<PageResponse<MsgRow>, Error> {
    let label = tag_label(tag);
    let rows = match channel_id {
        Some(channel_id) => CHANNEL_TAGS.range_within(
            ctx,
            &(channel_id, label),
            page,
            PAGE_BYTES,
            |(channel_id, _, newest)| MESSAGES.get(ctx, &(channel_id, newest_first(newest))),
        )?,
        None => TAGS.range_within(ctx, &label, page, PAGE_BYTES, |(_, _, channel_id, seq)| {
            MESSAGES.get(ctx, &(channel_id, seq))
        })?,
    };
    Ok(rows.flatten())
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
