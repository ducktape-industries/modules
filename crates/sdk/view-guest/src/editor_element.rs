//! Host-native multiline editor recipe.

use crate::context::Callback;
use crate::{
    Editor, EditorBinding, EditorDocumentUpdate, EditorTransaction, Element, IntoElement, Lowering,
    wire,
};
use gpui::{ElementId, StyleRefinement, Styled};
use std::rc::Rc;

/// A driver-routed event produced by an [`EditorElement`].
pub enum EditorElementEvent<P, V> {
    Document(EditorDocumentUpdate),
    Observed(P),
    Transaction(EditorTransaction<V>),
}

/// A multiline host editor bound to guest-owned [`Editor`] state.
///
/// GPUI core has no editor widget, so lowering emits the host primitive while
/// preserving GPUI identity and style values.
pub struct EditorElement<P, V> {
    id: ElementId,
    editor: Editor,
    document: String,
    binding: EditorBinding<P>,
    route: Rc<dyn Fn(EditorElementEvent<P, V>) -> Callback<V>>,
    placeholder: String,
    label: Option<String>,
    editable: bool,
    style: StyleRefinement,
    presentation: Option<Box<wire::editor_presentation::EditorPresentation>>,
}

impl<P: 'static, V: 'static> EditorElement<P, V> {
    pub fn new(
        id: impl Into<ElementId>,
        editor: &Editor,
        document: impl Into<String>,
        binding: EditorBinding<P>,
        route: impl Fn(EditorElementEvent<P, V>) -> Callback<V> + 'static,
    ) -> Self {
        Self {
            id: id.into(),
            editor: editor.clone(),
            document: document.into(),
            binding,
            route: Rc::new(route),
            placeholder: String::new(),
            label: None,
            editable: true,
            style: StyleRefinement::default(),
            presentation: None,
        }
    }

    pub fn placeholder(mut self, value: impl Into<String>) -> Self {
        self.placeholder = value.into();
        self
    }

    pub fn label(mut self, value: impl Into<String>) -> Self {
        self.label = Some(value.into());
        self
    }

    pub fn editable(mut self, editable: bool) -> Self {
        self.editable = editable;
        self
    }

    pub fn presentation(mut self, value: wire::editor_presentation::EditorPresentation) -> Self {
        self.presentation = Some(Box::new(value));
        self
    }
}

impl<V: 'static> EditorElement<(), V> {
    /// A plain multi-line field over one [`Editor`] the view owns: `field`
    /// finds it (`None` once the draft is gone), and every document update
    /// and transaction the host sends lands on it.
    pub fn plain(
        id: impl Into<ElementId>,
        editor: &Editor,
        document: impl Into<String>,
        field: fn(&mut V) -> Option<&mut Editor>,
    ) -> Self {
        Self::new(
            id,
            editor,
            document,
            EditorBinding::plain(),
            move |event| -> Callback<V> {
                match event {
                    EditorElementEvent::Document(update) => Rc::new(move |view, _, cx| {
                        if let Some(editor) = field(view) {
                            update.clone().apply(editor, cx);
                            cx.notify();
                        }
                    }),
                    EditorElementEvent::Observed(()) => Rc::new(|_, _, _| {}),
                    EditorElementEvent::Transaction(transaction) => {
                        Rc::new(move |view, window, cx| {
                            let Some(editor) = field(view) else {
                                return;
                            };
                            let then = transaction.clone().apply(editor, cx);
                            if let Some(then) = then {
                                then(view, window, cx);
                            }
                            cx.notify();
                        })
                    }
                }
            },
        )
    }
}

impl<P, V> Styled for EditorElement<P, V> {
    fn style(&mut self) -> &mut StyleRefinement {
        &mut self.style
    }
}

impl<P: 'static, V: 'static> Element for EditorElement<P, V> {
    fn id(&self) -> Option<ElementId> {
        Some(self.id.clone())
    }

    fn lower(self: Box<Self>, lowering: &mut Lowering<'_>) -> wire::Node {
        let Self {
            id,
            editor,
            document,
            binding,
            route,
            placeholder,
            label,
            editable,
            style,
            presentation,
        } = *self;
        let id = crate::element::wire_id(id);
        let context = &lowering.app().inner.slots;
        let document_route = route.clone();
        let (document, on_document) = editor.document(context, document, move |update| {
            document_route(EditorElementEvent::Document(update))
        });
        let observed_route = route.clone();
        let transaction_route = route;
        let binding = binding.register(
            context,
            move |value| observed_route(EditorElementEvent::Observed(value)),
            move |transaction| transaction_route(EditorElementEvent::Transaction(transaction)),
        );
        wire::Node::Editor {
            options: Box::new(wire::EditorOptions {
                rich: None,
                binding: Some(Box::new(binding)),
                presentation,
            }),
            id,
            style,
            placeholder,
            label,
            document,
            on_document,
            editable,
        }
    }
}

impl<P: 'static, V: 'static> IntoElement for EditorElement<P, V> {
    type Element = Self;

    fn into_element(self) -> Self {
        self
    }
}

impl<P: 'static, V: 'static> gpui::prelude::FluentBuilder for EditorElement<P, V> {}
