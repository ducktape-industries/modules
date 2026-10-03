use std::sync::Arc;

use serde::{Deserialize, Serialize};
use view_guest::prelude::*;
use view_guest::testing::TestAppContext;
use view_guest::{ElementId, Input, View, Window, wire};

#[derive(Default, Serialize, Deserialize)]
struct Form {
    value: String,
    submits: usize,
}

impl View for Form {
    const NAME: &'static str = "Form";
    fn new(_: &mut Window, _: &mut Context<Self>) -> Self {
        Self::default()
    }
}

impl Render for Form {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let id = ElementId::NamedChild(Arc::new(ElementId::Name("chat".into())), "search".into());
        div().child(
            Input::new(id, "Search messages")
                .value(self.value.clone())
                .placeholder("Search")
                .w(px(240.))
                .on_input(cx.listener(|view, text: &String, _, cx| {
                    view.value = text.clone();
                    cx.notify();
                }))
                .on_submit(cx.listener(|view, _: &(), _, cx| {
                    view.submits += 1;
                    cx.notify();
                })),
        )
    }
}

type Lowered<'a> = (
    &'a wire::ElementIdWire,
    u32,
    u32,
    &'a gpui::StyleRefinement,
    &'a str,
);

fn input(cx: &TestAppContext) -> Lowered<'_> {
    let wire::Node::Container(view_guest::wire::ContainerNode { children, .. }) = cx.root() else {
        panic!("root container")
    };
    let wire::Node::Input {
        id,
        on_input: Some(on_input),
        on_submit: Some(on_submit),
        style,
        options,
        ..
    } = &children[0]
    else {
        panic!("input child")
    };
    (id, *on_input, *on_submit, style, &options.label)
}

#[test]
fn input_lowers_typed_identity_style_and_frame_owned_callbacks() {
    let mut first = TestAppContext::new();
    let mut second = TestAppContext::new();
    let first_form = first.open::<Form>();
    let second_form = second.open::<Form>();
    let (id, first_input, first_submit, style, label) = input(&first);
    let (second_id, second_input, second_submit, _, _) = input(&second);
    assert_eq!(label, "Search messages");
    assert_eq!(id, second_id);
    assert_eq!(first_input, second_input);
    assert_eq!(first_submit, second_submit);
    assert_eq!(
        id,
        &wire::ElementIdWire::NamedChild {
            base: wire::ElementIdAtom::Name("chat".into()),
            names: vec!["search".into()],
        }
    );
    assert_eq!(style, &gpui::StyleRefinement::default().w(px(240.)));

    first.simulate_input("Search", "hello");
    first.simulate_submit("Search");
    first_form.read(|form| {
        assert_eq!(form.value, "hello");
        assert_eq!(form.submits, 1);
    });
    second_form.read(|form| {
        assert_eq!(form.value, "");
        assert_eq!(form.submits, 0);
    });

    first.simulate_event(wire::Event::Input {
        handler: second_input,
        text: "stale".into(),
    });
    first_form.read(|form| assert_eq!(form.value, "stale"));
}
