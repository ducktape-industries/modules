//! What the roster and the member page draw the same way, in the Expo
//! kit's shapes (mobile `src/kit/`): an account's face, its name line, a
//! row's middle column and chevron, the quiet status line and a refused
//! roster read. Reads one [`Row`], or the roster's refusal; Retry reads the
//! roster again.
use view::design::{self, Face, Ladder, Type, space};
use view::prelude::*;
use view::{Div, FontWeight, Svg};

use crate::{Members, Row, words};

/// A row with a caption under its title is this tall (`Row.tsx:44`,
/// `size.rowTall`).
// SDK gap: `Ladder::row_tall`, swap when feat/phone-kit lands
pub(super) const ROW_TALL: Pixels = px(design::height::touch::ROW_TALL as f32);

/// How far a row's caption sits under its title (`Row.tsx:58`, `gap: 1`).
// SDK gap: the row's title over caption, swap when feat/phone-kit lands
const TITLE_GAP: Pixels = px(1.);

/// An account that no longer acts shows its face at this opacity
/// (`Avatar.tsx:38`, `off`).
const OFF: f32 = 0.5;

/// An account's initial (the kit's `Avatar`): round for a person, the
/// agent tint for an agent, square and in the data face for a program;
/// faded when it no longer acts.
pub(super) fn face(row: &Row, size: Face, theme: &Theme) -> Div {
    let avatar = row
        .group()
        .face(design::avatar(&row.name, size, theme), theme);
    avatar.when(row.kind.note().is_some(), |avatar| avatar.opacity(OFF))
}

/// An account's name on one line in `role`, faint when it no longer acts,
/// with `you` after the reader's own: secondary, faint, regular, two
/// spaces on (`index.tsx:57-60`, `[number].tsx:84-88`).
pub(super) fn name_line(row: &Row, me: bool, role: Type, theme: &Theme, ladder: &Ladder) -> Div {
    div()
        .flex()
        .items_baseline()
        .min_w_0()
        .text_type(role)
        .when(row.kind.note().is_some(), |line| {
            line.text_color(theme.faint)
        })
        .child(div().min_w_0().truncate().child(row.name.clone()))
        .when(me, |line| {
            line.child(
                div()
                    .flex_none()
                    .text_type(ladder.secondary)
                    .font_weight(FontWeight::NORMAL)
                    .text_color(theme.faint)
                    .child(format!("  {}", words::YOU)),
            )
        })
}

/// A row's middle column (the kit's `Row`): the title, and a caption under
/// it when the row has one.
// SDK gap: the row's title over caption, swap when feat/phone-kit lands
pub(super) fn title_and_caption(title: impl IntoElement, caption: Option<Div>) -> Div {
    div()
        .flex_1()
        .min_w_0()
        .flex()
        .flex_col()
        .gap(TITLE_GAP)
        .child(title)
        .children(caption)
}

/// A row's caption: one muted line in caption size.
// SDK gap: the row's title over caption, swap when feat/phone-kit lands
pub(super) fn caption(text: impl Into<SharedString>, theme: &Theme, ladder: &Ladder) -> Div {
    div()
        .truncate()
        .text_type(ladder.caption)
        .text_color(theme.muted)
        .child(text.into())
}

/// The chevron at a row's end: the row opens a page.
// SDK gap: the row's chevron, swap when feat/phone-kit lands
pub(super) fn chevron(theme: &Theme, ladder: &Ladder) -> Svg {
    design::icon("chevron-right", ladder.icon_row).text_color(theme.faint)
}

/// One muted status line (the kit's `Quiet`): secondary, 16 all round.
// SDK gap: `quiet` at the ladder's sizes, swap when feat/phone-kit lands
pub(super) fn quiet(
    id: &'static str,
    text: impl Into<SharedString>,
    theme: &Theme,
    ladder: &Ladder,
) -> AnyElement {
    div()
        .id(id)
        .p(space::BLOCK)
        .text_type(ladder.secondary)
        .text_color(theme.muted)
        .child(text.into())
        .into_any_element()
}

/// A refused roster read: its sentence, and Retry, which reads it again.
// SDK gap: the kit's `Notice` strip for a refusal, swap when feat/phone-kit
// lands
pub(super) fn refused(
    refusal: &Error,
    cx: &mut Context<Members>,
    theme: &Theme,
    ladder: &Ladder,
) -> AnyElement {
    let retry = cx.listener(|view, _: &ClickEvent, _, cx| view.read(cx));
    div()
        .px(space::BLOCK)
        .py(space::SM)
        .child(design::refused(
            "members",
            refusal.message.clone(),
            ladder,
            theme,
            retry,
        ))
        .into_any_element()
}
