use crate::prelude::*;
use crate::testing::TestAppContext;
use crate::{View, modal_overlay, resize_handle, sensor, wire};

#[derive(Default, serde::Deserialize, serde::Serialize)]
struct BehaviorView {
    measured: (f32, f32),
    dragged: (f32, f32),
    dismissed: bool,
}

impl crate::Capabilities for BehaviorView {
    const CAPABILITIES: &'static [crate::methods::Capability] = &[];
}

impl View for BehaviorView {
    fn new(_: &mut Window, _: &mut Context<Self>) -> Self {
        Self::default()
    }
}

#[derive(Default, serde::Deserialize, serde::Serialize)]
struct DefaultSensorView;

impl crate::Capabilities for DefaultSensorView {
    const CAPABILITIES: &'static [crate::methods::Capability] = &[];
}

impl View for DefaultSensorView {
    fn new(_: &mut Window, _: &mut Context<Self>) -> Self {
        Self
    }
}

impl Render for DefaultSensorView {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        sensor(ElementId::Name("default-sensor".into()), div())
    }
}

impl Render for BehaviorView {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let measured = cx.listener(|view, size: &(Pixels, Pixels), _, cx| {
            view.measured = (size.0.into(), size.1.into());
            cx.notify();
        });
        let dragged = cx.listener(|view, delta: &(Pixels, Pixels), _, cx| {
            view.dragged = (delta.0.into(), delta.1.into());
            cx.notify();
        });
        let dismissed = cx.listener(|view, _: &(), _, cx| {
            view.dismissed = true;
            cx.notify();
        });
        modal_overlay(
            ElementId::Name("behavior-overlay".into()),
            "Behavior dialog",
            sensor(
                ElementId::Name("behavior-sensor".into()),
                resize_handle(
                    ElementId::Name("behavior-resize".into()),
                    div().child("base"),
                )
                .on_drag(dragged),
            )
            .on_show(measured)
            .size_full(),
            div().child("modal"),
        )
        .flex()
        .items_center()
        .justify_center()
        .on_dismiss(dismissed)
    }
}

#[test]
fn behavior_elements_lower_typed_routes_and_children() {
    let mut cx = TestAppContext::new();
    let view = cx.open::<BehaviorView>();
    let full = crate::StyleRefinement::default().size_full();
    let Some(wire::Node::Sensor {
        on_show: Some(_),
        style,
        ..
    }) = cx.find("behavior-sensor")
    else {
        panic!("behavior sensor")
    };
    assert_eq!(style.size.width, full.size.width);
    assert_eq!(style.size.height, full.size.height);
    assert!(matches!(
        cx.find("behavior-resize"),
        Some(wire::Node::ResizeHandle {
            on_drag: Some(_),
            ..
        })
    ));
    assert!(matches!(
        cx.find("behavior-overlay"),
        Some(wire::Node::Overlay {
            label: Some(label),
            on_dismiss: Some(_),
            children,
            ..
        }) if label == "Behavior dialog" && children.len() == 2
    ));
    cx.simulate_measure("behavior-sensor", 321., 123.);
    cx.simulate_drag("behavior-resize", 12., -3.);
    cx.simulate_dismiss("behavior-overlay");
    view.read(|view| {
        assert_eq!(view.measured, (321., 123.));
        assert_eq!(view.dragged, (12., -3.));
        assert!(view.dismissed);
    });
}

#[test]
fn sensor_style_is_opt_in() {
    let mut cx = TestAppContext::new();
    cx.open::<DefaultSensorView>();
    let Some(wire::Node::Sensor { style, .. }) = cx.find("default-sensor") else {
        panic!("default sensor")
    };
    assert_eq!(*style, crate::StyleRefinement::default());
}

#[test]
fn a_resize_handle_carries_role_name_focus_and_keys_to_the_wire() {
    let mut app = crate::App::for_driver();
    let mut window = app.window();
    let handle = resize_handle("pane-resize", div())
        .role(Role::Splitter)
        .aria_label("Resize the pane")
        .focusable()
        .on_key_down(|_, _, _| {});
    let node = crate::Lowering::new(&mut window, &mut app).lower(handle);
    let wire::Node::ResizeHandle {
        id, interactivity, ..
    } = node
    else {
        panic!("a resize handle")
    };
    assert_eq!(id, wire::ElementIdWire::Name("pane-resize".into()));
    assert_eq!(interactivity.role, Some(Role::Splitter));
    assert_eq!(interactivity.aria.label.as_deref(), Some("Resize the pane"));
    assert!(interactivity.focusable);
    assert!(interactivity.on_key_down.is_some());
}
