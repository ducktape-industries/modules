use serde::{Deserialize, Serialize};
use std::rc::Rc;
use view_guest::{
    Callback, Context, Driver, Editor, EditorBinding, EditorElement, ElementId, Render, Styled,
    View, Window, wire,
};

#[derive(Default, Serialize, Deserialize)]
struct EditorView {
    editor: Editor,
}

impl View for EditorView {
    const NAME: &'static str = "EditorView";
    fn new(_: &mut Window, _: &mut Context<Self>) -> Self {
        Self {
            editor: Editor::new("hello"),
        }
    }
}

impl Render for EditorView {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl view_guest::IntoElement {
        let binding = EditorBinding::new(
            Vec::new(),
            |_| wire::EditorDecision::DefaultEditorAction,
            |_| None::<()>,
        );
        EditorElement::new(
            ElementId::Name("draft".into()),
            &self.editor,
            "app:draft",
            binding,
            |_| -> Callback<Self> { Rc::new(|_, _, _| {}) },
            "Message",
        )
        .placeholder("Write a message")
        .w_full()
    }
}

#[test]
fn editor_element_lowers_identity_style_and_document_without_author_routes() {
    let mut driver = Driver::<EditorView>::new();
    let frame = driver.tick(Vec::new());
    let wire::Node::Editor {
        id,
        style,
        placeholder,
        label,
        document,
        binding,
        ..
    } = frame.root.unwrap()
    else {
        panic!("editor element must lower to the host editor primitive")
    };
    assert_eq!(id, wire::ElementIdWire::Name("draft".into()));
    assert!(style.size.width.is_some());
    assert_eq!(placeholder, "Write a message");
    assert_eq!(label.as_deref(), Some("Message"));
    assert_eq!(document.document, "app:draft");
    assert_eq!(document.byte_len, 5);
    assert!(binding.is_some());
}
