//! Guest-local decisions borrow the canonical document owned by application state.
use crate::context::Callback;
use crate::{Editor, slots, wire};
use std::rc::Rc;

pub use wire::EditorDecision;

#[derive(Clone, Copy, Debug)]
pub struct EditorStateView<'a> {
    pub text: &'a str,
    pub cursor: wire::EditorCursor,
    pub reset: u64,
    pub text_revision: u64,
    pub revision: u64,
}
impl<'a> EditorStateView<'a> {
    fn new(text: &'a str, reference: &wire::editor_document::EditorDocumentRef) -> Self {
        Self {
            text,
            cursor: reference.cursor,
            reset: reference.reset,
            text_revision: reference.text_revision,
            revision: reference.revision,
        }
    }
}
#[derive(Clone, Copy, Debug)]
pub struct EditorKeyRequest<'a> {
    pub id: &'a wire::EditorTransactionId,
    pub state: EditorStateView<'a>,
    pub key: &'a wire::keyboard::KeyState,
    pub repeat: bool,
    pub input_time_ms: u64,
}
#[derive(Clone, Copy, Debug)]
pub struct EditorInteractionRequest<'a> {
    pub id: &'a wire::EditorTransactionId,
    pub state: EditorStateView<'a>,
    pub action: &'a wire::editor_presentation::EditorInteraction,
    pub input_time_ms: u64,
}
#[derive(Clone, Copy, Debug)]
pub enum EditorTransactionEvent<'a> {
    Interaction {
        id: &'a wire::EditorTransactionId,
        state: EditorStateView<'a>,
        action: &'a wire::editor_presentation::EditorInteraction,
        input_time_ms: u64,
    },
    Commit {
        id: &'a wire::EditorTransactionId,
        origin: Option<&'a wire::EditorRequestInput>,
        before: EditorStateView<'a>,
        after: EditorStateView<'a>,
        kind: wire::EditorEditKind,
        history: wire::EditorHistoryEffect,
        input_time_ms: u64,
    },
    Fault {
        id: &'a wire::EditorTransactionId,
        reason: wire::EditorFault,
    },
    Cancelled {
        id: &'a wire::EditorTransactionId,
    },
}

type Decide = Rc<dyn for<'a> Fn(EditorKeyRequest<'a>) -> EditorDecision>;
type Interact = Rc<dyn for<'a> Fn(EditorInteractionRequest<'a>) -> EditorDecision>;
type Observe<P> = Rc<dyn for<'a> Fn(EditorTransactionEvent<'a>) -> Option<P>>;
pub struct EditorBinding<P> {
    claims: Vec<wire::EditorKeyClaim>,
    decide: Decide,
    interact: Option<Interact>,
    on_event: Observe<P>,
}
struct Callbacks<V> {
    decide: Decide,
    interact: Option<Interact>,
    on_event: Observe<Callback<V>>,
}
impl<P: 'static> EditorBinding<P> {
    pub fn new(
        claims: Vec<wire::EditorKeyClaim>,
        decide: impl for<'a> Fn(EditorKeyRequest<'a>) -> EditorDecision + 'static,
        on_event: impl for<'a> Fn(EditorTransactionEvent<'a>) -> Option<P> + 'static,
    ) -> Self {
        assert!(
            claims.len() <= wire::editor_transaction::MAX_EDITOR_CLAIMS,
            "editor claim limit"
        );
        Self {
            claims,
            decide: Rc::new(decide),
            interact: None,
            on_event: Rc::new(on_event),
        }
    }
    pub fn on_interaction(
        mut self,
        decide: impl for<'a> Fn(EditorInteractionRequest<'a>) -> EditorDecision + 'static,
    ) -> Self {
        self.interact = Some(Rc::new(decide));
        self
    }
    pub(crate) fn register<V: 'static>(
        self,
        context: &slots::Context,
        route: impl Fn(P) -> Callback<V> + 'static,
        wrap: impl Fn(EditorTransaction<V>) -> Callback<V> + 'static,
    ) -> wire::EditorBinding {
        let observe = self.on_event;
        let callbacks = Rc::new(Callbacks {
            decide: self.decide,
            interact: self.interact,
            on_event: Rc::new(move |event| observe(event).map(&route)),
        });
        // Existing handler storage already supplies bounded frame-local lifetime
        // and memo capture. No second callback registry or copied document.
        let map = slots::handler::<(), Rc<Callbacks<V>>>(
            context,
            Box::new(move |()| Some(callbacks.clone())),
        );
        let identity = context.identity();
        let wrap = Rc::new(wrap);
        let request_wrap = wrap.clone();
        let request_identity = identity.clone();
        let on_request = slots::handler::<wire::EditorRequest, Callback<V>>(
            context,
            Box::new(move |request| {
                Some(request_wrap(EditorTransaction {
                    event: Transaction::Request(request),
                    map,
                    identity: request_identity.clone(),
                    view: std::marker::PhantomData,
                }))
            }),
        );
        let on_event = slots::handler::<wire::EditorTransactionEvent, Callback<V>>(
            context,
            Box::new(move |event| {
                Some(wrap(EditorTransaction {
                    event: Transaction::Event(event),
                    map,
                    identity: identity.clone(),
                    view: std::marker::PhantomData,
                }))
            }),
        );
        wire::EditorBinding {
            claims: self.claims,
            on_request,
            on_event,
        }
    }
}
impl EditorBinding<()> {
    pub fn plain() -> Self {
        EditorBinding::new(
            Vec::new(),
            |_| EditorDecision::DefaultEditorAction,
            |_| None,
        )
    }
}
#[derive(Clone, Debug)]
enum Transaction {
    Request(wire::EditorRequest),
    Event(wire::EditorTransactionEvent),
}
pub struct EditorTransaction<V> {
    event: Transaction,
    map: u32,
    identity: std::sync::Weak<()>,
    view: std::marker::PhantomData<fn() -> V>,
}
impl<V> Clone for EditorTransaction<V> {
    fn clone(&self) -> Self {
        Self {
            event: self.event.clone(),
            map: self.map,
            identity: self.identity.clone(),
            view: std::marker::PhantomData,
        }
    }
}
impl<V> std::fmt::Debug for EditorTransaction<V> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_tuple("EditorTransaction")
            .field(&self.event)
            .finish()
    }
}
impl<V: 'static> EditorTransaction<V> {
    pub fn apply(self, editor: &mut Editor, cx: &mut crate::App) -> Option<Callback<V>> {
        self.apply_in(editor, &cx.inner.slots)
    }

    fn apply_in(self, editor: &mut Editor, context: &slots::Context) -> Option<Callback<V>> {
        if !context.owns(&self.identity) {
            return None;
        }
        let callbacks = slots::run_handler::<(), Rc<Callbacks<V>>>(context, self.map, ())?;
        match self.event {
            Transaction::Request(request) => {
                if !slots::editor_request_current(context, &request.id) {
                    return None;
                }
                if editor.document_reference(request.id.document.clone()) != request.state {
                    if request.state.reset == editor.reset_revision()
                        && let Err(reason) = slots::request_editor_mirror(context, &request)
                    {
                        slots::editor_document_failure(context, (&request.id).into(), reason);
                    }
                    return None;
                }
                let state = EditorStateView::new(editor.text_ref(), &request.state);
                let decision = match &request.input {
                    wire::EditorRequestInput::Key { key, repeat } => {
                        (callbacks.decide)(EditorKeyRequest {
                            id: &request.id,
                            state,
                            key,
                            repeat: *repeat,
                            input_time_ms: request.input_time_ms,
                        })
                    }
                    wire::EditorRequestInput::Interaction { action } => callbacks
                        .interact
                        .as_ref()
                        .map_or(EditorDecision::Noop, |decide| {
                            decide(EditorInteractionRequest {
                                id: &request.id,
                                state,
                                action,
                                input_time_ms: request.input_time_ms,
                            })
                        }),
                };
                slots::editor_response(
                    context,
                    wire::EditorResponse {
                        id: request.id,
                        decision,
                    },
                );
                None
            }
            Transaction::Event(event) => {
                let id = event.id();
                if !slots::editor_matches_pending(context, id) {
                    return None;
                }
                let mapped = match &event {
                    wire::EditorTransactionEvent::Interaction {
                        state,
                        action,
                        input_time_ms,
                        ..
                    } => {
                        if editor.document_reference(id.document.clone()) != *state {
                            return None;
                        }
                        (callbacks.on_event)(EditorTransactionEvent::Interaction {
                            id,
                            state: EditorStateView::new(editor.text_ref(), state),
                            action,
                            input_time_ms: *input_time_ms,
                        })
                    }
                    wire::EditorTransactionEvent::Commit {
                        origin,
                        before,
                        after,
                        patches,
                        kind,
                        history,
                        input_time_ms,
                        ..
                    } => {
                        if before.document != id.document
                            || before.reset != id.reset
                            || before.text_revision != id.text_revision
                            || before.revision != id.revision
                        {
                            return None;
                        }
                        let old = editor.accept_patch(before, after, patches)?;
                        (callbacks.on_event)(EditorTransactionEvent::Commit {
                            id,
                            origin: origin.as_ref(),
                            before: EditorStateView::new(
                                old.as_deref().unwrap_or_else(|| editor.text_ref()),
                                before,
                            ),
                            after: EditorStateView::new(editor.text_ref(), after),
                            kind: *kind,
                            history: *history,
                            input_time_ms: *input_time_ms,
                        })
                    }
                    wire::EditorTransactionEvent::Fault { state, reason, .. } => {
                        if state.document != id.document
                            || state.reset != id.reset
                            || state.reset != editor.reset_revision()
                        {
                            return None;
                        }
                        (callbacks.on_event)(EditorTransactionEvent::Fault {
                            id,
                            reason: *reason,
                        })
                    }
                    wire::EditorTransactionEvent::Cancelled { state, .. } => {
                        if state.document != id.document || state.reset != id.reset {
                            return None;
                        }
                        (callbacks.on_event)(EditorTransactionEvent::Cancelled { id })
                    }
                };
                slots::editor_acknowledge(context, &event);
                mapped
            }
        }
    }
}

#[cfg(test)]
mod tests;
