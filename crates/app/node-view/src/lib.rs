//! Nodes: the node this app talks to (its network, height, epoch, block
//! time, tip, identity and contract, re-read every second), the validator
//! set the `valset` program answers with, and every membership it holds —
//! the key, the address it is reached at, and whether it validates or only
//! resides.
//!
//! The rows are valset's own types (`queries.rs`), kept as they land and
//! worded only when drawn (`ui.rs`); valset's live heads re-read them.
use ducktape_view_guest::Loadable;
use ducktape_view_guest::methods::{ChainStatus, Changes, ClockTicks, NodeStatus};
use ducktape_view_guest::{Context, IntoElement, Render, Task, View, Window, export_view};
use serde::{Deserialize, Serialize};
use valset::view::ValsetApi;

mod queries;
mod ui;

pub use queries::Set;

#[derive(Serialize, Deserialize, Default)]
pub struct Nodes {
    /// the connected node's own status
    pub(crate) status: Loadable<NodeStatus>,
    pub(crate) set: Loadable<Set>,
    /// What the view follows; dropping them unsubscribes.
    #[serde(skip)]
    pub(crate) followers: Vec<Task<()>>,
}

/// How often the node status is re-read, in milliseconds.
const STATUS_TICK: i64 = 1_000;

impl View for Nodes {
    const MIN_WINDOW_WIDTH: u32 = 480;

    fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let mut view = Self::default();
        view.restored(window, cx);
        view
    }

    fn restored(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        let heads = cx.host().subscribe::<Changes<ValsetApi>>(());
        let ticks = cx.host().subscribe::<ClockTicks>(STATUS_TICK);
        self.followers = vec![
            cx.for_each(heads, |view, head, _, cx| match head {
                Ok(_) => view.read(cx),
                Err(refusal) => cx
                    .host()
                    .log_refused("nodes", "valset's live heads", &refusal),
            }),
            cx.for_each(ticks, |view, tick, _, cx| match tick {
                Ok(()) => view.read_status(cx),
                Err(refusal) => cx.host().log_refused("nodes", "the clock", &refusal),
            }),
        ];
        self.read(cx);
        self.read_status(cx);
    }
}

impl Render for Nodes {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        ui::render(self, cx)
    }
}

impl Nodes {
    /// One read of the set — the boot, a retry, a restore, a live head.
    /// What is already on screen stays there while it runs.
    pub(crate) fn read(&mut self, cx: &mut Context<Self>) {
        let work = queries::set(cx.host());
        match self.set.ready() {
            Some(_) => cx.refresh(work, |view, set, _| view.set = Loadable::Ready(set)),
            None => self.set = cx.load(work, |view| &mut view.set),
        }
        cx.notify();
    }

    /// The node status: read, or re-read with what is on screen kept.
    pub(crate) fn read_status(&mut self, cx: &mut Context<Self>) {
        let work = cx.host().ask::<ChainStatus>(());
        if self.status.ready().is_some() {
            cx.refresh(work, |view, status, _| {
                view.status = Loadable::Ready(status)
            });
        } else if !self.status.is_loading() {
            self.status = cx.load(work, |view| &mut view.status);
        }
        cx.notify();
    }
}

export_view!(
    Nodes,
    "Nodes",
    "The node this app talks to, the validator set of this network and every membership behind it.",
    [Chain, Module, Host, Clock]
);

#[cfg(test)]
mod tests;
