//! Typed reads of `files` and identity. A folder or a file is read at one
//! snapshot: the first answer names it, and every later page and read of
//! the same screen asks that snapshot, so a commit landing in between never
//! mixes two trees on one page.
use files::{BlobId, PageRequest};
use view::prelude::*;

use crate::state::{Changed, Folder, HISTORY_PAGE, History, Names, Opened, PREVIEW, ROOT};

/// A folder's entries, every page of them, and who writes it.
pub(crate) async fn folder(host: Host, path: String) -> Result<Folder, Error> {
    let mut snapshot = None;
    let mut entries = Vec::new();
    let mut after = None;
    loop {
        let (at, page) = host
            .query(files::ask::List {
                path: path.clone(),
                at: snapshot,
                page: PageRequest { after, limit: None },
            })
            .await?;
        snapshot = at;
        entries.extend(page.items);
        match page.next {
            Some(next) => after = Some(next),
            None => break,
        }
    }
    // folders first, each kind by name as `files` keeps a tree
    entries.sort_by_key(|entry| !matches!(entry.node, files::Node::Dir { .. }));
    let ownership = ownership(&host, &path, snapshot).await?;
    Ok(Folder {
        path,
        snapshot,
        entries,
        ownership,
    })
}

/// Who writes `path`; `/` names nothing a change can, so it has no answer.
async fn ownership(
    host: &Host,
    path: &str,
    at: Option<BlobId>,
) -> Result<Option<files::PathOwnership>, Error> {
    if path == ROOT {
        return Ok(None);
    }
    let path = path.to_owned();
    host.query(files::ask::Ownership { path, at })
        .await
        .map(Some)
}

/// What is at `path`, its first [`PREVIEW`] bytes, and who writes it.
pub(crate) async fn file(host: Host, path: String) -> Result<Opened, Error> {
    let (snapshot, entry) = host
        .query(files::ask::Stat {
            path: path.clone(),
            at: None,
        })
        .await?;
    let readable = entry
        .as_ref()
        .is_some_and(|entry| !matches!(entry.node, files::Node::Dir { .. }));
    let (bytes, whole) = match readable {
        true => {
            let contents = host
                .query(files::ask::Read {
                    path: path.clone(),
                    at: snapshot,
                    offset: 0,
                    len: PREVIEW,
                })
                .await?;
            (contents.bytes, contents.eof)
        }
        false => (Vec::new(), true),
    };
    let ownership = match entry {
        Some(_) => ownership(&host, &path, snapshot).await?,
        None => None,
    };
    Ok(Opened {
        path,
        snapshot,
        entry,
        bytes,
        whole,
        ownership,
    })
}

/// The newest [`HISTORY_PAGE`] snapshots, and every pin.
pub(crate) async fn history(host: Host) -> Result<History, Error> {
    let newest = host
        .query(files::ask::History {
            page: PageRequest::first(HISTORY_PAGE),
        })
        .await?;
    let pins = host
        .query_all(|after| files::ask::Pins {
            page: PageRequest { after, limit: None },
        })
        .await?;
    Ok(History {
        snapshots: newest.items,
        older: newest.next.is_some(),
        pins,
    })
}

/// What the snapshot `id` changed against its `parent`; the first
/// snapshot's whole tree is new, which this does not list.
pub(crate) async fn changed(
    host: Host,
    seq: u64,
    id: BlobId,
    parent: Option<BlobId>,
) -> Result<Changed, Error> {
    let entries = match parent {
        Some(from) => {
            let to = id;
            let prefix = String::new();
            host.query(files::ask::Diff { from, to, prefix }).await?
        }
        None => Vec::new(),
    };
    Ok(Changed { seq, entries })
}

/// The name identity gives `account`, if any.
pub(crate) async fn name(host: Host, account: u64) -> Result<Option<String>, Error> {
    let profile = host
        .query(identity::ask::Profile { number: account })
        .await?;
    Ok(profile.map(|profile| profile.name))
}

/// Every account's name.
pub(crate) async fn names(host: Host) -> Result<Names, Error> {
    let accounts = host
        .query_all(|after| identity::ask::List {
            page: identity::PageRequest { after, limit: None },
        })
        .await?;
    let list = accounts
        .into_iter()
        .map(|account| (account.number, account.card.name))
        .collect();
    Ok(Names { list })
}
