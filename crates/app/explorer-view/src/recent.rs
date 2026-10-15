//! The lists `activity` answers, read a page at a time as far as they are
//! scrolled ([`Paged`]): the chain's latest frames, one program's, and an
//! account's. A row is drawn from the index; the block it sits in is read
//! when the row is drawn (`Explorer::locate`), its whole payload when it
//! is opened (`Explorer::open`).
use view::Paged;
use view::host::Page;
use view::prelude::*;

use crate::{Explorer, PAGE};

/// A frame as the index lists it: where it sits, when, to what, who
/// signed it (an account's own list leaves that out) and whether it ran.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct IndexRow {
    pub height: u64,
    pub position: u32,
    pub time: u64,
    pub target: String,
    pub signer: Option<Vec<u8>>,
    pub applied: bool,
}

impl From<activity::Row> for IndexRow {
    fn from(row: activity::Row) -> Self {
        IndexRow {
            signer: Some(row.signer),
            ..row.frame.into()
        }
    }
}

impl From<activity::Frame> for IndexRow {
    fn from(frame: activity::Frame) -> Self {
        IndexRow {
            height: frame.height,
            position: frame.position,
            time: frame.time,
            target: frame.target,
            signer: None,
            applied: frame.applied,
        }
    }
}

/// One list the index answers, and the redraw each of its pages brings.
pub(crate) struct Listing {
    pub(crate) rows: Entity<Paged<IndexRow>>,
    _landed: Subscription,
}

impl Listing {
    pub(crate) fn new<F>(
        ask: impl Fn(Option<Vec<u8>>) -> F + 'static,
        cx: &mut Context<Explorer>,
    ) -> Listing
    where
        F: Future<Output = Result<Page<IndexRow>, Error>> + 'static,
    {
        let rows = cx.new(|cx| Paged::new(ask, cx));
        let landed = cx.observe(&rows, |_, _, cx| cx.notify());
        Listing {
            rows,
            _landed: landed,
        }
    }

    /// The listing from its start again, as far as it was read: what a
    /// frame the index just kept means to it.
    pub(crate) fn reread(&self, cx: &mut Context<Explorer>) {
        self.rows.update(cx, |rows, cx| rows.reread(cx));
    }

    /// The first `most` rows held, or all of them.
    pub(crate) fn held(&self, most: Option<usize>) -> Vec<IndexRow> {
        self.rows.read(|rows| {
            rows.rows()
                .iter()
                .take(most.unwrap_or(usize::MAX))
                .cloned()
                .collect()
        })
    }
}

/// A page of the chain's frames, or of `program`'s.
pub(crate) async fn recent(
    host: Host,
    program: Option<String>,
    after: Option<Vec<u8>>,
) -> Result<Page<IndexRow>, Error> {
    let page = host
        .query(activity::ask::Recent {
            program,
            page: activity::PageRequest::resume(after, PAGE),
        })
        .await?;
    Ok((page.items.into_iter().map(Into::into).collect(), page.next))
}

/// A page of what `account` did.
pub(crate) async fn acted(
    host: Host,
    account: u64,
    after: Option<Vec<u8>>,
) -> Result<Page<IndexRow>, Error> {
    let page = host
        .query(activity::ask::Acted {
            account,
            page: activity::PageRequest::resume(after, PAGE),
        })
        .await?;
    Ok((page.items.into_iter().map(Into::into).collect(), page.next))
}
