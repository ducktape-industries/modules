//! Typed reads of valset, folded into one row per key.
use ducktape_view_guest::Host;
use ducktape_view_guest::host::{Error, all_pages};
use serde::{Deserialize, Serialize};
use valset::{Membership, PageRequest, Role, ask};

/// One member of the network, once: its key, the address it is reached at
/// (empty for a seated key valset holds no membership for), whether it
/// validates, and its seat, the place the consensus set answers it in.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Node {
    pub key: Vec<u8>,
    pub address: String,
    pub validator: bool,
    pub seat: Option<u32>,
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
        });
    }
    for key in validators {
        if !nodes.iter().any(|node| &node.key == key) {
            nodes.push(Node {
                key: key.clone(),
                address: String::new(),
                validator: true,
                seat: seat(key),
            });
        }
    }
    nodes.sort_by_key(|node| (!node.validator, node.seat.unwrap_or(u32::MAX)));
    nodes
}

/// The set, read twice: the consensus keys the program answers, then every
/// membership behind them.
pub(crate) async fn nodes(host: Host) -> Result<Vec<Node>, Error> {
    let validators = host.query(ask::Validators).await?;
    let members = all_pages(None, |after| {
        let ask = host.query(ask::Memberships {
            page: PageRequest { after, limit: None },
        });
        async move {
            let reply = ask.await?;
            Ok((reply.items, reply.next))
        }
    })
    .await?;
    Ok(fold(&validators, members))
}
