use crate::accesskit::{Action, ActionData, AriaCurrent, HasPopup, Invalid, Live};
use crate::prelude::*;
use crate::{Driver, Lowering, View, wire};

fn lower(element: impl IntoElement) -> wire::Node {
    let mut app = App::for_driver();
    let mut window = app.window();
    Lowering::new(&mut window, &mut app).lower(element)
}

fn aria(node: &wire::Node) -> &wire::Aria {
    match node {
        wire::Node::Container(wire::ContainerNode { interactivity, .. })
        | wire::Node::Svg { interactivity, .. } => &interactivity.aria,
        other => panic!("no interactivity: {other:?}"),
    }
}

fn named(names: &[&'static str]) -> Vec<wire::ElementIdWire> {
    names
        .iter()
        .map(|name| wire::ElementIdWire::Name((*name).into()))
        .collect()
}

#[test]
fn each_phase_two_setter_lowers_into_its_one_wire_field() {
    let node = lower(
        div()
            .id("field")
            .aria_live(Live::Polite)
            .aria_busy(true)
            .aria_required(true)
            .aria_invalid(Invalid::True)
            .aria_read_only(true)
            .aria_has_popup(HasPopup::Listbox)
            .aria_current(AriaCurrent::Page)
            .custom_action(3, "Pin"),
    );
    assert_eq!(
        *aria(&node),
        wire::Aria {
            live: Some(Live::Polite),
            busy: true,
            required: true,
            invalid: Some(Invalid::True),
            read_only: true,
            has_popup: Some(HasPopup::Listbox),
            current: Some(AriaCurrent::Page),
            custom_actions: vec![(3, "Pin".into())],
            ..Default::default()
        }
    );
}

#[test]
fn a_relation_names_its_sibling_by_the_path_from_the_root() {
    let node = lower(
        div()
            .id("form")
            .child(
                div()
                    .id("field")
                    .aria_labelled_by("caption")
                    .aria_labelled_by("unit")
                    .aria_described_by("hint")
                    .aria_controls("list")
                    .aria_error_message("error"),
            )
            .child(crate::svg().aria_labelled_by("caption")),
    );
    let children = node.children();
    let field = aria(&children[0]);
    assert_eq!(
        field.labelled_by,
        [named(&["form", "caption"]), named(&["form", "unit"])]
    );
    assert_eq!(field.described_by, [named(&["form", "hint"])]);
    assert_eq!(field.controls, [named(&["form", "list"])]);
    assert_eq!(field.error_message, Some(named(&["form", "error"])));
    // an element without an id is in its parent's scope already
    assert_eq!(
        aria(&children[1]).labelled_by,
        [named(&["form", "caption"])]
    );
}

#[derive(Default, serde::Serialize, serde::Deserialize)]
struct Stepper {
    count: i32,
    data: Option<ActionData>,
}

impl View for Stepper {
    fn new(_: &mut Window, _: &mut Context<Self>) -> Self {
        Self::default()
    }
}

impl Render for Stepper {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .id("stepper")
            .role(Role::SpinButton)
            .aria_label("Count")
            .aria_numeric_value(f64::from(self.count))
            .on_a11y_action(Action::Increment, {
                let step = cx.listener(|view: &mut Self, data: &Option<ActionData>, _, cx| {
                    view.count += 1;
                    view.data = data.clone();
                    cx.notify();
                });
                // gpui's shape: FnMut, handed the data by reference
                let mut asked = Vec::new();
                move |data: Option<&ActionData>, window: &mut Window, app: &mut App| {
                    asked.push(data.cloned());
                    step(asked.last().expect("just asked"), window, app)
                }
            })
    }
}

#[test]
fn an_a11y_action_reaches_the_listener_its_route_names_with_its_data() {
    let mut driver = Driver::<Stepper>::new();
    let frame = driver.tick(Vec::new());
    let root = frame.root.expect("a tree");
    let &[(Action::Increment, handler)] = aria(&root).actions.as_slice() else {
        panic!("one Increment route: {:?}", aria(&root).actions);
    };
    let data = Some(ActionData::Value("2".into()));
    driver.tick(vec![wire::Event::A11yAction {
        handler,
        data: data.clone(),
    }]);
    driver.entity().read(|view| {
        assert_eq!(view.count, 1);
        assert_eq!(view.data, data);
    });
}
