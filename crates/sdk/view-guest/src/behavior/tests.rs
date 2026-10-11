use crate::prelude::*;
use crate::testing::TestAppContext;
use crate::{View, modal_overlay, resize_handle, sensor, wire};

#[derive(Default, serde::Deserialize, serde::Serialize)]
struct BehaviorView {
    /// The sensor's bounds as last heard (x, y, width, height), and how
    /// many times it heard them.
    measured: [f32; 4],
    heard: u32,
    /// The sensor is out of the tree.
    gone: bool,
    dragged: (f32, f32),
    dismissed: bool,
}

impl View for BehaviorView {
    const NAME: &'static str = "BehaviorView";
    fn new(_: &mut Window, _: &mut Context<Self>) -> Self {
        Self::default()
    }
}

impl Render for BehaviorView {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let measured = cx.listener(|view, bounds: &Bounds<Pixels>, _, cx| {
            let (origin, size) = (bounds.origin, bounds.size);
            view.measured = [origin.x, origin.y, size.width, size.height].map(f32::from);
            view.heard += 1;
            cx.notify();
        });
        if self.gone {
            return div().into_any_element();
        }
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
                .role(Role::Splitter)
                .aria_label("Resize the base")
                .focusable()
                .on_key_down(|_, _, _| {})
                .on_drag(dragged),
            )
            .on_bounds(measured)
            .size_full(),
            Some(div().child("modal")),
        )
        .flex()
        .items_center()
        .justify_center()
        .on_dismiss(dismissed)
        .into_any_element()
    }
}

#[test]
fn behavior_elements_lower_typed_routes_and_children() {
    let mut cx = TestAppContext::new();
    let view = cx.open::<BehaviorView>();
    let full = crate::StyleRefinement::default().size_full();
    let Some(wire::Node::Sensor {
        on_bounds: Some(_),
        style,
        ..
    }) = cx.find("behavior-sensor")
    else {
        panic!("behavior sensor")
    };
    assert_eq!(cx.styles()[*style].size.width, full.size.width);
    assert_eq!(cx.styles()[*style].size.height, full.size.height);
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
    cx.simulate_bounds("behavior-sensor", (10., 20.), (321., 123.));
    cx.simulate_drag("behavior-resize", 12., -3.);
    cx.simulate_dismiss("behavior-overlay");
    view.read(|view| {
        assert_eq!(view.measured, [10., 20., 321., 123.]);
        assert_eq!(view.dragged, (12., -3.));
        assert!(view.dismissed);
    });
}

/// What the host tells a sensor, and when: its child's bounds in the
/// window's pixels, on the first sight of it and whenever the origin or the
/// size differs from the last told. The same bounds again are no event:
/// the view does not tick.
#[test]
fn a_sensor_hears_its_bounds_at_first_sight_and_when_they_differ() {
    let mut cx = TestAppContext::new();
    let view = cx.open::<BehaviorView>();
    let heard = |view: &crate::Entity<BehaviorView>| view.read(|view| (view.heard, view.measured));
    assert_eq!(heard(&view).0, 0, "no bounds before the host lays it out");
    cx.simulate_bounds("behavior-sensor", (10., 20.), (300., 200.));
    assert_eq!(heard(&view), (1, [10., 20., 300., 200.]), "the first sight");
    let ticks = cx.ticks();
    cx.simulate_bounds("behavior-sensor", (10., 20.), (300., 200.));
    assert_eq!(heard(&view).0, 1, "nothing changed");
    assert_eq!(cx.ticks(), ticks, "and nothing ticked");
    cx.simulate_bounds("behavior-sensor", (40., 20.), (300., 200.));
    assert_eq!(
        heard(&view),
        (2, [40., 20., 300., 200.]),
        "moved, same size"
    );
    cx.simulate_bounds("behavior-sensor", (40., 20.), (300., 260.));
    assert_eq!(
        heard(&view),
        (3, [40., 20., 300., 260.]),
        "resized in place"
    );
}

/// A sensor that left the tree is forgotten: back in it, it hears the
/// bounds it had before, since the view may have dropped them with it.
#[test]
fn a_sensor_back_in_the_tree_hears_its_bounds_again() {
    let mut cx = TestAppContext::new();
    let view = cx.open::<BehaviorView>();
    let show = |cx: &mut TestAppContext, gone: bool| {
        cx.update(&view, |view, _, cx| {
            view.gone = gone;
            cx.notify();
        });
        cx.run_until_parked();
    };
    cx.simulate_bounds("behavior-sensor", (10., 20.), (300., 200.));
    show(&mut cx, true);
    assert!(cx.find("behavior-sensor").is_none());
    show(&mut cx, false);
    cx.simulate_bounds("behavior-sensor", (10., 20.), (300., 200.));
    assert_eq!(view.read(|view| view.heard), 2);
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
        id,
        interactivity: Some(interactivity),
        ..
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

/// The base keeps its path whether or not a modal is open over it: the
/// host keys a list's scroll and a field's state by it, and a modal that
/// moved the base started the list over and dropped the field.
#[test]
fn a_modal_opening_leaves_its_base_where_it_was() {
    fn list_path(node: &wire::Node) -> Option<Vec<wire::ElementIdWire>> {
        if let wire::Node::UniformList { path, .. } = node {
            return Some(path.clone());
        }
        node.children().iter().find_map(list_path)
    }
    let lower = |open: bool| {
        let mut app = crate::App::for_driver();
        let mut window = app.window();
        let base = div()
            .id("base")
            .child(crate::uniform_list("rows", 1, |_, _, _| vec![div()]));
        let overlay = modal_overlay("overlay", "Dialog", base, open.then(div))
            .p_6()
            .backdrop(gpui::hsla(0., 0., 0., 0.5));
        crate::Lowering::new(&mut window, &mut app).lower(overlay)
    };
    let (closed, open) = (lower(false), lower(true));
    assert!(
        matches!(open, wire::Node::Overlay { .. }),
        "open: an overlay"
    );
    assert_eq!(closed.key(), Some("overlay"), "closed: under the same id");
    assert!(
        !matches!(closed, wire::Node::Overlay { .. }),
        "closed: no modal layer"
    );
    let path = list_path(&closed).expect("the base's list");
    assert_eq!(list_path(&open), Some(path));
}

/// An open modal is a dialog, and a dialog has a name: the host draws an
/// unnamed one as no dialog at all, so the view stops where it is built.
#[test]
#[should_panic(expected = "a dialog has a name: its label is empty")]
fn an_open_modal_overlay_needs_a_label() {
    let _ = modal_overlay("dialog", "", div(), Some(div()));
}

#[test]
fn a_closed_modal_overlay_needs_no_label() {
    let _ = modal_overlay("dialog", "", div(), None::<crate::Div>);
}

#[derive(Default, serde::Deserialize, serde::Serialize)]
struct WebView {
    #[serde(skip)]
    nav: WebHandle,
    heard: Vec<String>,
}

impl View for WebView {
    const NAME: &'static str = "WebView";
    fn new(_: &mut Window, _: &mut Context<Self>) -> Self {
        Self::default()
    }
}

impl Render for WebView {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        web("page", "https://example.com/")
            .track(&self.nav)
            .on_state(cx.listener(|view, state: &WebState, _, cx| {
                view.heard.push(state.url.clone());
                cx.notify();
            }))
            .size_full()
    }
}

/// A web piece carries its first address and a state route; a navigation
/// crosses once, under a revision the host has not seen, and the page's
/// state reaches the view's listener.
#[test]
fn a_web_piece_carries_its_navigations_once_and_hears_its_page() {
    let mut cx = TestAppContext::new();
    let view = cx.open::<WebView>();
    let wire::Node::Web {
        url,
        revision,
        commands,
        on_state: Some(_),
        ..
    } = cx.node("page").clone()
    else {
        panic!("no web piece with a state route: {:?}", cx.root());
    };
    assert_eq!(url, "https://example.com/");
    assert!(commands.is_empty());
    cx.update(&view, |view, _, cx| {
        view.nav.load("https://example.org/");
        view.nav.back();
        cx.notify();
    });
    cx.run_until_parked();
    let wire::Node::Web {
        revision: moved,
        commands,
        ..
    } = cx.node("page").clone()
    else {
        unreachable!()
    };
    assert_eq!(moved, revision + 2);
    assert_eq!(
        commands,
        vec![
            wire::WebCommand::Load("https://example.org/".into()),
            wire::WebCommand::Back
        ]
    );
    cx.simulate_web_state(
        "page",
        WebState {
            url: "https://example.org/a".into(),
            ..Default::default()
        },
    );
    // the commands went with the frame that carried them
    let wire::Node::Web {
        revision: kept,
        commands,
        ..
    } = cx.node("page").clone()
    else {
        unreachable!()
    };
    assert_eq!(kept, moved);
    assert!(commands.is_empty());
    cx.update(&view, |view, _, _| {
        assert_eq!(view.heard, ["https://example.org/a"]);
    });
}
