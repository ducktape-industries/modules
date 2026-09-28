use super::*;
use crate::{App, Lowering, wire};

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
