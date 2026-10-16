//! Forge: repositories, code, commits, refs and the Changes a reviewer
//! lives in, on the view `View` shape.
//!
//! The forge program answers everything this screen shows, in borsh, through
//! one method (`program.query`); conversation is chat's, through the method
//! chat-view uses. Reads are a cache keyed by the query itself: `sync` asks
//! what the current screen needs, issues what is missing, and drops what the
//! reader has navigated away from. `render` writes only what the frame
//! learns, the pane's width and the window's title last sent — what an
//! event changes lands in `actions`.
//!
//! - `state`: what the view holds; `select`: what the screens read out of it.
//! - `sync`: which reads the screen needs; `queries`: how one read is asked.
//! - `navigate`, `actions`, `review`, `tree`: what an event changes.
//! - `words`: every sentence both designs say; `markdown`, `highlight`,
//!   `blob`: documents, source tokens and the open file, read once for
//!   either design; `list`: a list as one Tab stop.
//! - `ui`: the desktop's screens; `ui::phone`: the phone's (the Expo app's).
mod api;
mod blob;
mod highlight;
mod list;
mod markdown;
mod queries;
mod select;
mod state;
mod sync;
mod tree;
mod words;

mod actions;
mod navigate;
mod review;
mod ui;

use view::prelude::*;

use identity::Identity;
pub(crate) use select::Stage;
pub use state::Forge;

impl View for Forge {
    const NAME: &'static str = "Forge";
    const DESCRIPTION: &'static str =
        "Repositories, code, commits and the changes waiting on your judgment.";
    const ICON: &'static str = "icons/hammer.svg";
    const CAPABILITIES: &'static [Capability] = &[
        Capability::Program,
        Capability::Op,
        Capability::Host,
        Capability::Link,
        Capability::Clipboard,
    ];
    const TARGETS: &'static [&'static str] = &[forge::PROGRAM, chat::PROGRAM, identity::PROGRAM];
    // at 560 the repository tab bar's About toggle is past the right edge
    const MIN_WINDOW_WIDTH: u32 = 640;

    /// Every stream this view follows, on a first mount and after a
    /// snapshot. A refused item says so in the notice.
    fn attach(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        cx.follow::<HostSession>((), |forge, item, cx| match item {
            Ok(session) => forge.session_changed(session, cx),
            Err(refusal) => {
                forge.notice = format!("Couldn’t read the session: {}", refusal.message);
                cx.notify();
            }
        })
        .detach();
        // `duck://<chain>/forge/<name>`: a link opened into this view names
        // the repository to open
        cx.follow::<HostRoute>((), |forge, route, cx| match route {
            Ok(route) => forge.open_route(&route, cx),
            Err(refusal) => {
                forge.notice = format!("Couldn’t follow the link: {}", refusal.message);
                cx.notify();
            }
        })
        .detach();
        // a block re-reads what it wrote to: forge's its reads on screen,
        // chat's the change conversations and the reads answered from chat
        // (a judgment), identity's the names. A refused item is a block this
        // view cannot see into: the host's log keeps why, and the next block
        // reconciles
        cx.follow::<Changes<forge::Forge>>((), |forge, change, cx| match change {
            Ok(change) => forge.reconcile(change.as_ref(), cx),
            Err(refusal) => cx.log_refused("forge's live heads", &refusal),
        })
        .detach();
        cx.follow::<Changes<::chat::Chat>>((), |forge, change, cx| match change {
            Ok(change) => forge.chat_changed(change.as_ref(), cx),
            Err(refusal) => cx.log_refused("chat's live heads", &refusal),
        })
        .detach();
        cx.follow::<Changes<Identity>>((), |forge, change, cx| match change {
            Ok(_) => forge.reread_names(cx),
            Err(refusal) => cx.log_refused("identity's live heads", &refusal),
        })
        .detach();
        cx.follow::<HostVisible>((), |forge, shown, cx| match shown {
            Ok(true) => forge.refresh(cx),
            Ok(false) => {}
            Err(refusal) => cx.log_refused("visibility", &refusal),
        })
        .detach();
        if self.names.is_idle() {
            cx.load(self, queries::roster(cx.host()), |forge| &mut forge.names);
        }
        self.sync(cx);
    }
}

impl Render for Forge {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        // the pane's size, as the host lays the frame out: the rail and
        // the dock fold to its width in the frame that shows them, and a
        // dropdown keeps to its height
        let viewport = window.viewport_size();
        self.layout.width = viewport.width.into();
        self.layout.height = viewport.height.into();
        // a change's title lands async: the frame is the one place that
        // sees each
        self.settle_title(cx);
        // the one pick between the two designs: under a finger the Expo
        // app's screens (`ui::phone`), under a pointer the desktop's; the
        // phone's screens read a little more (`Forge::touch`)
        if self.touch != window.touch() {
            self.touch = window.touch();
            self.sync(cx);
        }
        if self.touch {
            ui::phone::render(self, cx)
        } else {
            ui::render(self, cx).into_any_element()
        }
    }
}

/// An element id from its name: every id in this view is a name, so a test
/// or the host finds it by what it says.
pub(crate) fn id(text: impl Into<String>) -> ElementId {
    ElementId::Name(text.into().into())
}

impl Forge {
    /// The window's title: `Repositories` over the list, a repository's
    /// name, a change as `<repo> · #<n>` and its title once it lands. Sent
    /// when it changes.
    fn settle_title(&mut self, cx: &mut Context<Self>) {
        let title = match (&self.nav.repo, self.nav.change) {
            (None, _) => "Repositories".to_owned(),
            (Some(repo), None) => repo.clone(),
            (Some(repo), Some(n)) => match self.change() {
                Some((change, ..)) => format!("{repo} · #{n} {}", change.title),
                None => format!("{repo} · #{n}"),
            },
        };
        if self.title.as_deref() != Some(title.as_str()) {
            cx.host().notify::<HostTitle>(title.clone());
            self.title = Some(title);
        }
    }
}

export_view!(Forge);

#[cfg(test)]
mod tests;
