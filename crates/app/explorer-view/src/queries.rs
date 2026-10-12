//! Typed reads of the system programs: accounts (identity), validators
//! (valset), programs (module-registry) and what each account did
//! (activity). The window's blocks are the host's (`load.rs`).
use ducktape_view_guest::prelude::*;
use module_registry as registry;

use crate::chain::{TxRow, rows};
use crate::state::{Accounts, Acted, Network, Used};
use crate::ui::LIST_ROWS;

/// Every account.
pub(crate) async fn accounts(host: Host) -> Result<Accounts, Error> {
    let list = host
        .query_all(|after| identity::ask::List {
            page: identity::PageRequest { after, limit: None },
        })
        .await?;
    Ok(Accounts { list })
}

pub(crate) async fn validators(host: Host) -> Result<Vec<Vec<u8>>, Error> {
    host.query(valset::ask::Validators).await
}

/// What the registry runs and lists, and what it will.
pub(crate) async fn network(host: Host) -> Result<Network, Error> {
    let tip = host.ask::<ChainStatus>(()).await?.height;
    let programs = host.query(registry::ask::At(tip)).await?;
    let views = host.query(registry::ask::Views(tip)).await?;
    let mut changes = host
        .query_all(|after| registry::ask::Scheduled {
            page: registry::PageRequest { after, limit: None },
        })
        .await?;
    changes.retain(|change| change.height > tip);
    Ok(Network {
        programs,
        views,
        changes,
    })
}

pub(crate) async fn used(host: Host) -> Result<Used, Error> {
    let list = host
        .query_all(|after| activity::ask::Used {
            page: activity::PageRequest { after, limit: None },
        })
        .await?;
    Ok(Used { list })
}

pub(crate) async fn acted(host: Host, account: u64, known: Vec<TxRow>) -> Result<Acted, Error> {
    let asked = activity::ask::Acted {
        account,
        page: activity::PageRequest::first(LIST_ROWS as u64),
    };
    let frames = host.query(asked).await?.items;
    let mut block: Vec<TxRow> = Vec::new();
    let mut acted = Vec::with_capacity(frames.len());
    for frame in frames {
        let at = |row: &&TxRow| (row.height, row.index) == (frame.height, frame.position);
        if let Some(row) = known.iter().find(at) {
            acted.push(row.clone());
            continue;
        }
        let held = block.first().is_some_and(|row| row.height == frame.height);
        if !held {
            let read = host
                .ask::<ChainBlock>(BlockRef::Height(frame.height))
                .await?;
            block = read.map(|read| rows(read).1).unwrap_or_default();
        }
        acted.extend(block.iter().find(at).cloned());
    }
    Ok(Acted {
        account,
        rows: acted,
    })
}
