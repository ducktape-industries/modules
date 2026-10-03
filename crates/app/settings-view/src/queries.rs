//! Typed reads: who the seated key is (identity), and whether each of its
//! keys validates (valset). Rows keep what the programs said; they are
//! worded only when drawn.
use ducktape_view_guest::Host;
use ducktape_view_guest::host::{Error, malformed, pages};
use identity::{Control, Kind, PageRequest, Reference, Standing};
use serde::{Deserialize, Serialize};

/// Who the seated key is.
#[derive(Clone, PartialEq, Serialize, Deserialize)]
pub enum Seat {
    /// a key no account holds
    Bare(Key),
    /// a key held by an agent that does not act: the agent's name and why
    Stopped {
        key: Key,
        name: String,
        note: String,
    },
    Account(Account),
}

#[derive(Clone, PartialEq, Serialize, Deserialize)]
pub struct Account {
    pub number: u64,
    pub name: String,
    /// what identity says it is: a person, an agent, a module
    #[serde(with = "ducktape_view_guest::borsh_bytes")]
    pub kind: Kind,
    pub keys: Vec<Key>,
    /// a person's account manages agents; an agent's or a module's none
    pub manages: bool,
    pub agents: Vec<Agent>,
}

/// An agent the account manages.
#[derive(Clone, PartialEq, Serialize, Deserialize)]
pub struct Agent {
    pub number: u64,
    pub name: String,
    pub keys: usize,
    /// what identity says it is: always `Kind::Managed` (checked on read)
    #[serde(with = "ducktape_view_guest::borsh_bytes")]
    pub kind: Kind,
}

impl Agent {
    pub fn standing(&self) -> Standing {
        match self.kind {
            Kind::Managed { standing, .. } => standing,
            Kind::Person | Kind::Module(_) => Standing::Active,
        }
    }
}

#[derive(Clone, PartialEq, Serialize, Deserialize)]
pub struct Key {
    pub label: Option<String>,
    pub key: Vec<u8>,
    pub validator: bool,
}

/// Who the seated `signer` (hex) is: its account as the host resolved it
/// (`number`), with every key on it, or the key alone while it holds none.
pub(crate) async fn account(
    host: Host,
    signer: String,
    number: Option<u64>,
) -> Result<Option<Seat>, Error> {
    if signer.is_empty() {
        return Ok(None);
    }
    let key = abi::unhex(&signer)
        .ok_or_else(|| malformed("the session's signer is not hexadecimal".into()))?;
    let Some(number) = number else {
        // the host resolves no account for a suspended agent's key, which
        // identity still holds (and so refuses to create an account with)
        let held = held_by(&host, &key).await?;
        let key = read_key(&host, key, None).await?;
        return Ok(Some(match held {
            Some((name, note)) => Seat::Stopped {
                key,
                note: format!("This key belongs to {name}, {note} by its manager."),
                name,
            },
            None => Seat::Bare(key),
        }));
    };
    let Some(account) = host.query(identity::ask::Get { number }).await? else {
        return Ok(None);
    };
    let mut keys = Vec::new();
    for key in account.keys() {
        keys.push(read_key(&host, key.key.clone(), key.label.clone()).await?);
    }
    let manages = matches!(account.control, Control::Person { .. });
    let agents = if manages {
        agents(&host, number).await?
    } else {
        Vec::new()
    };
    Ok(Some(Seat::Account(Account {
        number,
        kind: account.kind(),
        name: account.card.name,
        keys,
        manages,
        agents,
    })))
}

/// The account holding `key` that does not act, by its name and why not
/// (`Kind::note`); `None` for a key no account holds.
async fn held_by(host: &Host, key: &[u8]) -> Result<Option<(String, &'static str)>, Error> {
    let references = vec![Reference::Key(key.to_vec())];
    let resolved = host.query(identity::ask::Resolve { references }).await?;
    let Some(number) = resolved.into_iter().next().flatten() else {
        return Ok(None);
    };
    let profile = host.query(identity::ask::Profile { number }).await?;
    Ok(profile.and_then(|p| Some((p.name, p.kind.note()?))))
}

/// Every agent `manager` manages.
async fn agents(host: &Host, manager: u64) -> Result<Vec<Agent>, Error> {
    let listed = pages(None, |after| {
        let ask = host.query(identity::ask::Managed {
            by: manager,
            page: PageRequest { after, limit: None },
        });
        async move {
            let reply = ask.await?;
            Ok((reply.items, reply.next))
        }
    })
    .await?;
    let agents = listed
        .into_iter()
        .map(|agent| match agent.kind() {
            kind @ Kind::Managed { .. } => Ok(Agent {
                number: agent.number,
                keys: agent.keys().len(),
                name: agent.card.name,
                kind,
            }),
            Kind::Person | Kind::Module(_) => Err(malformed(format!(
                "identity lists account {} as managed, and it is not",
                agent.number
            ))),
        })
        .collect::<Result<_, _>>()?;
    Ok(agents)
}

async fn read_key(host: &Host, key: Vec<u8>, label: Option<String>) -> Result<Key, Error> {
    let membership = host
        .query(valset::ask::Membership { key: key.clone() })
        .await?;
    Ok(Key {
        label,
        key,
        validator: membership.is_some_and(|m| m.role == valset::Role::Validator),
    })
}
