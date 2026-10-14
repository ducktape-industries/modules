//! Typed reads of the system programs: accounts (identity), validators
//! (valset), programs (programs) and what each account did
//! (activity). The window's blocks are the host's (`load.rs`).
use view::prelude::*;

use crate::chain::{ROW_PAYLOAD, TxRow, located};
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

/// What the programs role runs and lists at its height, and what it will
/// after it; who may change it. A change keeps its place in the schedule
/// until the programs role's next op folds it, landed or not, so one due by that
/// height is left out: it is in the roster.
pub(crate) async fn network(host: Host) -> Result<Network, Error> {
    let (height, programs, views) = host.query(programs::ask::Roster).await?;
    let hooks = host.query(programs::ask::Hooks(height)).await?;
    let roles = host.query(programs::ask::Roles).await?;
    let grants = host
        .query_all(|after| programs::ask::Grants {
            page: programs::PageRequest { after, limit: None },
        })
        .await?;
    let mut changes = host
        .query_all(|after| programs::ask::Scheduled {
            page: programs::PageRequest { after, limit: None },
        })
        .await?;
    changes.retain(|change| change.height > height);
    Ok(Network {
        programs,
        views,
        changes,
        hooks,
        grants,
        roles: Some(roles),
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
    let mut acted = Vec::with_capacity(frames.len());
    for frame in frames {
        let at = |row: &&TxRow| (row.height, row.index) == (frame.height, frame.position);
        if let Some(row) = known.iter().find(at) {
            acted.push(row.clone());
            continue;
        }
        let ask = TxAsk {
            tx: TxRef::At {
                height: frame.height,
                position: frame.position,
            },
            most: ROW_PAYLOAD,
        };
        acted.extend(host.ask::<ChainTx>(ask).await?.map(located));
    }
    Ok(Acted {
        account,
        rows: acted,
    })
}
