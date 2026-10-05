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
use ducktape_view_guest::methods::Capability;
use ducktape_view_guest::methods::{
    self, ChainBlocks, ChainNetwork, ChainStatus, Changes, ClockTicks, NetworkStatus, NodeStatus,
};
use ducktape_view_guest::{Context, IntoElement, Loadable, Render, View, Window, export_view};
use serde::{Deserialize, Serialize};
use valset::Valset;

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
    /// the pane's width, read from the window each render
    #[serde(skip)]
    pub(crate) width: f32,
    /// the asks in flight: the clock asks again only once one lands
    #[serde(skip)]
    pub(crate) asking: Asking,
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

impl View for Nodes {
    const NAME: &'static str = "Nodes";
    const DESCRIPTION: &'static str = "The network this app talks to: its height and epoch, and every member, with how far each validator's signature is from the tip.";
    const CAPABILITIES: &'static [Capability] = &[
        Capability::Chain,
        Capability::Module,
        Capability::Host,
        Capability::Clock,
    ];
    const TARGETS: &'static [&'static str] = &[valset::MODULE];
    const MIN_WINDOW_WIDTH: u32 = 480;

    fn attach(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        cx.follow::<Changes<Valset>>((), |view, head, cx| match head {
            Ok(_) => view.read(cx),
            Err(refusal) => cx.log_refused("valset's live heads", &refusal),
        })
        .detach();
        // the header's age is drawn from the ticks
        cx.follow::<ClockTicks>(TICK, |view, tick, cx| match tick {
            Ok(()) => {
                view.ticks += 1;
                cx.notify();
                view.read_status(cx);
                view.read_network(cx);
            }
            Err(refusal) => cx.log_refused("the clock", &refusal),
        })
        .detach();
        self.read(cx);
        self.read_status(cx);
        self.read_network(cx);
    }
}

impl Render for Nodes {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        // the pane's width, as the host lays the frame out: the table fits
        // to it in the frame that shows it
        self.width = window.viewport_size().width.into();
        ui::render(self, cx)
    }
}

impl Nodes {
    /// One read of the members — the boot, a retry, a restore, a live
    /// head. What is already on screen stays there while it runs.
    pub(crate) fn read(&mut self, cx: &mut Context<Self>) {
        let work = queries::nodes(cx.host());
        cx.reload(&mut self.nodes, work, |view| &mut view.nodes);
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
            Err(refusal) => cx.log_refused("the node's status", &refusal),
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
                    // a node that does not serve `chain.network`
                    Err(refusal) if refusal.code == methods::refusal::UNKNOWN_REQUEST => {
                        if view.network.failed().is_none() {
                            cx.log_refused("the validators' votes", &refusal);
                        }
                        view.network = Loadable::Failed(refusal);
                    }
                    Err(refusal) => cx.log_refused("the validators' votes", &refusal),
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
                    Err(refusal) => cx.log_refused("the recent blocks", &refusal),
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

export_view!(Nodes);

#[cfg(test)]
mod tests;
