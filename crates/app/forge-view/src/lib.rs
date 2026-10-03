//! Forge: repositories, code, commits, refs and the Changes a reviewer
//! lives in, on the ducktape-view-guest `View` shape.
//!
//! The forge program answers everything this screen shows, in borsh, through
//! one method (`module.query`); conversation is chat's, through the method
//! chat-view uses. Reads are a cache keyed by the query itself: `sync` asks
//! what the current screen needs, issues what is missing, and drops what the
//! reader has navigated away from. `render` never mutates — what an event
//! changes lands in `actions`.
//!
//! - `state`: what the view holds; `select`: what the screens read out of it.
//! - `sync`: which reads the screen needs; `queries`: how one read is asked.
//! - `navigate`, `actions`, `review`, `tree`: what an event changes.
//! - `ui`: the screens; `ui::markdown`: documents and their links.
mod api;
mod queries;
mod select;
mod state;
mod sync;
mod tree;

mod actions;
mod navigate;
mod review;
mod ui;

use ducktape_view_guest::methods::Capability;
use ducktape_view_guest::methods::{Changes, HostRoute, HostVisible};
use ducktape_view_guest::{Context, IntoElement, Render, View, Window, export_view};

use api::HostSession;
use program::role::Identity;
pub(crate) use select::Stage;
pub use state::Forge;

impl View for Forge {
    const NAME: &'static str = "Forge";
    const DESCRIPTION: &'static str =
        "Repositories, code, commits and the changes waiting on your judgment.";
    const CAPABILITIES: &'static [Capability] = &[
        Capability::Module,
        Capability::Op,
        Capability::Host,
        Capability::Link,
        Capability::Clipboard,
    ];
    const TARGETS: &'static [&'static str] = &[
        forge::MODULE,
        chat::MODULE,
        <Identity as ducktape_view_guest::methods::Program>::NAME,
    ];
    const MIN_WINDOW_WIDTH: u32 = 640;

    /// Every stream this view follows, on a first mount and after a
    /// snapshot. A refused item says so in the notice.
    fn attach(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        self.watches.clear();
        let props = cx.host().subscribe::<HostSession>(());
        self.watches
            .push(cx.for_each(props, |forge, item, _, cx| match item {
                Ok(session) => forge.session_changed(session, cx),
                Err(refusal) => {
                    forge.notice = format!("Couldn’t read the session: {}", refusal.message);
                    cx.notify();
                }
            }));
        // `duck://<chain>/forge/<name>`: a link opened into this view names
        // the repository to open
        let routes = cx.host().subscribe::<HostRoute>(());
        self.watches
            .push(cx.for_each(routes, |forge, route, _, cx| match route {
                Ok(route) => forge.open_route(&route, cx),
                Err(refusal) => {
                    forge.notice = format!("Couldn’t follow the link: {}", refusal.message);
                    cx.notify();
                }
            }));
        // a block re-reads what it wrote to: forge's its reads on screen,
        // chat's the change conversations and the reads answered from chat
        // (a judgment), identity's the names. A refused item is a block this
        // view cannot see into: the host's log keeps why, and the next block
        // reconciles
        let forge = cx.host().subscribe::<Changes<forge::Forge>>(());
        let chat = cx.host().subscribe::<Changes<::chat::Chat>>(());
        let identity = cx.host().subscribe::<Changes<Identity>>(());
        self.watches.extend([
            cx.for_each(forge, |forge, change, _, cx| match change {
                Ok(change) => forge.reconcile(change.as_ref(), cx),
                Err(refusal) => cx
                    .host()
                    .log_refused("forge", "forge's live heads", &refusal),
            }),
            cx.for_each(chat, |forge, change, _, cx| match change {
                Ok(change) => forge.chat_changed(change.as_ref(), cx),
                Err(refusal) => cx
                    .host()
                    .log_refused("forge", "chat's live heads", &refusal),
            }),
            cx.for_each(identity, |forge, change, _, cx| match change {
                Ok(_) => forge.reread_names(cx),
                Err(refusal) => cx
                    .host()
                    .log_refused("forge", "identity's live heads", &refusal),
            }),
        ]);
        let visible = cx.host().subscribe::<HostVisible>(());
        self.watches
            .push(cx.for_each(visible, |forge, shown, _, cx| match shown {
                Ok(true) => forge.refresh(cx),
                Ok(false) => {}
                Err(refusal) => cx.host().log_refused("forge", "visibility", &refusal),
            }));
        if self.names.is_idle() {
            self.names = cx.load(queries::roster(cx.host()), |forge| &mut forge.names);
        }
        self.sync(cx);
    }
}

impl Render for Forge {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        // the pane's width, as the host lays the frame out: the rail and
        // the dock fold to it in the frame that shows them
        self.layout.width = window.viewport_size().width.into();
        ui::render(self, cx)
    }
}

export_view!(Forge);

#[cfg(test)]
mod tests;
