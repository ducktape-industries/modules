//! Explorer: the chain as this node lists it. Finalized blocks and the
//! transactions they carry come from `chain.blocks` and `chain.block`, a
//! transaction's whole payload from `chain.tx`; who signed them from
//! `identity`; the validators from `valset`; the programs from
//! `programs`; what each account did from `activity`, whose frames
//! are read by their place (`chain.tx`).
//!
//! What the node does not keep is not shown: there are no receipts, so a
//! transaction is "in block N", never applied or rejected; and there is no
//! per-block state root or write set. The block and transaction lists are
//! the last [`WINDOW`] blocks, read a page at a time and then followed at
//! the head as `chain.heads` pushes it.
//!
//! The state is one struct (`state.rs`) and the window (`chain.rs`). Reads
//! are typed (`queries.rs`) and loaded in `load.rs`, re-read on the live
//! heads `watch.rs` follows; where the reader goes lands in `actions.rs`;
//! what the reader changes on the Programs page, by grant or by vote, in
//! `acting.rs`; `ui/` draws.
mod acting;
mod actions;
mod chain;
mod decode;
mod load;
mod queries;
mod state;
pub(crate) mod ui;
mod watch;

pub use chain::{BlockRow, Chain, TxRow};
pub use state::{Accounts, Explorer, Network, Note, Route};

use view::prelude::*;

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
        ui::render(self, cx)
    }
}

export_view!(Explorer);

#[cfg(test)]
mod tests;
