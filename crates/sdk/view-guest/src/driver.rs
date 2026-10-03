use crate::context::Callback;
use crate::{
    App, Context, Entity, Host, IntoElement, Lowering, Theme, View, executor, px, slots, wire,
};

const MAX_ROUNDS: usize = 8;

/// One view's run loop: the wasm exports drive it on the host, and
/// [`TestAppContext`](crate::testing::TestAppContext) drives it in a test.
pub(crate) struct Driver<V: View> {
    pub(crate) app: App,
    pub(crate) entity: Entity<V>,
    pub(crate) last_root: Option<wire::Node>,
    pub(crate) busy: bool,
    /// How many times the root rendered: what `TestAppContext::renders`
    /// reads.
    pub(crate) renders: u64,
}
impl<V: View> Drop for Driver<V> {
    fn drop(&mut self) {
        self.app.inner.alive.set(false);
        self.app.inner.tasks.borrow_mut().clear();
    }
}
impl<V: View> Default for Driver<V> {
    fn default() -> Self {
        Self::new()
    }
}
impl<V: View> Driver<V> {
    pub fn new() -> Self {
        Self::initialize_in(App::for_driver(), None)
    }
    pub(crate) fn initialize_in(mut app: App, restored: Option<V>) -> Self {
        let entity = Entity::reserve(&app);
        let mut window = app.window();
        let mut cx = Context {
            app: &mut app,
            entity: entity.clone(),
        };
        let mut value = match restored {
            Some(value) => value,
            None => V::new(&mut window, &mut cx),
        };
        value.attach(&mut window, &mut cx);
        *entity.value.borrow_mut() = Some(value);
        Self {
            app,
            entity,
            last_root: None,
            busy: false,
            renders: 0,
        }
    }
    pub fn entity(&self) -> Entity<V> {
        self.entity.clone()
    }
    pub fn host(&self) -> Host {
        self.app.host()
    }
    pub(crate) fn app_mut(&mut self) -> &mut App {
        &mut self.app
    }
    /// Runs one tick and hands its frame to `send` — the host's encoder —
    /// before putting back into the kept tree what the frame carried out of
    /// it. The tree a frame sends whole, and the subtrees its patches
    /// carry, are moved there and back rather than copied: the frame is
    /// gone once it is encoded, and the kept tree is the next frame's base.
    pub(crate) fn tick_with<R>(
        &mut self,
        events: Vec<wire::Event>,
        send: impl FnOnce(&wire::Frame) -> R,
    ) -> R {
        let frame = self.tick_wire(events);
        let sent = send(&frame);
        self.put_back(frame);
        sent
    }

    fn put_back(&mut self, frame: wire::Frame) {
        let kept = match frame.root {
            Some(root) => self.last_root.insert(root),
            None if frame.patches.is_empty() => return,
            None => {
                let kept = self
                    .last_root
                    .as_mut()
                    .expect("a patch frame has a tree to patch");
                put_back(kept, frame.patches);
                kept
            }
        };
        // Remembered without the picture bytes this frame carried: the
        // next view names those pictures by hash alone, and that is
        // the same tree — and the tree the host keeps, which drops the
        // bytes the same way once it has the pictures.
        kept.for_each_mut(&mut |node| match node {
            wire::Node::Svg {
                source: wire::SvgSource::Data { bytes, .. },
                ..
            } => *bytes = None,
            wire::Node::Image { data, .. } => *data = None,
            _ => {}
        });
    }

    fn tick_wire(&mut self, events: Vec<wire::Event>) -> wire::Frame {
        self.busy = false;
        let owed = slots::start_frame(&self.app.inner.slots);
        self.settle();
        for event in events {
            let editor_event = matches!(
                &event,
                wire::Event::EditorDocument { .. }
                    | wire::Event::EditorRequest { .. }
                    | wire::Event::EditorTransaction { .. }
            );
            if let Some(callback) = self.dispatch(event) {
                self.entity.clone().update_app(&mut self.app, |v, w, cx| {
                    callback(v, w, cx);
                    if editor_event {
                        cx.notify();
                    }
                });
                self.settle();
            }
        }
        self.settle();
        self.frame(owed)
    }

    /// Runs one event: its route or its handler. A handler's message comes
    /// back for the view to run; everything else has already happened.
    fn dispatch(&mut self, event: wire::Event) -> Option<Callback<V>> {
        match event {
            wire::Event::Message(index) => {
                let slots = self.app.inner.slots.clone();
                let mut window = self.app.window();
                slots::run_message_route(&slots, index, &mut window, &mut self.app);
                None
            }
            wire::Event::Click { handler, event } | wire::Event::AuxClick { handler, event } => {
                let slots = self.app.inner.slots.clone();
                let mut window = self.app.window();
                slots::run_click(&slots, handler, &event.into(), &mut window, &mut self.app);
                None
            }
            wire::Event::MouseDown { handler, event, .. } => {
                self.route(handler, &event.into_gpui())
            }
            wire::Event::MouseUp { handler, event, .. } => self.route(handler, &event.into_gpui()),
            wire::Event::MouseDownOut { handler, event } => self.route(handler, &event.into_gpui()),
            wire::Event::MouseUpOut { handler, event } => self.route(handler, &event.into_gpui()),
            wire::Event::MousePressure { handler, event, .. } => {
                self.route(handler, &event.into_gpui())
            }
            wire::Event::MouseMove { handler, event, .. } => {
                self.route(handler, &event.into_gpui())
            }
            wire::Event::MouseExit { handler, event, .. } => {
                self.route(handler, &event.into_gpui())
            }
            wire::Event::ScrollWheel { handler, event, .. } => {
                self.route(handler, &event.into_gpui())
            }
            wire::Event::Pinch { handler, event, .. } => self.route(handler, &event.into_gpui()),
            wire::Event::KeyDown { handler, event, .. } => self.route(handler, &event.into_gpui()),
            wire::Event::KeyUp { handler, event, .. } => self.route(handler, &event.into_gpui()),
            wire::Event::ModifiersChanged { handler, event } => {
                self.route(handler, &event.into_gpui())
            }
            wire::Event::Hover { handler, hovered } => self.route(handler, &hovered),
            wire::Event::FileDropExit { handler } => {
                self.route(handler, &gpui::FileDropEvent::Exited)
            }
            wire::Event::RichTextHover { handler, event } => self.route(handler, &event),
            wire::Event::ListScroll { handler, event } => self.route(handler, &event),
            wire::Event::ListRequest { handler, request } => self.route(handler, &request),
            wire::Event::TooltipRequest {
                request,
                character_index,
            } => {
                self.tooltip(request, character_index);
                None
            }
            wire::Event::Input { handler, text } => {
                self.route_or_handle(handler, text, |text| text)
            }
            wire::Event::Select { handler, index } => {
                self.route_or_handle(handler, index, |index| index)
            }
            wire::Event::Size {
                handler,
                width,
                height,
            } => self.route_or_handle(handler, (px(width), px(height)), |_| (width, height)),
            wire::Event::Drag { handler, dx, dy } => {
                self.route_or_handle(handler, (px(dx as f32), px(dy as f32)), |_| (dx, dy))
            }
            wire::Event::EditorDocument { handler, message } => {
                self.editor_document(handler, message)
            }
            wire::Event::EditorRequest { handler, request } => self.handle(handler, request),
            wire::Event::EditorTransaction { handler, event } => {
                self.editor_transaction(handler, event)
            }
            wire::Event::ScrollOffset {
                handler,
                x,
                y,
                relative_x,
                relative_y,
            } => self.handle(handler, (x, y, relative_x, relative_y)),
            wire::Event::UniformListRange {
                path,
                route,
                start,
                end,
            } => {
                if self.app.inner.uniform_lists.request_range(
                    path,
                    route,
                    start as usize,
                    end as usize,
                ) {
                    self.app.notify();
                }
                None
            }
            wire::Event::UniformListState {
                path,
                route,
                top_index,
                scrollable,
                scrolled_to_end,
            } => {
                self.app.inner.uniform_lists.update_state(
                    &path,
                    route,
                    top_index as usize,
                    scrollable,
                    scrolled_to_end,
                );
                None
            }
            wire::Event::Theme { dark } => {
                self.app
                    .set_global(if dark { Theme::dark() } else { Theme::light() });
                self.app.notify();
                None
            }
            wire::Event::Response { id, result, done } => {
                self.app.host().fulfill(id, result, done);
                // Response order is semantic: queued hidden data must be
                // applied before a later visibility notification.
                self.settle();
                None
            }
            // The host dropped the tree the patches build on.
            wire::Event::Resync => {
                self.last_root = None;
                slots::clear_pictures(&self.app.inner.slots);
                self.app.notify();
                None
            }
            wire::Event::A11yAction { handler, data } => self.route(handler, &data),
        }
    }

    /// A route listener that takes this event, else the handler's message
    /// with the value the handler takes (built from the routed event).
    fn route_or_handle<R: 'static, H: 'static>(
        &mut self,
        handler: u32,
        route_event: R,
        value: impl FnOnce(R) -> H,
    ) -> Option<Callback<V>> {
        let slots = self.app.inner.slots.clone();
        let mut window = self.app.window();
        if slots::run_route(&slots, handler, &route_event, &mut window, &mut self.app) {
            None
        } else {
            slots::run_handler::<H, Callback<V>>(&slots, handler, value(route_event))
        }
    }

    fn handle<H: 'static>(&self, handler: u32, value: H) -> Option<Callback<V>> {
        slots::run_handler::<H, Callback<V>>(&self.app.inner.slots, handler, value)
    }

    fn tooltip(&mut self, request: u32, character_index: Option<u32>) {
        let slots = self.app.inner.slots.clone();
        if let Some(build) = slots::tooltip_route(&slots, request) {
            let mut window = self.app.window();
            let content = build(
                character_index.map(|index| index as usize),
                &mut window,
                &mut self.app,
            )
            .map(|view| Box::new(Lowering::new(&mut window, &mut self.app).lower(view)));
            slots::tooltip_response(
                &slots,
                wire::TooltipResponse {
                    request,
                    character_index,
                    content,
                },
            );
        }
    }

    fn editor_document(
        &self,
        handler: u32,
        message: wire::editor_document::EditorDocumentMessage,
    ) -> Option<Callback<V>> {
        use wire::editor_document::EditorDocumentMessage;
        if matches!(
            message,
            EditorDocumentMessage::Acknowledged { .. } | EditorDocumentMessage::Failed { .. }
        ) {
            slots::finish_editor_transfer(&self.app.inner.slots, message.id());
            return None;
        }
        self.handle(handler, message)
    }

    fn editor_transaction(
        &self,
        handler: u32,
        event: wire::EditorTransactionEvent,
    ) -> Option<Callback<V>> {
        if matches!(
            event,
            wire::EditorTransactionEvent::Fault { .. }
                | wire::EditorTransactionEvent::Cancelled { .. }
        ) {
            slots::finish_editor_transfer(&self.app.inner.slots, &event.id().into());
        }
        if let wire::EditorTransactionEvent::Cancelled { id, .. } = &event {
            if !slots::editor_matches_pending(&self.app.inner.slots, id) {
                return None;
            }
            slots::editor_acknowledge(&self.app.inner.slots, &event);
        }
        self.handle(handler, event)
    }

    /// The frame after this tick's events: the tree lowered if anything
    /// changed, or if the last frame owed a picture it drew, then sent whole
    /// or as patches against the last one. A frame that owes a picture
    /// asks for the next, until every picture drawn is sent.
    fn frame(&mut self, owed: bool) -> wire::Frame {
        let render = self.app.inner.dirty.replace(false)
            || self.last_root.is_none()
            || slots::editor_transferring(&self.app.inner.slots)
            || owed;
        let mut root = render.then(|| self.render_root());
        self.busy |= self.app.inner.dirty.get()
            || executor::ready(&self.app.inner.tasks.borrow())
            || slots::pictures_owed(&self.app.inner.slots);
        // Patches against the last tree, unless there is none — a first
        // frame or a resync. Nothing lowered is nothing changed, and a
        // lowered tree that diffs to no patches is the same tree
        // (`apply(old, diff(old, new))` leaves `old == new`).
        let mut patches = Vec::new();
        let unchanged = match (&mut root, &mut self.last_root) {
            (None, _) => true,
            (Some(root), Some(last)) => {
                patches = wire::diff_taking(last, root);
                patches.is_empty()
            }
            (Some(_), None) => false,
        };
        if let Some(tree) = root.take().filter(|_| !unchanged) {
            root = self.keep(tree, &mut patches);
        }
        let editor_decisions = slots::take_editor_responses(&self.app.inner.slots);
        self.busy |= slots::editor_responses_ready(&self.app.inner.slots);
        let host = self.app.host();
        let requests = host.drain_outbox();
        let cancels = host.drain_cancels();
        // what one frame cannot carry goes in the next
        self.busy |= host.outbox_waiting();
        wire::Frame {
            upstream_sanitization: Default::default(),
            editor_decisions,
            editor_documents: slots::take_editor_documents(&self.app.inner.slots),
            tooltip_responses: slots::take_tooltip_responses(&self.app.inner.slots),
            root,
            patches,
            requests,
            cancels,
            unchanged,
            busy: self.busy,
        }
    }

    fn render_root(&mut self) -> wire::Node {
        self.renders += 1;
        slots::reset(&self.app.inner.slots);
        let mut window = self.app.window();
        let element = {
            let mut cx = Context {
                app: &mut self.app,
                entity: self.entity.clone(),
            };
            let mut view = self.entity.value.borrow_mut();
            view.as_mut()
                .expect("entity initialized")
                .render(&mut window, &mut cx)
                .into_element()
        };
        Lowering::new(&mut window, &mut self.app).lower_element(element)
    }

    /// Keeps a changed tree as the next frame's base and says what to send:
    /// the tree itself (patches cleared), or nothing beside the patches.
    /// The diff took the patches' subtrees out of `tree`; a tree sent whole
    /// gets them back here, a patched one in [`Driver::put_back`] once the
    /// frame is sent.
    fn keep(&mut self, mut tree: wire::Node, patches: &mut Vec<wire::Patch>) -> Option<wire::Node> {
        // Property changes remain patches even for tiny trees; replacing
        // their identity would discard native state. Structural edits
        // use the whole tree when it has no more nodes than the patches
        // carry (counted, not encoded: sizing both by encoding them was
        // most of a frame's cost).
        let only_props = patches
            .iter()
            .all(|patch| matches!(patch, wire::Patch::Props { .. }));
        let carried = || -> usize {
            patches
                .iter()
                .map(|patch| match patch {
                    wire::Patch::Replace { node, .. } | wire::Patch::Insert { node, .. } => {
                        node.count()
                    }
                    _ => 1,
                })
                .sum()
        };
        // The tree's count as rendered: the hollowed tree plus what each
        // taken subtree holds beyond its stand-in.
        let nodes = || -> usize {
            let taken: usize = patches
                .iter()
                .map(|patch| match patch {
                    wire::Patch::Replace { node, .. } | wire::Patch::Insert { node, .. } => {
                        node.count() - 1
                    }
                    _ => 0,
                })
                .sum();
            tree.count() + taken
        };
        if patches.len() > wire::MAX_PATCHES || (!only_props && carried() >= nodes()) {
            put_back(&mut tree, std::mem::take(patches));
        }
        // A frame that carries patches sends no tree, so the tree itself is
        // kept; one that sends the tree has nothing to keep until the tree
        // comes back from the encoder.
        match patches.is_empty() {
            true => {
                self.last_root = None;
                Some(tree)
            }
            false => {
                self.last_root = Some(tree);
                None
            }
        }
    }

    /// Runs a route listener. A routed event carries no message back.
    fn route<E: 'static>(&mut self, handler: u32, event: &E) -> Option<Callback<V>> {
        let slots = self.app.inner.slots.clone();
        let mut window = self.app.window();
        slots::run_route(&slots, handler, event, &mut window, &mut self.app);
        None
    }

    fn settle(&mut self) {
        for _ in 0..MAX_ROUNDS {
            let mut tasks = std::mem::take(&mut *self.app.inner.tasks.borrow_mut());
            let cut_short = executor::poll(&mut tasks);
            self.app.refresh_globals();
            let added = !self.app.inner.tasks.borrow().is_empty();
            tasks.append(&mut self.app.inner.tasks.borrow_mut());
            *self.app.inner.tasks.borrow_mut() = tasks;
            self.busy |= cut_short;
            if !added {
                return;
            }
        }
        self.busy = true;
    }
}

/// Puts the subtrees `patches` carry back into the tree `wire::diff_taking`
/// took them from, which is `new` as it was: a patch's path and index are
/// the position of its node in `new`. Each subtree goes into its stand-in's
/// slot, never inserted, so the patches' order does not matter. Patches
/// that carry no subtree are skipped.
pub(crate) fn put_back(new: &mut wire::Node, patches: Vec<wire::Patch>) {
    for patch in patches {
        let (path, index, node) = match patch {
            wire::Patch::Replace { path, node } => (path, None, node),
            wire::Patch::Insert { path, index, node } => (path, Some(index), node),
            wire::Patch::Props { .. } | wire::Patch::Remove { .. } | wire::Patch::Move { .. } => {
                continue;
            }
        };
        let mut target = &mut *new;
        for index in path {
            target = &mut target.children_mut()[index as usize];
        }
        match index {
            Some(index) => target.children_mut()[index as usize] = node,
            None => *target = node,
        }
    }
}
