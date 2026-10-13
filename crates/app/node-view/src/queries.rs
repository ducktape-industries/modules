//! Typed reads of valset, folded into one row per key; who operates each
//! node, as identity says; and what the vote needs of the registry, with
//! the names of the accounts it shows.
use std::collections::BTreeMap;

use ducktape_view_guest::prelude::*;
use identity::Kind;
use module_registry::Entry;
use serde::{Deserialize, Serialize};
use valset::{Membership, Motion, PageRequest, Pending, Role, ask};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Node {
    pub key: Vec<u8>,
    pub address: Option<String>,
    pub validator: bool,
    pub seat: Option<u32>,
    /// "operated by <manager> (<agent>)" for a key on an agent, or why
    /// identity would not say
    pub operator: Option<String>,
}

/// The consensus keys and the memberships, as one list: validators in seat
/// order, then residents in the order valset keeps them. Each key once.
pub fn fold(validators: &[Vec<u8>], members: Vec<Membership>) -> Vec<Node> {
    let seat = |key: &[u8]| {
        validators
            .iter()
            .position(|seated| seated == key)
            .map(|at| at as u32)
    };
    let mut nodes: Vec<Node> = Vec::with_capacity(members.len());
    for member in members {
        if nodes.iter().any(|node| node.key == member.key) {
            continue;
        }
        nodes.push(Node {
            seat: seat(&member.key),
            validator: member.role == Role::Validator,
            key: member.key,
            address: member.address,
            operator: None,
        });
    }
    for key in validators {
        if !nodes.iter().any(|node| &node.key == key) {
            nodes.push(Node {
                key: key.clone(),
                address: None,
                validator: true,
                seat: seat(key),
                operator: None,
            });
        }
    }
    nodes.sort_by_key(|node| (!node.validator, node.seat.unwrap_or(u32::MAX)));
    nodes
}

/// The set, read twice: the consensus keys the program answers, then every
/// membership behind them; then who operates each node.
pub(crate) async fn nodes(host: Host) -> Result<Vec<Node>, Error> {
    let validators = host.query(ask::Validators).await?;
    let members = host
        .query_all(|after| ask::Memberships {
            page: PageRequest { after, limit: None },
        })
        .await?;
    let mut nodes = fold(&validators, members);
    // one row's operator unread is that row's line: the list stands
    for node in &mut nodes {
        node.operator = operator(&host, &node.key)
            .await
            .unwrap_or_else(|refusal| Some(format!("operator unreadable: {}", refusal.message)));
    }
    Ok(nodes)
}

/// Who operates the node whose key is `key`, through the identity role as
/// valset's vote reads it (`Account`, then `Profile`): "operated by
/// <manager> (<agent>)" for a key on an agent. `None` for a key on a
/// person or on no account, and for an agent that does not act now
/// (refused `unauthorized`); any other refusal is the caller's.
pub(crate) async fn operator(host: &Host, key: &[u8]) -> Result<Option<String>, Error> {
    let account = match host.query(identity::ask::OfKey { key: key.to_vec() }).await {
        Ok(account) => account,
        Err(refusal) if refusal.code == abi::reason::UNAUTHORIZED => return Ok(None),
        Err(refusal) => return Err(refusal),
    };
    let Some(number) = account else {
        return Ok(None);
    };
    let Some(agent) = host.query(identity::ask::Profile { number }).await? else {
        return Ok(None);
    };
    let Kind::Managed { manager, .. } = agent.kind else {
        return Ok(None);
    };
    let manager = host
        .query(identity::ask::Profile { number: manager })
        .await?
        .map_or_else(|| format!("account {manager}"), |profile| profile.name);
    Ok(Some(format!("operated by {manager} ({})", agent.name)))
}

/// The motions waiting for votes, how many votes pass one, and the name of
/// each account a grant or a revoke among them names.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Motions {
    pub(crate) needed: u64,
    pub(crate) pending: Vec<Pending>,
    pub(crate) names: Names,
}

/// Accounts by number, as identity names them; one it does not profile
/// is left out, and reads as `account N`.
pub(crate) type Names = BTreeMap<u64, String>;

/// The registry's changes still waiting, by height, and the name of each
/// account whose grant made one.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Schedule {
    pub(crate) changes: Vec<module_registry::Pending>,
    pub(crate) names: Names,
}

async fn names(host: &Host, accounts: impl IntoIterator<Item = u64>) -> Result<Names, Error> {
    let mut names = Names::new();
    for number in accounts {
        if names.contains_key(&number) {
            continue;
        }
        if let Some(profile) = host.query(identity::ask::Profile { number }).await? {
            names.insert(number, profile.name);
        }
    }
    Ok(names)
}

/// The validators the seated key votes for: empty, it does not vote.
pub(crate) async fn voices(host: Host, key: Vec<u8>) -> Result<Vec<Vec<u8>>, Error> {
    host.query(ask::Voices { key }).await
}

/// The motions waiting. ponytail: one page (256 motions); page on when a
/// network keeps more open at once.
pub(crate) async fn motions(host: Host) -> Result<Motions, Error> {
    let (needed, page) = host
        .query(ask::Motions {
            page: PageRequest {
                after: None,
                limit: None,
            },
        })
        .await?;
    let named = page
        .items
        .iter()
        .filter_map(|pending| match &pending.motion {
            Motion::Registry { op } => match abi::decode(op) {
                Ok(module_registry::Op::Grant(grant) | module_registry::Op::Revoke(grant)) => {
                    Some(grant.account)
                }
                _ => None,
            },
            _ => None,
        });
    let names = names(&host, named.collect::<Vec<_>>()).await?;
    Ok(Motions {
        needed,
        pending: page.items,
        names,
    })
}

/// The programs the registry runs now, each with its code and params.
pub(crate) async fn roster(host: Host) -> Result<Vec<Entry>, Error> {
    let (height, _) = host
        .query(module_registry::ask::Program(
            module_registry::MODULE.into(),
        ))
        .await?;
    host.query(module_registry::ask::At(height)).await
}

/// Every change the registry holds scheduled, by height, and who made it.
pub(crate) async fn scheduled(host: Host) -> Result<Schedule, Error> {
    let changes: Vec<module_registry::Pending> = host
        .query_all(|after| module_registry::ask::Scheduled {
            page: PageRequest { after, limit: None },
        })
        .await?;
    let names = names(&host, changes.iter().filter_map(|change| change.by)).await?;
    Ok(Schedule { changes, names })
}
