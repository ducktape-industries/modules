//! Account: who the seated key is (its account, its keys, the agents it
//! manages) and invites to the network, one pane each behind a left menu.
//! The program is still `settings` (module-registry's view); only its name
//! on screen is Account. The node's own status is the Nodes view's, and
//! the app's preferences the host's gear.
//!
//! The state is one struct (`state.rs`). Reads are typed asks of identity
//! and valset (`queries.rs`), re-read on their live heads (`watch.rs`);
//! presses land in `actions.rs`; `ui/` draws and never mutates.
mod actions;
mod api;
mod queries;
mod state;
mod ui;
mod watch;

pub use state::Settings;

use ducktape_view_guest::methods::Capability;
use ducktape_view_guest::{Context, IntoElement, Render, View, Window, export_view};

impl View for Settings {
    const NAME: &'static str = "Account";
    const DESCRIPTION: &'static str = "Your account, its keys, the agents it manages, and invites.";
    const CAPABILITIES: &'static [Capability] = &[
        Capability::Module,
        Capability::Op,
        Capability::Invite,
        Capability::Host,
        Capability::Clipboard,
    ];
    const TARGETS: &'static [&'static str] = &[identity::MODULE, valset::MODULE];
    const MIN_WINDOW_WIDTH: u32 = 560;

    fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let mut view = Self {
            // an invite lasts a week unless the reader picks otherwise
            ttl: 1,
            ..Self::default()
        };
        view.restored(window, cx);
        view
    }

    fn restored(&mut self, _: &mut Window, cx: &mut Context<Self>) {
        self.watch(cx);
    }
}

impl Render for Settings {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        ui::render(self, cx)
    }
}

export_view!(Settings);

#[cfg(test)]
mod tests;
