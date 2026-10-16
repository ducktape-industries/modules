//! Expo `src/kit/Row.tsx`, `src/kit/Field.tsx` and `src/kit/States.tsx`:
//! the kit's pieces both phone files draw. They draw only what they are
//! handed; none reads the state or writes. The ones marked "SDK gap" are
//! every view's, and come to the SDK in one PR; until then they live here
//! as they are.
use view::accesskit::Invalid;
use view::design::Ladder;
use view::prelude::*;
use view::{Div, ElementId, FontWeight, Stateful, Svg};

use crate::state::Form;
use crate::words;

// SDK gap: `Ladder::row_tall`, swap when feat/phone-kit lands
/// A row with a caption under its title (Expo `src/kit/Row.tsx:44`).
pub(super) const ROW_TALL: Pixels = px(design::height::touch::ROW_TALL as f32);
// SDK gap: the row's title over caption, swap when feat/phone-kit lands
/// The room between a row's title and its caption (Expo `src/kit/Row.tsx:58`).
const TITLE_GAP: Pixels = px(1.);
// SDK gap: the labelled field, swap when feat/phone-kit lands
/// A stopped field's text inset: its edge goes 2 thick in the danger tone,
/// so the text moves one in to stay put (Expo `src/kit/Field.tsx:52`).
const STOPPED_INSET: Pixels = px((design::spacing::LG - 1.) as f32);
/// A stopped field of many lines moves its lines one in from the top and
/// bottom too (Expo `src/kit/Field.tsx:79`).
pub(super) const STOPPED_LINES: Pixels = px((design::spacing::MD - 1.) as f32);

/// A column of a pane or a sheet, its children edge to edge.
pub(super) fn column(id: impl Into<ElementId>) -> Stateful<Div> {
    div().id(id).flex().flex_col().w_full()
}

/// A one-line field over `form` for a name: `label` names it, `prompt` is
/// drawn in it while empty. It takes no typing while the submit is in
/// flight, and tells assistive tech what stopped it. Return sends nothing
/// unless the caller says so (Expo sets `onSubmitEditing` on some fields).
pub(super) fn name_input(
    id: impl Into<ElementId>,
    form: &Form,
    label: impl Into<String>,
    prompt: &str,
    problem: Option<&str>,
) -> Input {
    Input::new(id, &form.text, label)
        .placeholder(prompt)
        .disabled(form.busy)
        // Expo's fields set none: React Native's default, sentences
        .autocapitalize(Autocapitalize::Sentences)
        .when_some(problem, |input, problem| {
            input.invalid(Invalid::True).description(problem)
        })
}

// SDK gap: the labelled field, swap when feat/phone-kit lands
/// A field under its label, and under it what stopped it, in the danger
/// tone, its edge too (the Expo kit's `Field`).
pub(super) fn labelled(
    label: &str,
    field: impl IntoElement + Styled,
    problem: Option<String>,
    ladder: &Ladder,
    theme: &Theme,
) -> Div {
    let field = match problem {
        Some(_) => field
            .border_2()
            .border_color(theme.danger)
            .px(STOPPED_INSET),
        None => field,
    };
    div()
        .flex()
        .flex_col()
        .child(
            div()
                .mb(design::space::XS)
                .text_type(ladder.caption)
                .text_color(theme.muted)
                .child(label.to_owned()),
        )
        .child(field)
        .children(problem.map(|problem| {
            div()
                .mt(design::space::XS)
                .text_type(ladder.caption)
                .text_color(theme.danger)
                .child(problem)
        }))
}

/// What stopped `form`, worded; `empty` says what to type.
pub(super) fn problem_of(form: &Form, empty: &str) -> Option<String> {
    form.problem
        .as_ref()
        .map(|problem| words::problem_text(problem, empty))
}

// SDK gap: quiet at the ladder's sizes, swap when feat/phone-kit lands
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

// SDK gap: the row's title over caption, swap when feat/phone-kit lands
/// A row's words (the Expo kit's `Row`): the title on one line, in `tone`,
/// the caption under it.
pub(super) fn title_and_caption(title: &str, tone: Hsla, caption: Div, ladder: &Ladder) -> Div {
    div()
        .flex_1()
        .min_w_0()
        .flex()
        .flex_col()
        .gap(TITLE_GAP)
        .child(
            div()
                .truncate()
                .text_type(ladder.body)
                .text_color(tone)
                .child(title.to_owned()),
        )
        .child(caption)
}

// SDK gap: the row's title over caption, swap when feat/phone-kit lands
/// The muted line under a row's title.
pub(super) fn caption(text: impl Into<SharedString>, ladder: &Ladder, theme: &Theme) -> Div {
    div()
        .truncate()
        .text_type(ladder.caption)
        .text_color(theme.muted)
        .child(text.into())
}

// SDK gap: mono at the ladder's sizes, swap when feat/phone-kit lands
/// A caption in the data face: a key, an account number.
pub(super) fn mono(text: impl Into<SharedString>, ladder: &Ladder, theme: &Theme) -> Div {
    div()
        .font_family(design::fonts::FAMILY_MONO)
        .text_type(ladder.caption)
        .text_color(theme.muted)
        .child(text.into())
}

// SDK gap: the sheet row's hint, swap when feat/phone-kit lands
/// A sheet row's value at its right, never squeezed (the Expo kit's
/// `SheetRow` hint, `src/kit/Sheet.tsx:168`).
pub(super) fn hint(text: &str, ladder: &Ladder, theme: &Theme) -> Div {
    div()
        .flex_none()
        .text_type(ladder.caption)
        .text_color(theme.muted)
        .child(text.to_owned())
}

// SDK gap: the row's chevron, swap when feat/phone-kit lands
/// A row that opens something (the Expo kit's `Row` `chevron`).
pub(super) fn chevron(ladder: &Ladder, theme: &Theme) -> Svg {
    design::icon("chevron-right", ladder.icon_row).text_color(theme.faint)
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
