//! Guest-owned composer projection and its GPUI presentation.

use super::{Draft, MentionChoice};
use crate::context::Callback;
use crate::prelude::*;
use crate::{
    App, EditorDocumentUpdate, EditorElement, EditorElementEvent, EditorTransaction, View, wire,
};
use std::rc::Rc;

mod editor;
use editor::{editor, matching_choices};

#[derive(Clone, Debug)]
pub struct Change {
    pub before: String,
    pub after: String,
    pub cursor: wire::EditorCursor,
    pub tag: String,
}

pub enum Event<V> {
    Document(EditorDocumentUpdate),
    Transaction(EditorTransaction<V>),
    Committed(Change),
    Action(String),
}

impl<V> Clone for Event<V> {
    fn clone(&self) -> Self {
        match self {
            Self::Document(update) => Self::Document(update.clone()),
            Self::Transaction(transaction) => Self::Transaction(transaction.clone()),
            Self::Committed(change) => Self::Committed(change.clone()),
            Self::Action(action) => Self::Action(action.clone()),
        }
    }
}

pub enum Outcome<V> {
    Updated,
    Run(Callback<V>),
    Action(String),
    Enqueue(String),
}

pub type Handle<V> = Rc<dyn Fn(&mut V, Event<V>, &mut Window, &mut Context<V>)>;
/// A press on a composer control.
pub type Click = Box<dyn Fn(&ClickEvent, &mut Window, &mut App) + 'static>;

impl Draft {
    pub fn handle<V: 'static>(
        &mut self,
        event: Event<V>,
        choices: &[MentionChoice],
        cx: &mut App,
    ) -> Outcome<V> {
        match event {
            Event::Document(update) => {
                update.apply(&mut self.editor, cx);
                Outcome::Updated
            }
            Event::Transaction(transaction) => transaction
                .apply(&mut self.editor, cx)
                .map_or(Outcome::Updated, Outcome::Run),
            Event::Committed(change) => {
                self.committed(
                    &change.before,
                    &change.after,
                    change.cursor,
                    &change.tag,
                    choices,
                );
                match change.tag.as_str() {
                    "send" | "paste" | "copy" | "cut" | "restore" => Outcome::Action(change.tag),
                    _ => Outcome::Updated,
                }
            }
            Event::Action(tag) => Outcome::Enqueue(tag),
        }
    }
}

/// Dresses a mark's sign as what it does: bold, italic, code, quote.
type Face = fn(crate::Div) -> crate::Div;

#[derive(IntoElement)]
struct Mark {
    id: ElementId,
    sign: SharedString,
    label: SharedString,
    face: Face,
    on_click: Option<Click>,
}

impl RenderOnce for Mark {
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        let theme = *cx.global::<Theme>();
        let sign = (self.face)(div()).child(self.sign);
        let mut mark = div()
            .id(self.id)
            .role(Role::Button)
            .aria_label(self.label)
            .aria_disabled(self.on_click.is_none())
            .flex()
            .items_center()
            .justify_center()
            .size(crate::design::size::CONTROL)
            .text_size(crate::design::text::BODY)
            .text_color(theme.muted)
            .child(sign);
        if let Some(on_click) = self.on_click {
            mark = mark
                .hover(move |style| style.bg(theme.surface_raised).text_color(theme.foreground))
                .focusable()
                .on_click(on_click);
        }
        mark
    }
}

#[derive(IntoElement)]
struct ActionButton {
    id: ElementId,
    label: SharedString,
    primary: bool,
    on_click: Option<Click>,
}

impl RenderOnce for ActionButton {
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        let theme = *cx.global::<Theme>();
        // A primary button that can't act yet reads as resting, not as ink.
        let (background, foreground, border) = match (self.primary, self.on_click.is_some()) {
            (true, true) => (theme.primary, theme.primary_foreground, theme.primary),
            (true, false) => (theme.surface_raised, theme.muted, theme.surface_raised),
            (false, _) => (theme.surface, theme.foreground, theme.border),
        };
        let mut button = div()
            .id(self.id)
            .role(Role::Button)
            .aria_label(self.label.clone())
            .aria_disabled(self.on_click.is_none())
            .flex()
            .items_center()
            .justify_center()
            .h(crate::design::size::CONTROL)
            .px_2()
            .border_1()
            .border_color(border)
            .bg(background)
            .text_color(foreground)
            .text_size(crate::design::text::SECONDARY)
            .child(self.label);
        if let Some(on_click) = self.on_click {
            button = button.focusable().on_click(on_click);
            if self.primary {
                button = crate::design::focus_shown_on_ink(button, &theme);
            }
        }
        button
    }
}

#[derive(IntoElement)]
struct MentionItem {
    id: ElementId,
    label: SharedString,
    selected: bool,
    on_click: Option<Click>,
}

impl RenderOnce for MentionItem {
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        let theme = *cx.global::<Theme>();
        let mut row = div()
            .id(self.id)
            .role(Role::MenuItem)
            .aria_label(self.label.clone())
            .aria_selected(self.selected)
            .w_full()
            .flex()
            .items_center()
            .min_h(crate::design::size::ROW)
            .px_2()
            .bg(if self.selected {
                theme.accent_soft
            } else {
                theme.background
            })
            .text_color(if self.selected {
                theme.accent_foreground
            } else {
                theme.foreground
            })
            .text_size(crate::design::text::BODY)
            .child(self.label);
        if let Some(on_click) = self.on_click {
            row = row.on_click(on_click);
        }
        row
    }
}

/// A press that acts on the draft. A pointer press focuses the control it
/// lands on, so the press hands the keys back to `editor` first: the
/// typing goes on where it was.
fn press<V: View + 'static>(
    editable: bool,
    tag: String,
    editor: &str,
    handle: &Handle<V>,
    cx: &Context<V>,
) -> Option<Click> {
    if !editable {
        return None;
    }
    let handle = handle.clone();
    let editor = ElementId::Name(editor.to_owned().into());
    Some(Box::new(cx.listener(
        move |view, _: &ClickEvent, window, cx| {
            window.focus(editor.clone());
            handle(view, Event::Action(tag.clone()), window, cx);
            cx.notify();
        },
    )))
}

#[allow(clippy::too_many_arguments)]
pub fn view<V: View + 'static, F: Fn(&mut V, Event<V>, &mut Window, &mut Context<V>) + 'static>(
    draft: &Draft,
    key: &str,
    // what the field is, for assistive technology ("New message"); the
    // hint is what to write in it ("Message #general") and is drawn
    label: &str,
    hint: &str,
    // what the commit button says: "Send" for a new message, "Save" for an
    // edit; the composer does not guess from the draft
    commit: &str,
    // a way out beside the commit, as an edit's Cancel beside its Save;
    // `None` for a composer that stays open
    cancel: Option<Click>,
    editable: bool,
    choices: &[MentionChoice],
    cx: &mut Context<V>,
    handle: F,
) -> impl IntoElement + use<V, F> {
    let handle: Handle<V> = Rc::new(handle);
    let editor_id = format!("{key}/editor");
    let editor = editor(
        draft,
        &editor_id,
        key,
        label,
        hint,
        editable,
        choices,
        handle.clone(),
    );
    let mut rows: Vec<AnyElement> = Vec::new();

    if let Some((_, partial)) = draft.query(draft.editor.state_view()) {
        let matches = matching_choices(choices, &partial);
        let selected = draft.menu_index.min(matches.len().saturating_sub(1));
        let menu = matches
            .into_iter()
            .enumerate()
            .map(|(index, choice)| MentionItem {
                id: ElementId::Name(format!("{key}/mention/{}", choice.token).into()),
                label: format!("@{}", choice.label).into(),
                selected: index == selected,
                on_click: press(
                    editable,
                    format!("mention:{}", choice.token),
                    &editor_id,
                    &handle,
                    cx,
                ),
            })
            .collect::<Vec<_>>();
        if !menu.is_empty() {
            rows.push(
                div()
                    .flex()
                    .flex_col()
                    .gap_1()
                    .children(menu)
                    .into_any_element(),
            );
        }
    }
    rows.push(editor.into_any_element());

    if !draft.note.is_empty() {
        rows.push(
            div()
                .mx(crate::design::space::MD)
                .text_sm()
                .text_color(cx.global::<Theme>().danger)
                .child(draft.note.clone())
                .into_any_element(),
        );
    }
    if draft.failed_send.is_some() {
        rows.push(
            div()
                .mx(crate::design::space::MD)
                .flex()
                .items_center()
                .gap_2()
                .p_2()
                .bg(cx.global::<Theme>().danger_soft)
                .text_color(cx.global::<Theme>().danger)
                .child(div().flex_1().child("An earlier message wasn’t sent"))
                .child(ActionButton {
                    id: ElementId::Name(format!("{key}/restore").into()),
                    label: "Restore".into(),
                    primary: false,
                    on_click: press(editable, "restore".into(), &editor_id, &handle, cx),
                })
                .into_any_element(),
        );
    }

    let mut toolbar = div()
        .mx(crate::design::space::XXS)
        .flex()
        .items_center()
        .gap(px(2.));
    let faces: [(&str, &str, &str, Face); 4] = [
        ("B", "Bold", "bold", |sign| {
            sign.font_weight(crate::FontWeight::BOLD)
        }),
        ("I", "Italic", "italic", |sign| sign.italic()),
        ("</>", "Code", "code", |sign| {
            sign.font_family(design::fonts::FAMILY_MONO)
                .text_size(crate::design::text::CAPTION)
        }),
        ("“", "Quote", "quote", |sign| sign.text_size(px(16.))),
    ];
    for (sign, label, tag, face) in faces {
        toolbar = toolbar.child(Mark {
            id: ElementId::Name(format!("{key}/{tag}").into()),
            sign: sign.into(),
            label: label.into(),
            face,
            on_click: press(editable, tag.into(), &editor_id, &handle, cx),
        });
    }
    let sendable = editable && draft.can_send(draft.editor.state_view().text);
    toolbar = toolbar.child(div().flex_1());
    if let Some(cancel) = cancel {
        toolbar = toolbar.child(ActionButton {
            id: ElementId::Name(format!("{key}/cancel").into()),
            label: "Cancel".into(),
            primary: false,
            on_click: Some(cancel),
        });
    }
    toolbar = toolbar.child(ActionButton {
        id: ElementId::Name(format!("{key}/send").into()),
        label: commit.to_owned().into(),
        primary: true,
        on_click: press(sendable, "send".into(), &editor_id, &handle, cx),
    });
    rows.push(toolbar.into_any_element());

    div()
        .id(ElementId::Name(key.into()))
        .flex()
        .flex_col()
        .gap_1()
        .border_1()
        .border_color(cx.global::<Theme>().border)
        .bg(cx.global::<Theme>().background)
        .pb(crate::design::space::XXS)
        .children(rows)
}

#[cfg(test)]
mod tests;
