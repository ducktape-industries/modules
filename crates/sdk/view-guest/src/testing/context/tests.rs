use super::*;
use crate::methods::Capability;
use crate::{
    ClickEvent, Context, InteractiveElement, KeyDownEvent, ParentElement, Render, Role,
    StatefulInteractiveElement, Task, Window, methods::Changes, testing::Probe,
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
    // subscribes in `attach` alone: `new` is the default
    fn attach(&mut self, _: &mut Window, cx: &mut Context<Self>) {
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

/// A view that follows the host in `attach` alone keeps following it
/// after a redeploy restores it from its snapshot, and hears each item
/// once.
#[test]
fn a_view_that_subscribes_in_attach_keeps_its_followers_across_a_restore() {
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
struct Untargeted {
    #[serde(skip)]
    live: Option<Task<()>>,
}
impl View for Untargeted {
    const NAME: &'static str = "Untargeted";
    const CAPABILITIES: &'static [Capability] = &[Capability::Module];
    const TARGETS: &'static [&'static str] = &["other"];
    fn attach(&mut self, _: &mut Window, cx: &mut Context<Self>) {
        let live = cx.host().subscribe::<Changes<Probe>>(());
        self.live = Some(cx.for_each(live, |_: &mut Self, _, _, _| {}));
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
#[should_panic(
    expected = "the host refuses this frame: duplicate typed element identity among siblings: \
                1 twice under the root"
)]
fn every_frame_is_held_to_the_host_sanitizer() {
    TestAppContext::new().open::<Twins>();
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
    cx.simulate_hover("target", true);
}

/// A hundred rows in a uniform list.
#[derive(Default, Serialize, Deserialize)]
struct Rows;
impl View for Rows {
    const NAME: &'static str = "Rows";
}
impl Render for Rows {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl crate::IntoElement {
        crate::uniform_list("rows", 100, |range, _, _| {
            range
                .map(|row| crate::div().id(row).child(format!("row {row}")))
                .collect::<Vec<_>>()
        })
    }
}

fn rows(rows: impl Iterator<Item = usize>) -> Vec<String> {
    rows.map(|row| format!("row {row}")).collect()
}

/// A test sees the rows the host shows: the one it measures on the first
/// frame, then the ones a viewport holds, then the ones scrolled to.
#[test]
fn a_uniform_list_shows_the_rows_its_viewport_shows() {
    let mut cx = TestAppContext::new();
    cx.open::<Rows>();
    assert_eq!(cx.texts(), rows(0..1));
    cx.simulate_viewport(10);
    assert_eq!(cx.texts(), rows(0..10));
    cx.simulate_range("rows", 40..50);
    assert_eq!(cx.texts(), rows(std::iter::once(0).chain(40..50)));
}

/// A pane that hears every key on its way down and back up, around a list
/// that hears the arrows, a button beside it, and one that moves the
/// keyboard to the list.
#[derive(Default, Serialize, Deserialize)]
struct Keys {
    heard: Vec<String>,
}
impl Keys {
    fn hear(
        at: &'static str,
    ) -> impl Fn(&mut Self, &KeyDownEvent, &mut Window, &mut Context<Self>) {
        move |view, event, _, cx| {
            view.heard.push(format!("{at} {}", event.keystroke.key));
            cx.notify();
        }
    }
}
impl View for Keys {
    const NAME: &'static str = "Keys";
    const CAPABILITIES: &'static [Capability] = &[Capability::Host];
}
impl Render for Keys {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl crate::IntoElement {
        crate::div()
            .id("pane")
            .role(Role::Group)
            .aria_label("Pane")
            .capture_key_down(cx.listener(Self::hear("pane capture")))
            .on_key_down(cx.listener(Self::hear("pane")))
            .child(
                crate::div()
                    .id("list")
                    .role(Role::ListBox)
                    .aria_label("Rows")
                    .focusable()
                    .on_key_down(cx.listener(Self::hear("list")))
                    .child("row"),
            )
            .child(
                crate::div()
                    .id("other")
                    .role(Role::Button)
                    .focusable()
                    .on_click(|_: &ClickEvent, _, _| {})
                    .child("Other"),
            )
            .child(
                crate::div()
                    .id("jump")
                    .role(Role::Button)
                    .focusable()
                    .on_click(|_: &ClickEvent, window, _| window.focus("list"))
                    .child("Jump"),
            )
            .child(
                crate::div()
                    .id("note")
                    .child(format!("{} keys", self.heard.len())),
            )
    }
}

/// Nothing has the keyboard: the host delivers the key to no node, and
/// the test fails instead of firing the list's listener.
#[test]
#[should_panic(
    expected = "the host refuses this key: \"list\" is not on the focus path (nothing has focus)"
)]
fn a_key_at_a_node_nothing_focused_is_refused() {
    let mut cx = TestAppContext::new();
    cx.open::<Keys>();
    cx.simulate_key_down("list", "down");
}

#[test]
#[should_panic(expected = "\"list\" is not on the focus path (focus is at \"other\")")]
fn a_key_at_a_sibling_of_the_focused_node_is_refused() {
    let mut cx = TestAppContext::new();
    cx.open::<Keys>();
    cx.simulate_click("other");
    cx.simulate_key_down("list", "down");
}

/// A key goes down the focus path to the capture listeners, then back up
/// it, focused node first; a press moves the keyboard, and so does the
/// view's own `Window::focus`.
#[test]
fn keys_go_down_the_focus_path_and_back_up_it() {
    let mut cx = TestAppContext::new();
    let keys = cx.open::<Keys>();
    cx.simulate_focus("list");
    cx.simulate_key_down("list", "down");
    keys.read(|view| assert_eq!(view.heard, ["pane capture down", "list down", "pane down"]));
    cx.simulate_click("other");
    assert_eq!(cx.focused().and_then(Node::key), Some("other"));
    cx.simulate_key_down("pane", "escape");
    keys.read(|view| assert_eq!(view.heard[3..], ["pane capture escape", "pane escape"]));
    cx.simulate_click("jump");
    assert_eq!(cx.focused().and_then(Node::key), Some("list"));
}

#[test]
#[should_panic(expected = "\"note\" cannot hold focus")]
fn a_node_that_cannot_hold_focus_is_not_focused() {
    let mut cx = TestAppContext::new();
    cx.open::<Keys>();
    cx.simulate_focus("note");
}

/// A screen with a dialog that opens over it, holding one button.
#[derive(Default, Serialize, Deserialize)]
struct Dialog {
    open: bool,
}
impl View for Dialog {
    const NAME: &'static str = "Dialog";
}
impl Render for Dialog {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl crate::IntoElement {
        let open = cx.listener(|view: &mut Self, _: &ClickEvent, _, cx| {
            view.open = true;
            cx.notify();
        });
        let base = crate::div()
            .id("screen")
            .role(Role::Button)
            .focusable()
            .on_click(open)
            .child("Open");
        let modal = self.open.then(|| {
            crate::div()
                .id("card")
                .child(crate::div().child("Sure?"))
                .child(
                    crate::div()
                        .id("confirm")
                        .role(Role::Button)
                        .focusable()
                        .on_click(|_: &ClickEvent, _, _| {})
                        .child("Confirm"),
                )
        });
        crate::modal_overlay("dialog", "Confirm", base, modal)
    }
}

/// A dialog that opens takes the keyboard to its first Tab stop, as the
/// host's focus trap does.
#[test]
fn a_dialog_that_opens_takes_the_keyboard() {
    let mut cx = TestAppContext::new();
    cx.open::<Dialog>();
    cx.simulate_click("screen");
    assert_eq!(cx.focused().and_then(Node::key), Some("confirm"));
}

/// Tab walks the view's Tab stops in document order; in an open dialog it
/// goes round the dialog's, never out to the screen under it.
#[test]
fn tab_stays_in_an_open_dialog() {
    let mut cx = TestAppContext::new();
    cx.open::<Dialog>();
    cx.simulate_tab(true);
    assert_eq!(cx.focused().and_then(Node::key), Some("screen"));
    cx.simulate_click("screen");
    assert_eq!(cx.focused().and_then(Node::key), Some("confirm"));
    for forward in [true, false] {
        cx.simulate_tab(forward);
        assert_eq!(
            cx.focused().and_then(Node::key),
            Some("confirm"),
            "the dialog's one stop, not the screen"
        );
    }
}

/// Three buttons; the first moves the keyboard on as Tab does, and a list
/// that asks for the keys by id is refused: the host focuses a container
/// or a field.
#[derive(Default, Serialize, Deserialize)]
struct Moves;
impl View for Moves {
    const NAME: &'static str = "Moves";
    const CAPABILITIES: &'static [Capability] = &[Capability::Host];
}
impl Render for Moves {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl crate::IntoElement {
        let press = |id: &'static str, then: fn(&mut Window)| {
            crate::div()
                .id(id)
                .role(Role::Button)
                .focusable()
                .on_click(move |_: &ClickEvent, window, _| then(window))
                .child(id)
        };
        crate::div()
            .id("moves")
            .child(press("next", |window| window.focus_next()))
            .child(press("back", |window| window.focus_prev()))
            .child(press("rows", |window| window.focus("table")))
            .child(crate::uniform_list("table", 3, |range, _, _| {
                range
                    .map(|row| crate::div().id(row).child(format!("row {row}")))
                    .collect()
            }))
    }
}

#[test]
fn focus_next_and_prev_move_as_tab_does() {
    let mut cx = TestAppContext::new();
    cx.open::<Moves>();
    cx.simulate_click("next");
    assert_eq!(cx.focused().and_then(Node::key), Some("back"));
    cx.simulate_click("back");
    assert_eq!(cx.focused().and_then(Node::key), Some("next"));
}

#[test]
#[should_panic(expected = "it focuses a container or a field")]
fn a_focus_on_a_uniform_list_is_refused() {
    let mut cx = TestAppContext::new();
    cx.open::<Moves>();
    cx.simulate_click("rows");
}

/// Asks the host three hundred times on its first tick.
#[derive(Default, Serialize, Deserialize)]
struct Fanout {
    answers: usize,
    #[serde(skip)]
    asks: Vec<Task<()>>,
}
impl View for Fanout {
    const NAME: &'static str = "Fanout";
    const CAPABILITIES: &'static [Capability] = &[Capability::Host];
    fn attach(&mut self, _: &mut Window, cx: &mut Context<Self>) {
        for n in 0..300 {
            let ask = cx.host().ask::<crate::methods::HostId>(format!("n{n}"));
            self.asks.push(cx.spawn(async move |this, cx| {
                ask.await.unwrap();
                this.update(cx, |view, cx| {
                    view.answers += 1;
                    cx.notify();
                })
                .unwrap();
            }));
        }
    }
}
impl Render for Fanout {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl crate::IntoElement {
        crate::div().child(self.answers.to_string())
    }
}

/// One frame carries at most `MAX_REQUESTS`: the rest go in the next, in
/// order, and every ask is answered; the host never sees a frame it
/// would refuse.
#[test]
fn three_hundred_asks_go_out_256_then_44_and_every_one_is_answered() {
    let mut cx = TestAppContext::new();
    cx.host()
        .handle::<crate::methods::HostId>(|prefix| Ok(format!("{prefix}-1")));
    let fanout = cx.open::<Fanout>();
    let sent: Vec<_> = cx.reports().iter().map(|tick| tick.requests).collect();
    assert_eq!(sent, [256, 44, 0]);
    fanout.read(|view| assert_eq!(view.answers, 300));
    let asked = cx.host().requests::<crate::methods::HostId>();
    assert_eq!(asked.first().map(String::as_str), Some("n0"));
    assert_eq!(asked.last().map(String::as_str), Some("n299"));
}

/// What a tick cost: the first one renders the tree whole, a tick that
/// changes nothing sends nothing.
#[test]
fn a_tick_reports_what_it_rendered_and_sent() {
    let mut cx = TestAppContext::new();
    cx.open::<Keys>();
    let first = cx.reports()[0];
    assert!(first.rendered && first.patches == 0 && first.bytes > 0);
    assert_eq!(first.nodes, cx.root().count());
    let quiet = cx.tick(Vec::new());
    assert!(!quiet.rendered && quiet.patches == 0 && !quiet.busy);
    cx.simulate_focus("list");
    cx.simulate_key_down("list", "down");
    let key = cx.reports()[0];
    assert!(key.rendered && key.patches > 0 && key.bytes < first.bytes);
}
