//! Typed reads of the system programs: accounts (identity), validators
//! (valset), programs (programs), how much each account used each program
//! and when its keys last signed (activity). The transaction lists are
//! `recent.rs`; the blocks are the host's (`load.rs`).
use view::prelude::*;

use crate::state::{Accounts, Devices, Network, Used};

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

/// When each of `account`'s keys last signed: the newest frame the index
/// keeps under each.
pub(crate) async fn devices(host: Host, account: u64) -> Result<Devices, Error> {
    let Some(found) = host.query(identity::ask::Get { number: account }).await? else {
        return Ok(Devices {
            account,
            last_used: Vec::new(),
        });
    };
    let mut last_used = Vec::with_capacity(found.keys().len());
    for key in found.keys() {
        let newest = activity::ask::Signed {
            key: key.key.clone(),
            page: activity::PageRequest::first(1),
        };
        let page = host.query(newest).await?;
        last_used.push(page.items.first().map(|frame| frame.time));
    }
    Ok(Devices { account, last_used })
}
