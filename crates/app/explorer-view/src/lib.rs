//! Explorer: the chain as this node lists it. Finalized blocks and the
//! transactions they carry come from `chain.blocks` and `chain.block`, a
//! transaction's whole payload from `chain.tx`; who signed them from
//! `identity`; the validators from `valset`; the programs from
//! `programs`; the chain's latest transactions, a program's and an
//! account's from `activity`, which lists every frame newest first.
//!
//! What the node does not keep is not shown: there is no per-block state
//! root or write set. The block list is the newest `chain.blocks` page,
//! followed at the head as `chain.heads` pushes it and read further down
//! as the Blocks tab is scrolled. The transaction lists are the index's,
//! read a page at a time as they are scrolled (`recent.rs`); a row's block
//! is read when the row is drawn, its whole payload when it is opened.
//!
//! The state is one struct (`state.rs`) and the blocks held (`chain.rs`).
//! Reads are typed (`queries.rs`) and loaded in `load.rs`, re-read on the
//! live heads `watch.rs` follows; where the reader goes lands in
//! `actions.rs`; what the reader changes on the Programs page, by grant or
//! by vote, in `acting.rs`; `ui/` draws.
mod acting;
mod actions;
mod chain;
mod decode;
mod load;
mod queries;
mod recent;
mod state;
pub(crate) mod ui;
mod watch;

pub use chain::{BlockRow, Chain, TxRow};
pub use recent::IndexRow;
pub use state::{Accounts, Devices, Explorer, Network, Note, Route};

use view::prelude::*;

/// Rows the index is asked for at a time: a page more each time a list is
/// scrolled to the end of what it holds.
const PAGE: u64 = 64;
/// The Blocks tab's reach: the newest `chain.blocks` page at boot, the
/// heads since, and older pages as the tab is drawn down to this many
/// blocks, which is also what the view keeps once the tab is left. Older
/// blocks open by height; the transaction lists are the index's and reach
/// every frame; a transaction's block is read when its row is drawn.
pub const WINDOW: usize = 1_000;

impl View for Explorer {
    const NAME: &'static str = "Explorer";
    const DESCRIPTION: &'static str =
        "The chain as this node keeps it: blocks, transactions, accounts and programs.";
    const ICON: &'static str = "icons/compass.svg";
    const CAPABILITIES: &'static [Capability] = &[
        Capability::Chain,
        Capability::Program,
        // the programs role changed by grant, or by a motion to valset
        Capability::Op,
        Capability::Host,
        Capability::Clock,
        Capability::Clipboard,
    ];
    const TARGETS: &'static [&'static str] = &[
        identity::PROGRAM,
        valset::PROGRAM,
        programs::PROGRAM,
        activity::PROGRAM,
    ];
    // at 560 a block's hash is cut at the right edge
    const MIN_WINDOW_WIDTH: u32 = 640;

    fn attach(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        self.watch(cx);
        self.read_all(cx);
    }
}

impl Render for Explorer {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        // the pane's width, as the host lays the frame out: the Overview
        // lays its panels out to it in the frame that shows them
        self.width = window.viewport_size().width.into();
        // the rows the Overview draws whole are known ahead of the frame:
        // the blocks they sit in are asked for here, not while drawing
        self.locate_latest(cx);
        ui::render(self, cx)
    }
}

export_view!(Explorer);

#[cfg(test)]
mod tests;
