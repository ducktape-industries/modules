//! Expo `src/kit/Row.tsx`, `src/kit/Field.tsx` and `src/kit/States.tsx`:
//! the kit's pieces both phone files draw that the SDK has no component
//! for yet: a row's words, a labelled field, a quiet line, a sentence. They
//! draw only what they are handed; none reads the state or writes.
use super::*;
use view::Svg;

/// A row with a caption under its title (Expo `src/kit/Row.tsx:44`).
pub(super) const TALL: Pixels = px(design::height::touch::ROW_TALL as f32);
/// The room between a row's title and its caption (Expo `src/kit/Row.tsx:58`).
const WORDS_GAP: Pixels = px(1.);
/// A stopped field's text inset: its edge goes 2 thick in the danger tone,
/// so the text moves one in to stay put (Expo `src/kit/Field.tsx:52`).
const STOPPED_INSET: Pixels = px((design::spacing::LG - 1.) as f32);

/// A field under its label, and under it what stopped it, in the danger
/// tone, its edge too (the Expo kit's `Field`).
pub(super) fn labelled(
    label: &str,
    field: impl IntoElement + Styled,
    problem: Option<String>,
    theme: &Theme,
    ladder: &Ladder,
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
        .map(|problem| problem_text(problem, empty))
}

/// One muted line standing for a pane's content: a help line, a wait, a
/// note (the Expo kit's `Quiet`, `src/kit/States.tsx:40`).
pub(super) fn quiet(
    id: &str,
    text: impl Into<SharedString>,
    theme: &Theme,
    ladder: &Ladder,
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

/// A row's words (the Expo kit's `Row`): the title on one line, in `tone`,
/// the caption under it.
pub(super) fn words(title: &str, tone: Hsla, caption: Div, ladder: &Ladder) -> Div {
    div()
        .flex_1()
        .min_w_0()
        .flex()
        .flex_col()
        .gap(WORDS_GAP)
        .child(
            div()
                .truncate()
                .text_type(ladder.body)
                .text_color(tone)
                .child(title.to_owned()),
        )
        .child(caption)
}

/// A caption in the data face: a key, an account number.
pub(super) fn mono(text: impl Into<SharedString>, theme: &Theme, ladder: &Ladder) -> Div {
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
