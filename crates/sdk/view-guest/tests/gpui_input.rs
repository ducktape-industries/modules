use std::sync::Arc;

use serde::{Deserialize, Serialize};
use view_guest::prelude::*;
use view_guest::testing::TestAppContext;
use view_guest::{ElementId, Input, TextField, View, Window, wire};

#[derive(Default, Serialize, Deserialize)]
struct Form {
    value: TextField,
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
                .value(&self.value)
                .placeholder("Search")
                .w(px(240.))
                .on_change(cx.listener(|view, change: &wire::TextChange, _, cx| {
                    view.value.apply(change);
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
    u64,
);

fn input(cx: &TestAppContext) -> Lowered<'_> {
    let wire::Node::Container(view_guest::wire::ContainerNode { children, .. }) = cx.root() else {
        panic!("root container")
    };
    let wire::Node::Field {
        id,
        on_change: Some(on_change),
        on_submit: Some(on_submit),
        style,
        options,
        generation,
        ..
    } = &children[0]
    else {
        panic!("input child")
    };
    (
        id,
        *on_change,
        *on_submit,
        style,
        &options.label,
        *generation,
    )
}

#[test]
fn input_lowers_typed_identity_style_and_frame_owned_callbacks() {
    let mut first = TestAppContext::new();
    let mut second = TestAppContext::new();
    let first_form = first.open::<Form>();
    let second_form = second.open::<Form>();
    let (id, first_input, first_submit, style, label, first_generation) = input(&first);
    let (second_id, second_input, second_submit, _, _, _) = input(&second);
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
        assert_eq!(form.value.text, "hello");
        assert_eq!(form.submits, 1);
    });
    second_form.read(|form| {
        assert_eq!(form.value.text, "");
        assert_eq!(form.submits, 0);
    });

    first.simulate_event(wire::Event::Text {
        handler: second_input,
        change: wire::TextChange {
            generation: first_generation,
            revision: 9,
            edit: Some(wire::Edit {
                range: wire::TextRange::caret(0),
                len: 5,
            }),
            text: "stale".into(),
            cursor: wire::TextRange::caret(5),
            preedit: None,
            tokens: Vec::new(),
        },
    });
    first_form.read(|form| {
        assert_eq!(
            (form.value.text.as_str(), form.value.revision),
            ("stale", 9)
        )
    });
}
