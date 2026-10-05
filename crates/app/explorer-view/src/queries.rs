//! Typed reads of the system programs: accounts (identity), validators
//! (valset) and programs (module-registry). The window's blocks are the
//! host's (`load.rs`).
use ducktape_view_guest::Host;
use ducktape_view_guest::host::Error;
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
///
/// `At(0)` is the folded set: the registry applies the changes due at or
/// before the height asked, and it answers no height of its own, so there is
/// no "as of now" to ask for — the scheduled list is what is still to come.
pub(crate) async fn network(host: Host) -> Result<Network, Error> {
    let programs = host.query(registry::ask::At(0)).await?;
    let views = host.query(registry::ask::Views(0)).await?;
    let changes = host
        .query_all(|after| registry::ask::Scheduled {
            page: registry::PageRequest { after, limit: None },
        })
        .await?;
    Ok(Network {
        programs,
        views,
        changes,
    })
}
