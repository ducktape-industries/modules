use super::*;
use crate::{App, Lowering, wire};
use gpui::Toggled;

fn lower(element: impl IntoElement) -> wire::Node {
    let mut app = App::for_driver();
    let mut window = app.window();
    Lowering::new(&mut window, &mut app).lower(element)
}

fn interactivity(node: &wire::Node) -> &wire::Interactivity {
    match node {
        wire::Node::Container(wire::ContainerNode { interactivity, .. })
        | wire::Node::ResizeHandle { interactivity, .. } => interactivity,
        other => panic!("no interactivity: {other:?}"),
    }
}

fn faults(node: &wire::Node) -> Vec<wire::FaultKind> {
    wire::audit(node)
        .faults
        .into_iter()
        .map(|fault| fault.kind)
        .collect()
}

#[test]
fn an_icon_button_is_a_focusable_button_named_in_words() {
    let theme = Theme::light();
    let node = lower(icon_button("close", "✕", "Close", &theme, |_, _, _| {}));
    let control = interactivity(&node);
    assert_eq!(control.role, Some(Role::Button));
    assert_eq!(control.aria.label.as_deref(), Some("Close"));
    assert!(control.focusable && control.on_click.is_some());
    assert_eq!(faults(&node), []);
}

#[test]
fn a_segmented_choice_is_a_radio_group_with_its_name() {
    let theme = Theme::light();
    let node = lower(segmented(
        "format",
        "Object format",
        &theme,
        [segment("sha1", "SHA-1", true, &theme, |_, _, _| {})],
    ));
    let group = interactivity(&node);
    assert_eq!(group.role, Some(Role::RadioGroup));
    assert_eq!(group.aria.label.as_deref(), Some("Object format"));
}

#[test]
fn a_switch_that_is_on_reports_toggled_true() {
    let theme = Theme::light();
    for (on, toggled) in [(true, Toggled::True), (false, Toggled::False)] {
        let node = lower(switch("dark", "Dark", on, true, &theme, |_, _, _| {}));
        let control = interactivity(&node);
        assert_eq!(control.role, Some(Role::Switch));
        assert_eq!(control.aria.toggled, Some(toggled));
        assert_eq!(control.aria.selected, None);
        assert_eq!(faults(&node), []);
    }
}

#[test]
fn the_picked_segment_reports_toggled_true() {
    let theme = Theme::light();
    let node = lower(segment("sha1", "SHA-1", true, &theme, |_, _, _| {}));
    let control = interactivity(&node);
    assert_eq!(control.role, Some(Role::RadioButton));
    assert_eq!(control.aria.toggled, Some(Toggled::True));
    assert_eq!(control.aria.selected, None);
}

#[test]
fn a_button_is_a_toggle_only_once_told_it_is_selected() {
    let theme = Theme::light();
    let plain = lower(button("save", "Save", &theme, |_, _, _| {}));
    assert_eq!(interactivity(&plain).aria.toggled, None);
    for (selected, toggled) in [(true, Toggled::True), (false, Toggled::False)] {
        let node = lower(button("tree", "Tree", &theme, |_, _, _| {}).selected(selected));
        let control = interactivity(&node);
        assert_eq!(control.role, Some(Role::Button));
        assert_eq!(control.aria.toggled, Some(toggled));
        assert_eq!(control.aria.selected, None);
    }
}

#[derive(Default, serde::Serialize, serde::Deserialize)]
struct Panes {
    moved: Vec<f32>,
}

impl crate::Capabilities for Panes {
    const CAPABILITIES: &'static [crate::methods::Capability] = &[];
}

impl crate::View for Panes {
    fn new(_: &mut Window, _: &mut crate::Context<Self>) -> Self {
        Self::default()
    }
}

impl crate::Render for Panes {
    fn render(&mut self, _: &mut Window, cx: &mut crate::Context<Self>) -> impl IntoElement {
        let theme = Theme::light();
        divider(
            "panes-resize",
            "Resize the list",
            &theme,
            cx,
            |panes: &mut Self, dx| panes.moved.push(dx),
        )
    }
}

#[test]
fn a_divider_is_a_named_focusable_splitter_the_arrows_move() {
    let mut cx = crate::testing::TestAppContext::new();
    let panes = cx.open::<Panes>();
    let node = cx.find("panes-resize").expect("the divider").clone();
    let handle = interactivity(&node);
    assert_eq!(handle.role, Some(Role::Splitter));
    assert_eq!(handle.aria.label.as_deref(), Some("Resize the list"));
    assert_eq!(handle.aria.orientation, Some(gpui::Orientation::Vertical));
    assert!(handle.focusable && handle.tab_stop == Some(true));
    assert_eq!(faults(&node), []);
    for keystroke in ["left", "right", "shift-left", "shift-right", "up", "a"] {
        cx.simulate_key_down("panes-resize", keystroke);
    }
    cx.simulate_drag("panes-resize", 5., 0.);
    panes.read(|panes| assert_eq!(panes.moved, [-8., 8., -32., 32., 5.]));
}
