//! Typed reads of the chat module. Every list takes a `PageRequest` and answers a
//! `PageResponse`; `next` is the cursor of the page after it.
use chat::view::Names;
use chat::{ChannelInfo, MemberRow, MessageHits, MsgRow, PageRequest, Principal, ask};
use ducktape_view_guest::Host;
use ducktape_view_guest::host::Error;

use crate::{PAGE, WINDOW};

fn page(after: Option<Vec<u8>>, limit: usize) -> PageRequest {
    PageRequest {
        after,
        limit: Some(limit as u64),
    }
}

/// Every room.
pub(crate) async fn channels(host: Host) -> Result<Vec<ChannelInfo>, Error> {
    host.query_all(|after| ask::Channels {
        page: page(after, PAGE),
    })
    .await
}

/// The `limit` roots below `below` (or the newest), oldest first, with
/// whether older ones remain.
pub(crate) async fn roots(
    host: Host,
    channel_id: String,
    viewer: Vec<Principal>,
    below: Option<Vec<u8>>,
    limit: usize,
) -> Result<(Vec<MsgRow>, bool), Error> {
    let ask = move |ask| host.query(ask);
    ::chat::view::roots(ask, channel_id, viewer, below, limit, PAGE as u64).await
}

/// Every account's profile, folded into [`Names`].
pub(crate) async fn roster(host: Host) -> Result<Names, Error> {
    ::chat::view::roster(move |ask| host.query(ask)).await
}

/// The rows around a landing seq, oldest first.
pub(crate) async fn around(
    host: Host,
    channel_id: String,
    seq: u64,
    viewer: Vec<Principal>,
) -> Result<Vec<MsgRow>, Error> {
    let rows = host
        .query(ask::MessagesAround {
            channel_id,
            seq,
            viewer,
            page: page(None, WINDOW / 2),
        })
        .await?;
    Ok(sorted(rows))
}

pub(crate) fn sorted(mut rows: Vec<MsgRow>) -> Vec<MsgRow> {
    rows.sort_by_key(|row| row.seq);
    rows
}

/// Every member of a room, in account order.
pub(crate) async fn members(host: Host, channel_id: String) -> Result<Vec<MemberRow>, Error> {
    host.query_all(|after| ask::Members {
        channel_id: channel_id.clone(),
        page: page(after, WINDOW),
    })
    .await
}

/// One page of a thread's replies after `after`, and the cursor to page on.
pub(crate) async fn thread(
    host: Host,
    channel_id: String,
    root_seq: u64,
    viewer: Vec<Principal>,
    after: Option<Vec<u8>>,
) -> Result<(Vec<MsgRow>, Option<Vec<u8>>), Error> {
    let (_root, replies) = host
        .query(ask::Thread {
            channel_id,
            root_seq,
            viewer,
            page: page(after, WINDOW),
        })
        .await?;
    Ok((sorted(replies.items), replies.next))
}

/// A search: `#tag` pages through the tag index, anything else is a
/// full-text search capped by the module. Returns the hits, whether the
/// search was capped, and the cursor of the next tag page.
pub(crate) async fn search_hits(
    host: Host,
    text: String,
    channel_id: Option<String>,
    viewer: Vec<Principal>,
    after: Option<Vec<u8>>,
) -> Result<(Vec<MsgRow>, bool, Option<Vec<u8>>), Error> {
    match text.strip_prefix('#') {
        Some(tag) if !tag.is_empty() => {
            let tagged = host
                .query(ask::TagSearch {
                    tag: tag.to_owned(),
                    viewer,
                    channel_id,
                    page: page(after, PAGE),
                })
                .await?;
            Ok((tagged.items, false, tagged.next))
        }
        _ => {
            let MessageHits { hits, capped } = host
                .query(ask::Search {
                    text,
                    viewer,
                    channel_id,
                    page: page(None, PAGE),
                })
                .await?;
            Ok((hits, capped, None))
        }
    }
}
