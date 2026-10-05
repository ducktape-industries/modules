//! Chat: channels, direct messages, threads and search.
//!
//! The view's state is one struct per pane (`state.rs`). Rows come from the
//! `chat` program through its own types (`queries.rs`), are folded with the
//! name directory into what a frame draws (`message.rs`, `names.rs`) at
//! render time, and `render` (`ui/`) never mutates: what an event changes
//! lands in the handlers of `room`, `actions`, `search` and `compose`. The
//! rich editor is the composer (`composer.rs`, `composer/`), over the SDK's
//! text field.
mod actions;
mod compose;
mod composer;
mod emoji;
mod kept;
mod links;
mod message;
mod names;
mod notices;
mod queries;
mod room;
mod search;
mod session;
mod state;
mod ui;
mod watch;

pub use state::*;

use ducktape_view_guest::prelude::*;

/// Rows asked per page.
const PAGE: usize = 64;
/// Rows a room keeps on screen before paging older.
const WINDOW: usize = 256;

impl View for Chat {
    const NAME: &'static str = "Chat";
    const DESCRIPTION: &'static str =
        "Channels, direct messages, threads, search and the live call of this workspace.";
    const CAPABILITIES: &'static [Capability] = &[
        Capability::Module,
        Capability::Op,
        Capability::Host,
        Capability::Link,
        Capability::Clipboard,
        Capability::Notify,
        Capability::Store,
    ];
    const TARGETS: &'static [&'static str] = &[
        chat::MODULE,
        <program::role::Identity as ducktape_view_guest::methods::Program>::NAME,
    ];
    const MIN_WINDOW_WIDTH: u32 = 560;

    /// Follows the host and reads what the screen shows: on a first mount,
    /// and again after a snapshot, whose in-flight work it parks.
    fn attach(&mut self, _: &mut Window, cx: &mut Context<Self>) {
        // a send in flight when the snapshot was taken never came back:
        // park its body as a failed send the composer can restore
        for draft in self.drafts.values_mut() {
            draft.retire_device_requests();
        }
        self.menu = None;
        self.watch(cx);
        if self.names.is_idle() {
            self.load_names(cx);
        }
        if self.channels.is_idle() {
            self.load_channels(cx);
        }
        if let Some(room) = &self.room {
            let (id, thread) = (room.id.clone(), room.thread.as_ref().map(|t| t.root));
            self.open(id, cx);
            if let Some(root) = thread {
                self.open_thread(root, cx);
            }
        }
        if !self.search.query.is_empty() {
            self.search_now(cx);
        }
    }
}

impl Render for Chat {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        // the pane's size, as the host lays the frame out: the panes
        // clamp to it before the frame that shows them
        let size = window.viewport_size();
        self.layout.viewport = (size.width.into(), size.height.into());
        self.layout.clamp();
        self.seat_drafts();
        ui::render(self, cx)
    }
}

export_view!(Chat);

#[cfg(test)]
mod tests;
