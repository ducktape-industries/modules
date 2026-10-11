//! The web piece: a page the host's browser engine draws ([`wire::Node::Web`]),
//! for a view that declares [`Capability::Web`](crate::methods::Capability::Web).
use crate::element::wire_id;
use crate::interactivity::EventListener;
use crate::{App, Element, ElementId, IntoElement, Lowering, Window, wire};
use gpui::{StyleRefinement, Styled};

/// What a [`web`] page tells its view: its address now, its title,
/// whether it loads, whether it can go back or forward.
pub use wire::WebState;

/// The navigations a view asks of a [`web`] page, kept until the frame
/// that carries them: give it to the piece with [`Web::track`], call
/// `load`, `back`, `forward`, `reload` or `stop`, and notify. Each
/// reaches the page once.
#[derive(Clone, Default)]
pub struct WebHandle(std::rc::Rc<std::cell::RefCell<WebQueue>>);

#[derive(Default)]
struct WebQueue {
    /// Moves with every command: the host applies the commands of a
    /// revision it has not seen.
    revision: u64,
    commands: Vec<wire::WebCommand>,
}

impl WebHandle {
    pub fn new() -> Self {
        Self::default()
    }

    fn push(&self, command: wire::WebCommand) {
        let mut queue = self.0.borrow_mut();
        queue.revision = queue.revision.wrapping_add(1);
        if queue.commands.len() < wire::MAX_WEB_COMMANDS {
            queue.commands.push(command);
        }
    }

    /// Opens `url`. The host loads only an `https://` page, or an
    /// `http://` one on this machine (`localhost`, a loopback address),
    /// with no user name or password in it.
    pub fn load(&self, url: impl Into<String>) {
        self.push(wire::WebCommand::Load(url.into()));
    }
    pub fn back(&self) {
        self.push(wire::WebCommand::Back);
    }
    pub fn forward(&self) {
        self.push(wire::WebCommand::Forward);
    }
    pub fn reload(&self) {
        self.push(wire::WebCommand::Reload);
    }
    pub fn stop(&self) {
        self.push(wire::WebCommand::Stop);
    }
}

/// A web page the host draws in this element's box, opened on `url` the
/// first time the host sees the element under `id`, and closed when a frame
/// no longer has it. The page is the reader's to press, type and scroll
/// in, so the element takes no listeners but [`Web::on_state`]. Only a
/// view whose manifest declares [`Capability::Web`](crate::methods::Capability::Web)
/// may draw one: the host refuses any other's frame.
///
/// ```
/// # use ducktape_view_guest::prelude::*;
/// let nav = WebHandle::new();
/// let page = web("page", "https://example.com")
///     .track(&nav)
///     .on_state(|state: &WebState, _, _| println!("{}", state.url))
///     .size_full();
/// nav.back();
/// ```
pub struct Web {
    id: ElementId,
    url: String,
    handle: Option<WebHandle>,
    on_state: Option<EventListener<WebState>>,
    style: Box<StyleRefinement>,
}

pub fn web(id: impl Into<ElementId>, url: impl Into<String>) -> Web {
    Web {
        id: id.into(),
        url: url.into(),
        handle: None,
        on_state: None,
        style: Box::default(),
    }
}

impl Web {
    /// The page takes its navigations from `handle`.
    pub fn track(mut self, handle: &WebHandle) -> Self {
        self.handle = Some(handle.clone());
        self
    }

    /// Hears where the page is: when it first reports, and whenever its
    /// address, title, loading or back and forward change.
    pub fn on_state(
        mut self,
        listener: impl Fn(&WebState, &mut Window, &mut App) + 'static,
    ) -> Self {
        self.on_state = Some(Box::new(listener));
        self
    }
}

impl IntoElement for Web {
    type Element = Self;
    fn into_element(self) -> Self {
        self
    }
}

impl Element for Web {
    fn id(&self) -> Option<ElementId> {
        Some(self.id.clone())
    }

    fn lower(self: Box<Self>, lowering: &mut Lowering<'_>) -> wire::Node {
        let (revision, commands) = self.handle.map_or((0, Vec::new()), |handle| {
            let mut queue = handle.0.borrow_mut();
            (queue.revision, std::mem::take(&mut queue.commands))
        });
        wire::Node::Web {
            id: wire_id(self.id),
            url: self.url,
            style: lowering.style(&self.style),
            revision,
            commands,
            on_state: self
                .on_state
                .map(|listener| lowering.route(crate::slots::Kind::WebState, listener)),
        }
    }
}

impl Styled for Web {
    fn style(&mut self) -> &mut StyleRefinement {
        &mut self.style
    }
}

impl gpui::prelude::FluentBuilder for Web {}
