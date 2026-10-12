//! Typed reads of the system programs: accounts (identity), validators
//! (valset) and programs (module-registry). The window's blocks are the
//! host's (`load.rs`).
use ducktape_view_guest::prelude::*;
use module_registry as registry;

use crate::state::{Accounts, Network};

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
