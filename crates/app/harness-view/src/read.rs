//! Every read the screens make: the roster (harness's agents, identity's accounts, each
//! agent's threads, and how many member nodes serve each service and relay), one agent in
//! full, one thread's transcript, and the services the create form offers.
use std::collections::{HashMap, HashSet};

use egress::{RELAY, ServiceRow};
use harness::{AgentRow, Body, CallRow, Frame, ThreadRow, ToolInfo, Usage, WithheldFrame};
use identity::{Audience, Category, Kind, PageRequest, Standing};
use view::prelude::*;

fn page(after: Option<Vec<u8>>) -> PageRequest {
    PageRequest { after, limit: None }
}

#[derive(PartialEq)]
pub(crate) struct Roster {
    pub agents: Vec<Card>,
    /// the reader's active agents whose script calls harness that harness has not hired
    pub unhired: Vec<Unhired>,
    pub names: HashMap<u64, String>,
    /// member nodes that serve relay: no tool call runs without one
    pub relaying: usize,
}

/// One hired agent as the list draws it.
#[derive(PartialEq)]
pub(crate) struct Card {
    pub row: AgentRow,
    /// newest first
    pub threads: Vec<ThreadRow>,
    /// member nodes that serve the agent's service
    pub serving: usize,
    /// "suspended" or "revoked", for an agent that is
    pub note: Option<&'static str>,
}

#[derive(PartialEq)]
pub(crate) struct Unhired {
    pub number: u64,
    pub name: String,
}

impl Roster {
    pub fn name(&self, number: u64) -> String {
        self.names
            .get(&number)
            .cloned()
            .unwrap_or_else(|| format!("account {number}"))
    }

    pub fn card(&self, agent: u64) -> Option<&Card> {
        self.agents.iter().find(|card| card.row.agent == agent)
    }
}

pub(crate) async fn roster(host: Host, me: Option<u64>) -> Result<Roster, Error> {
    let rows = host
        .query_all(|after| harness::ask::Agents { page: page(after) })
        .await?;
    let accounts = host
        .query_all(|after| identity::ask::List { page: page(after) })
        .await?;
    let mut serving = HashMap::new();
    let services = rows.iter().map(|row| row.service.as_str()).chain([RELAY]);
    for service in services {
        if !serving.contains_key(service) {
            serving.insert(service.to_owned(), served(&host, service).await?);
        }
    }
    let kinds: HashMap<u64, Kind> = accounts
        .iter()
        .map(|account| (account.number, account.kind()))
        .collect();
    let mut agents = Vec::new();
    // ponytail: one Threads listing per agent; a harness query of every agent's live state
    // when networks run many agents
    for row in rows {
        let agent = row.agent;
        let threads = host
            .query_all(|after| harness::ask::Threads {
                agent,
                page: page(after),
            })
            .await?;
        agents.push(Card {
            serving: serving[&row.service],
            note: kinds.get(&agent).and_then(Kind::note),
            threads,
            row,
        });
    }
    let hired: HashSet<u64> = agents.iter().map(|card| card.row.agent).collect();
    let unhired = accounts
        .iter()
        .filter(|account| match account.kind() {
            Kind::Program {
                manager,
                category: Category::Agent,
                standing: Standing::Active,
                script: Some(script),
            } => Some(manager) == me && script.calls == harness::PROGRAM,
            Kind::User | Kind::Program { .. } | Kind::System(_) => false,
        })
        .filter(|account| !hired.contains(&account.number))
        .map(|account| Unhired {
            number: account.number,
            name: account.card.name.clone(),
        })
        .collect();
    Ok(Roster {
        agents,
        unhired,
        names: accounts
            .into_iter()
            .map(|account| (account.number, account.card.name))
            .collect(),
        relaying: serving[RELAY],
    })
}

/// How many member nodes the chain would elect for `service` now.
async fn served(host: &Host, service: &str) -> Result<usize, Error> {
    let asked = egress::ask::Serving {
        service: service.to_owned(),
    };
    Ok(host.query(asked).await?.len())
}

async fn frames(host: &Host, thread: u64) -> Result<Vec<Frame>, Error> {
    host.query_all(|after| harness::ask::Frames {
        thread,
        page: page(after),
    })
    .await
}

/// Every turn's usage, added up.
pub(crate) fn usage<'a>(frames: impl IntoIterator<Item = &'a Frame>) -> Usage {
    sum(frames.into_iter().filter_map(|frame| match &frame.body {
        Body::Turn { usage, .. } => Some(usage),
        _ => None,
    }))
}

pub(crate) fn sum<'a>(usages: impl IntoIterator<Item = &'a Usage>) -> Usage {
    let mut total = Usage::default();
    for usage in usages {
        total.input += usage.input;
        total.output += usage.output;
        total.cache_read += usage.cache_read;
        total.cache_write += usage.cache_write;
    }
    total
}

#[derive(PartialEq)]
pub(crate) struct Detail {
    /// `None` once its manager dismissed it
    pub agent: Option<AgentRow>,
    pub audience: Audience,
    /// newest first, each with what its turns used and its model's window
    pub threads: Vec<(ThreadRow, Usage, Option<u64>)>,
    pub tools: Vec<ToolInfo>,
}

pub(crate) async fn detail(host: Host, agent: u64) -> Result<Detail, Error> {
    let row = host.query(harness::ask::Agent { agent }).await?;
    let audience = host
        .query(identity::ask::Audience { number: agent })
        .await?;
    let threads = host
        .query_all(|after| harness::ask::Threads {
            agent,
            page: page(after),
        })
        .await?;
    let mut used = Vec::new();
    // ponytail: the totals read every frame of every thread; a total kept on the thread row
    // when transcripts run long
    for thread in threads {
        let usage = usage(&frames(&host, thread.id).await?);
        let window = window(&host, &thread.model).await?;
        used.push((thread, usage, window));
    }
    let tools = host.query(harness::ask::Tools).await?;
    Ok(Detail {
        agent: row,
        audience,
        threads: used,
        tools,
    })
}

/// `model`'s context window, once harness learned one: `None` before it asked, and when the
/// model reported none.
async fn window(host: &Host, model: &str) -> Result<Option<u64>, Error> {
    let asked = harness::ask::Window {
        model: model.to_owned(),
    };
    Ok(host.query(asked).await?.flatten())
}

#[derive(PartialEq)]
pub(crate) struct Transcript {
    pub thread: ThreadRow,
    pub frames: Vec<Frame>,
    pub calls: Vec<CallRow>,
    /// the input waiting behind the loop
    pub held: Vec<Body>,
    /// the frames left out of the model's view, which it reads in parts
    pub withheld: Vec<WithheldFrame>,
    pub window: Option<u64>,
    /// member nodes that serve the service the thread opened with
    pub serving: usize,
}

pub(crate) async fn transcript(host: Host, id: u64) -> Result<Option<Transcript>, Error> {
    let Some(thread) = host.query(harness::ask::Thread { thread: id }).await? else {
        return Ok(None);
    };
    // ponytail: the whole transcript again on each block that writes harness; a paged tail
    // when threads run long
    let frames = frames(&host, id).await?;
    let calls = host
        .query_all(|after| harness::ask::Calls {
            thread: id,
            page: page(after),
        })
        .await?;
    let held = host.query(harness::ask::Held { thread: id }).await?;
    let withheld = host.query(harness::ask::Withheld { thread: id }).await?;
    let window = window(&host, &thread.model).await?;
    let serving = served(&host, &thread.service).await?;
    Ok(Some(Transcript {
        thread,
        frames,
        calls,
        held,
        withheld,
        window,
        serving,
    }))
}

/// A service the create form offers, with how many member nodes serve it now.
#[derive(PartialEq)]
pub(crate) struct Offer {
    pub row: ServiceRow,
    pub serving: usize,
}

pub(crate) async fn offers(host: Host) -> Result<Vec<Offer>, Error> {
    let rows = host
        .query_all(|after| egress::ask::Services { page: page(after) })
        .await?;
    let mut offers = Vec::new();
    for row in rows {
        let serving = served(&host, &row.name).await?;
        offers.push(Offer { row, serving });
    }
    Ok(offers)
}

/// The thread at `place`: the open one, else the last one there.
pub(crate) async fn thread_at(host: Host, agent: u64, place: String) -> Result<Option<u64>, Error> {
    let asked = harness::ask::ThreadAt { agent, place };
    Ok(host.query(asked).await?.map(|thread| thread.id))
}
