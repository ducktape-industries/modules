//! Nodes: one sheet for the network this app talks to. The connected node's
//! own numbers head it (height, last block, block time, the epoch), then
//! every member once, validators then residents: its key and address, and
//! for a validator the blocks it led of the last 64 and how far its newest
//! finalize vote is from this node's tip as `chain.network` reports it (one
//! status word: In sync, N behind, Quiet). A resident's sync state is not
//! reported: its row is its key and address. A node that is not voting (it
//! reports no vote of its own) hears none, so then no validator's is
//! reported either. A node that does not serve `chain.network` says so in
//! one line, and the validators' cells stay empty.
//!
//! The members are valset's (`queries.rs`), re-read on its live heads; the
//! status, the network and the strip's blocks (`recent.rs`) follow the
//! clock, one ask of each in flight at a time.
use ducktape_view_guest::host::Error;
use ducktape_view_guest::methods::{
    ChainBlocks, ChainNetwork, ChainStatus, Changes, ClockTicks, NetworkStatus, NodeStatus,
};
use ducktape_view_guest::{
    Context, IntoElement, Loadable, Render, Task, View, Window, export_view,
};
use serde::{Deserialize, Serialize};
use valset::view::Valset;

mod queries;
mod recent;
mod row;
mod table;
mod ui;

use queries::Node;
use recent::Recent;

#[derive(Serialize, Deserialize, Default)]
pub struct Nodes {
    /// the connected node's own status
    pub(crate) status: Loadable<NodeStatus>,
    pub(crate) nodes: Loadable<Vec<Node>>,
    /// each validator's newest finalize vote as the connected node heard
    /// it; `Failed` only where it does not serve `chain.network`
    pub(crate) network: Loadable<NetworkStatus>,
    pub(crate) recent: Recent,
    /// clock ticks since the view opened, and the tick the node last
    /// answered its status on and the one its height last moved on
    #[serde(skip)]
    pub(crate) ticks: u64,
    #[serde(skip)]
    pub(crate) answered: u64,
    #[serde(skip)]
    pub(crate) moved: u64,
    /// the sheet's measured width; `None` until the first measure
    #[serde(skip)]
    pub(crate) width: Option<f32>,
    /// the asks in flight: the clock asks again only once one lands
    #[serde(skip)]
    pub(crate) asking: Asking,
    /// What the view follows; dropping them unsubscribes.
    #[serde(skip)]
    pub(crate) followers: Vec<Task<()>>,
}

#[derive(Default)]
pub(crate) struct Asking {
    status: bool,
    network: bool,
    blocks: bool,
}

/// How often the node is asked, in milliseconds.
const TICK: i64 = 1_000;
/// Ticks without a status answer before the node reads Not answering.
pub(crate) const SILENT_TICKS: u64 = 3;
/// The refusal of a node that does not serve `chain.network`.
const UNSUPPORTED: &str = "unknown_request";

impl View for Nodes {
    const PREFERRED_WINDOW_SIZE: &'static str = "1100,680";

    fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let mut view = Self::default();
        view.restored(window, cx);
        view
    }

    fn restored(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        let heads = cx.host().subscribe::<Changes<Valset>>(());
        let ticks = cx.host().subscribe::<ClockTicks>(TICK);
        self.followers = vec![
            cx.for_each(heads, |view, head, _, cx| match head {
                Ok(_) => view.read(cx),
                Err(refusal) => cx
                    .host()
                    .log_refused("nodes", "valset's live heads", &refusal),
            }),
            cx.for_each(ticks, |view, tick, _, cx| match tick {
                Ok(()) => {
                    view.ticks += 1;
                    view.read_status(cx);
                    view.read_network(cx);
                }
                Err(refusal) => cx.host().log_refused("nodes", "the clock", &refusal),
            }),
        ];
        self.read(cx);
        self.read_status(cx);
        self.read_network(cx);
    }
}

impl Render for Nodes {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        ui::render(self, cx)
    }
}

impl Nodes {
    /// One read of the members — the boot, a retry, a restore, a live
    /// head. What is already on screen stays there while it runs.
    pub(crate) fn read(&mut self, cx: &mut Context<Self>) {
        let work = queries::nodes(cx.host());
        match self.nodes.ready() {
            Some(_) => cx.refresh(work, |view, nodes, _| view.nodes = Loadable::Ready(nodes)),
            None => self.nodes = cx.load(work, |view| &mut view.nodes),
        }
        cx.notify();
    }

    /// The node's status. A refused re-read keeps the numbers on screen;
    /// the header says Not answering once [`SILENT_TICKS`] pass without one.
    pub(crate) fn read_status(&mut self, cx: &mut Context<Self>) {
        if std::mem::replace(&mut self.asking.status, true) {
            return;
        }
        let ask = cx.host().ask::<ChainStatus>(());
        cx.spawn(async move |this, cx| {
            let answer = ask.await;
            let _ = this.update(cx, |view, cx| view.status_landed(answer, cx));
        })
        .detach();
    }

    fn status_landed(&mut self, answer: Result<NodeStatus, Error>, cx: &mut Context<Self>) {
        self.asking.status = false;
        match answer {
            Ok(status) => {
                if self.status.ready().map(|now| now.height) != Some(status.height) {
                    self.moved = self.ticks;
                }
                self.answered = self.ticks;
                self.status = Loadable::Ready(status);
                self.pull(cx);
            }
            Err(refusal) if self.status.ready().is_none() => {
                self.status = Loadable::Failed(refusal)
            }
            Err(refusal) => cx
                .host()
                .log_refused("nodes", "the node's status", &refusal),
        }
        cx.notify();
    }

    /// Each validator's newest finalize vote as the node heard it. A node
    /// that does not serve it (`unknown_request`) is logged once and the
    /// sheet says so until an answer; any other refusal (a node restarting,
    /// a dropped link) is logged and keeps the last answer, as the status
    /// does.
    pub(crate) fn read_network(&mut self, cx: &mut Context<Self>) {
        if std::mem::replace(&mut self.asking.network, true) {
            return;
        }
        let ask = cx.host().ask::<ChainNetwork>(());
        cx.spawn(async move |this, cx| {
            let answer = ask.await;
            let _ = this.update(cx, |view, cx| {
                view.asking.network = false;
                match answer {
                    Ok(network) => view.network = Loadable::Ready(network),
                    Err(refusal) if refusal.code == UNSUPPORTED => {
                        if view.network.failed().is_none() {
                            cx.host()
                                .log_refused("nodes", "the validators' votes", &refusal);
                        }
                        view.network = Loadable::Failed(refusal);
                    }
                    Err(refusal) => {
                        cx.host()
                            .log_refused("nodes", "the validators' votes", &refusal)
                    }
                }
                cx.notify();
            });
        })
        .detach();
    }

    /// Reads the next page the strip wants at the node's height, if any,
    /// and on until it wants none; a refusal waits for the next status.
    fn pull(&mut self, cx: &mut Context<Self>) {
        let Some(head) = self.status.ready().map(|status| status.height) else {
            return;
        };
        if self.asking.blocks {
            return;
        }
        let Some(page) = self.recent.next(head) else {
            return;
        };
        self.asking.blocks = true;
        let ask = cx.host().ask::<ChainBlocks>(page.clone());
        cx.spawn(async move |this, cx| {
            let answer = ask.await;
            let _ = this.update(cx, |view, cx| {
                view.asking.blocks = false;
                match answer {
                    Ok(blocks) => {
                        view.recent.land(&page, head, blocks);
                        // a page that filled nothing is not asked again
                        // until the next status
                        let wanted = page.before.map_or(head, |before| before - 1);
                        if view.recent.led.contains_key(&wanted) {
                            view.pull(cx);
                        }
                    }
                    Err(refusal) => cx
                        .host()
                        .log_refused("nodes", "the recent blocks", &refusal),
                }
                cx.notify();
            });
        })
        .detach();
    }

    /// Whether the node answered its status within [`SILENT_TICKS`].
    pub(crate) fn answering(&self) -> bool {
        self.status.ready().is_some() && self.ticks - self.answered <= SILENT_TICKS
    }
}

export_view!(
    Nodes,
    "Nodes",
    "The network this app talks to: its height and epoch, and every member, with how far each validator's signature is from the tip.",
    [Chain, Module, Host, Clock]
);

#[cfg(test)]
mod tests;
