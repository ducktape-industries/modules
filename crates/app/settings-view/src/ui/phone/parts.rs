//! The pieces both phone files draw that are this view's own, beside the
//! SDK's kit (`design`). They draw only what they are handed; none reads
//! the state or writes. The ones marked "SDK gap" are the kit's still to
//! come.
use view::design::Ladder;
use view::prelude::*;
use view::{Div, ElementId, FontWeight, Stateful, Svg};

use crate::state::Form;
use crate::words;

/// A column of a pane or a sheet, its children edge to edge.
pub(super) fn column(id: impl Into<ElementId>) -> Stateful<Div> {
    div().id(id).flex().flex_col().w_full()
}

/// A one-line field over `form` for a name: `label` names it, `prompt` is
/// drawn in it while empty. It takes no typing while the submit is in
/// flight; the [`design::field`] it goes in says what stopped it. Return
/// sends nothing unless the caller says so (Expo sets `onSubmitEditing` on
/// some fields).
pub(super) fn name_input(
    id: impl Into<ElementId>,
    form: &Form,
    label: impl Into<String>,
    prompt: &str,
) -> Input {
    Input::new(id, &form.text, label)
        .placeholder(prompt)
        .disabled(form.busy)
        // Expo's fields set none: React Native's default, sentences
        .autocapitalize(Autocapitalize::Sentences)
}

/// What stopped `form`, worded; `empty` says what to type.
pub(super) fn problem_of(form: &Form, empty: &str) -> Option<String> {
    form.problem
        .as_ref()
        .map(|problem| words::problem_text(problem, empty))
}

// SDK gap: `design::quiet` at the ladder's sizes; the SDK builds it later
/// One muted line standing for a pane's content: a help line, a wait, a
/// note (the Expo kit's `Quiet`, `src/kit/States.tsx:40`).
pub(super) fn quiet(
    id: &str,
    text: impl Into<SharedString>,
    ladder: &Ladder,
    theme: &Theme,
) -> Stateful<Div> {
    div()
        .id(id.to_owned())
        .p(design::space::BLOCK)
        .text_type(ladder.secondary)
        .text_color(theme.muted)
        .child(text.into())
}

/// A sentence in the secondary size, in `tone`: a form's explanation, a
/// warning, what stopped it.
pub(super) fn note(text: impl Into<SharedString>, tone: Hsla, ladder: &Ladder) -> Div {
    div()
        .text_type(ladder.secondary)
        .text_color(tone)
        .child(text.into())
}

// SDK gap: `design::mono` at the ladder's sizes; the SDK builds it later
/// A caption in the data face: a key, an account number.
pub(super) fn mono(text: impl Into<SharedString>, ladder: &Ladder, theme: &Theme) -> Div {
    div()
        .font_family(design::fonts::FAMILY_MONO)
        .text_type(ladder.caption)
        .text_color(theme.muted)
        .child(text.into())
}

/// A row's leading glyph, muted.
pub(super) fn muted_icon(name: &str, ladder: &Ladder, theme: &Theme) -> Svg {
    design::icon(name, ladder.icon_row).text_color(theme.muted)
}

/// Who an account is, at the head of its screen or sheet: its face, then
/// its name (a [`name_line`]) over one mono line.
pub(super) fn named(
    face: Div,
    name: impl IntoElement,
    about: String,
    ladder: &Ladder,
    theme: &Theme,
) -> Div {
    div()
        .flex()
        .items_center()
        .gap(design::space::LG)
        .child(face)
        .child(
            div()
                .flex_1()
                .min_w_0()
                .flex()
                .flex_col()
                .child(name)
                .child(mono(about, ladder, theme)),
        )
}

/// An account's name over its line in `named`: one line, medium, in `size`.
pub(super) fn name_line(name: &str, size: design::Type) -> Div {
    div()
        .truncate()
        .text_type(size)
        .font_weight(FontWeight::MEDIUM)
        .child(name.to_owned())
}
