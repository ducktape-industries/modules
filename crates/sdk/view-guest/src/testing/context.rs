use super::{FakeHost, assert_frame_accessible, find, texts};
use crate::{
    App, Driver, Entity, View,
    host::Host,
    wire::{Event, Frame, Node},
};

trait TestDriver {
    fn tick(&mut self, events: Vec<Event>) -> Frame;
    fn app_mut(&mut self) -> &mut App;
    fn host(&self) -> Host;
    fn snapshot(&self) -> Result<Vec<u8>, String>;
}
impl<V: View> TestDriver for Driver<V> {
    // what the host gets: the context patches its tree as the host does
    fn tick(&mut self, events: Vec<Event>) -> Frame {
        self.tick_with(events, Frame::clone)
    }
    fn app_mut(&mut self) -> &mut App {
        self.app_mut()
    }
    fn host(&self) -> Host {
        self.host()
    }
    fn snapshot(&self) -> Result<Vec<u8>, String> {
        self.snapshot()
    }
}

/// A single view and its typed host, driven until no immediate work remains.
#[derive(Default)]
pub struct TestAppContext {
    host: FakeHost,
    driver: Option<Box<dyn TestDriver>>,
    frame: Frame,
    globals: crate::context::Globals,
}

impl TestAppContext {
    pub fn new() -> Self {
        Self::default()
    }
    pub fn host(&self) -> FakeHost {
        self.host.clone()
    }
    pub fn open<V: View>(&mut self) -> Entity<V> {
        self.host.declare(V::CAPABILITIES, V::TARGETS);
        let driver = Driver::<V>::initialize_in(self.fresh_app(), None);
        let entity = driver.entity();
        self.host.reset_connection();
        self.driver = Some(Box::new(driver));
        self.frame = Frame::default();
        self.run_until_parked();
        entity
    }
    pub fn snapshot(&self) -> Result<Vec<u8>, String> {
        self.driver.as_ref().expect("open a view first").snapshot()
    }
    pub fn restore<V: View>(&mut self, bytes: &[u8]) -> Result<Entity<V>, String> {
        self.host.declare(V::CAPABILITIES, V::TARGETS);
        let driver = Driver::<V>::from_snapshot_in(self.fresh_app(), bytes)?;
        let entity = driver.entity();
        self.host.reset_connection();
        self.driver = Some(Box::new(driver));
        self.frame = Frame::default();
        self.run_until_parked();
        Ok(entity)
    }
    pub(crate) fn app_mut(&mut self) -> &mut App {
        self.driver.as_mut().expect("open a view first").app_mut()
    }
    fn fresh_app(&self) -> App {
        let mut app = App::for_driver();
        for (kind, value) in &self.globals {
            app.set_shared_global(*kind, value.clone());
        }
        app
    }
    /// Set a global before opening a view, or rerender the current view with it.
    pub fn set_global<G: gpui::Global>(&mut self, global: G) {
        let kind = std::any::TypeId::of::<G>();
        let global: std::rc::Rc<dyn std::any::Any> = std::rc::Rc::new(global);
        self.globals.insert(kind, global.clone());
        if let Some(driver) = &mut self.driver {
            let app = driver.app_mut();
            app.set_shared_global(kind, global);
            app.notify();
            self.run_until_parked();
        }
    }
    pub fn run_until_parked(&mut self) {
        self.dispatch(Vec::new());
    }
    /// Ticks until the view parks, holding every frame it sends to the
    /// host's sanitizer, which must take it, and to `view_wire::audit`.
    fn dispatch(&mut self, mut events: Vec<Event>) {
        for _ in 0..10_000 {
            events.extend(self.host.take_events());
            let driver = self.driver.as_mut().expect("open a view first");
            let mut frame = driver.tick(std::mem::take(&mut events));
            self.host.accept(&frame, &driver.host());
            if frame.root.is_none() {
                frame.root = self.frame.root.take();
                if !frame.patches.is_empty() {
                    crate::wire::apply(
                        frame.root.as_mut().expect("patch needs previous tree"),
                        std::mem::take(&mut frame.patches),
                    )
                    .expect("valid view patches");
                }
            }
            assert_frame_accessible(&frame);
            let busy = frame.busy;
            self.frame = frame;
            events = self.host.take_events();
            if events.is_empty() && !busy {
                return;
            }
        }
        panic!("view did not park after 10000 ticks");
    }
    pub fn texts(&self) -> Vec<String> {
        texts(&self.frame)
    }
    pub fn has_text(&self, text: &str) -> bool {
        super::has_text(&self.frame, text)
    }
    pub fn find(&self, key: &str) -> Option<&Node> {
        find(&self.frame, key)
    }
    /// The role, aria, focus and routes of the node `key` names, which
    /// must be one of the kinds that carry them.
    pub fn interactivity(&self, key: &str) -> &crate::wire::Interactivity {
        self.find(key)
            .unwrap_or_else(|| panic!("no node {key:?}"))
            .interactivity()
            .unwrap_or_else(|| panic!("{key:?} carries no interactivity"))
    }
    /// The whole tree of the last frame, for node-less host renders.
    pub fn root(&self) -> &Node {
        self.frame.root.as_ref().expect("view has a tree")
    }
    /// The last frame's whole tree as a host would take it on a fresh mount:
    /// asserts the host's sanitizer passes it through untouched (inside
    /// every node, text and picture budget) and answers its encoded bytes —
    /// the proxy a native test has for the fuel a render spends.
    pub fn frame_bytes(&self) -> usize {
        let frame = Frame {
            root: Some(self.root().clone()),
            ..Frame::default()
        };
        let mut sanitized = frame.clone();
        crate::wire::sanitize(&mut sanitized).expect("the frame sanitizes");
        assert!(
            sanitized.root == frame.root,
            "the host would cut this frame: it is past a frame budget"
        );
        crate::wire::encode(&frame).len()
    }
    pub fn simulate_click(&mut self, key: &str) {
        self.dispatch(super::press(&self.frame, key));
    }
    pub fn simulate_input(&mut self, key: &str, text: &str) {
        self.dispatch(super::type_into(&self.frame, key, text));
    }
    pub fn simulate_rich_click(&mut self, key: &str, index: usize) {
        self.dispatch(vec![super::rich_click(&self.frame, key, index)]);
    }
    pub fn simulate_submit(&mut self, key: &str) {
        self.dispatch(super::submit(&self.frame, key));
    }
    pub fn simulate_measure(&mut self, key: &str, width: f32, height: f32) {
        self.dispatch(super::measure(&self.frame, key, width, height));
    }
    pub fn simulate_drag(&mut self, key: &str, dx: f64, dy: f64) {
        self.dispatch(super::drag(&self.frame, key, dx, dy));
    }
    pub fn simulate_key_down(&mut self, key: &str, keystroke: &str) {
        self.dispatch(super::key_down(&self.frame, key, keystroke));
    }
    pub fn simulate_dismiss(&mut self, key: &str) {
        self.dispatch(super::dismiss(&self.frame, key));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::methods::Capability;
    use crate::{
        Context, InteractiveElement, ParentElement, Render, StatefulInteractiveElement, Task,
        Window, methods::Changes, testing::Probe,
    };
    use futures::StreamExt;
    use serde::{Deserialize, Serialize};

    #[derive(Default, Serialize, Deserialize)]
    struct LiveView {
        items: usize,
        #[serde(skip)]
        task: Option<Task<()>>,
    }
    impl View for LiveView {
        const NAME: &'static str = "LiveView";
        const CAPABILITIES: &'static [Capability] = &[Capability::Module];
        const TARGETS: &'static [&'static str] = &[<Probe as crate::methods::Program>::NAME];
        fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
            let mut view = Self::default();
            view.restored(window, cx);
            view
        }
        fn restored(&mut self, _: &mut Window, cx: &mut Context<Self>) {
            let mut stream = cx.host().subscribe::<Changes<Probe>>(());
            self.task = Some(cx.spawn(async move |this, cx| {
                while let Some(item) = stream.next().await {
                    item.unwrap();
                    this.update(cx, |view, cx| {
                        view.items += 1;
                        cx.notify();
                    })
                    .unwrap();
                }
            }));
        }
    }
    impl Render for LiveView {
        fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl crate::IntoElement {
            crate::div().id("items").child(self.items.to_string())
        }
    }

    #[test]
    fn restoring_resubscribes_without_replaying_old_events_or_duplicate_ids() {
        let mut cx = TestAppContext::new();
        let feed = cx.host().stream::<Changes<Probe>>();
        cx.open::<LiveView>();
        feed.send(None);
        cx.run_until_parked();
        assert!(cx.has_text("1"));
        let snapshot = cx.snapshot().unwrap();
        assert!(snapshot[0] >= 0x80, "a named MessagePack map, not JSON");
        feed.send(None);
        let restored = cx.restore::<LiveView>(&snapshot).unwrap();
        restored.read(|view| assert_eq!(view.items, 1));
        feed.send(None);
        cx.run_until_parked();
        restored.read(|view| assert_eq!(view.items, 2));
        assert_eq!(cx.host().requests::<Changes<Probe>>().len(), 2);
    }

    /// Logs through `host`, which its manifest leaves out.
    #[derive(Default, Serialize, Deserialize)]
    struct Undeclared;
    impl View for Undeclared {
        const NAME: &'static str = "Undeclared";
        const CAPABILITIES: &'static [Capability] = &[Capability::Module];
        fn new(_: &mut Window, cx: &mut Context<Self>) -> Self {
            cx.host().log("hello");
            Self
        }
    }
    impl Render for Undeclared {
        fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl crate::IntoElement {
            crate::div()
        }
    }

    #[test]
    #[should_panic(expected = "undeclared_capability")]
    fn a_method_the_manifest_leaves_out_fails_the_test() {
        TestAppContext::new().open::<Undeclared>();
    }

    /// Follows `Probe`, which its `TARGETS` does not name.
    #[derive(Default, Serialize, Deserialize)]
    struct Untargeted;
    impl View for Untargeted {
        const NAME: &'static str = "Untargeted";
        const CAPABILITIES: &'static [Capability] = &[Capability::Module];
        const TARGETS: &'static [&'static str] = &["other"];
        fn new(_: &mut Window, cx: &mut Context<Self>) -> Self {
            let _ = cx.host().subscribe::<Changes<Probe>>(());
            Self
        }
    }
    impl Render for Untargeted {
        fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl crate::IntoElement {
            crate::div()
        }
    }

    #[test]
    #[should_panic(expected = "undeclared_target")]
    fn a_program_the_targets_leave_out_fails_the_test() {
        TestAppContext::new().open::<Untargeted>();
    }

    /// A button with nothing to say for itself.
    #[derive(Default, Serialize, Deserialize)]
    struct Nameless;
    impl View for Nameless {
        const NAME: &'static str = "Nameless";
        fn new(_: &mut Window, _: &mut Context<Self>) -> Self {
            Self
        }
    }
    impl Render for Nameless {
        fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl crate::IntoElement {
            crate::div()
                .id("nameless")
                .role(crate::Role::Button)
                .focusable()
                .on_click(|_, _, _| {})
        }
    }

    #[test]
    #[should_panic(expected = "Unnamed at nameless")]
    fn every_frame_a_view_sends_is_audited() {
        TestAppContext::new().open::<Nameless>();
    }

    #[test]
    #[should_panic(expected = "Unnamed at nameless")]
    fn a_frame_driver_tick_returns_is_audited() {
        Driver::<Nameless>::new().tick(Vec::new());
    }

    /// Twins: two siblings with one typed id, which the audit (it keys on
    /// names) passes and the host refuses.
    #[derive(Default, Serialize, Deserialize)]
    struct Twins;
    impl View for Twins {
        const NAME: &'static str = "Twins";
        fn new(_: &mut Window, _: &mut Context<Self>) -> Self {
            Self
        }
    }
    impl Render for Twins {
        fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl crate::IntoElement {
            crate::div()
                .child(crate::div().id(1usize))
                .child(crate::div().id(1usize))
        }
    }

    #[test]
    #[should_panic(expected = "the host refuses this frame")]
    fn a_frame_driver_tick_returns_is_held_to_the_host_sanitizer() {
        Driver::<Twins>::new().tick(Vec::new());
    }

    /// A target whose tooltip is the nameless button.
    #[derive(Default, Serialize, Deserialize)]
    struct NamelessTip;
    impl View for NamelessTip {
        const NAME: &'static str = "NamelessTip";
        fn new(_: &mut Window, _: &mut Context<Self>) -> Self {
            Self
        }
    }
    impl Render for NamelessTip {
        fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl crate::IntoElement {
            crate::div()
                .id("target")
                .child("Target")
                .tooltip(|_, cx| cx.new(|_| Nameless).into())
        }
    }

    #[test]
    #[should_panic(expected = "Unnamed at nameless")]
    fn a_tooltip_the_host_draws_is_audited() {
        let mut cx = TestAppContext::new();
        cx.open::<NamelessTip>();
        let Some(Node::Container(crate::wire::ContainerNode { interactivity, .. })) =
            cx.find("target")
        else {
            panic!("the target")
        };
        let request = interactivity.tooltip.as_ref().expect("a tooltip").request;
        cx.dispatch(vec![Event::TooltipRequest {
            request,
            character_index: None,
        }]);
    }
}
