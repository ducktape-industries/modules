//! What the roster and the member page draw the same way, in the Expo
//! kit's shapes (mobile `src/kit/`): an account's face, its name line, a
//! row's middle column and chevron, and the quiet status line. Reads one
//! [`Row`]; writes nothing.
use view::design::{self, Face, Ladder, Type, space};
use view::prelude::*;
use view::{Div, FontWeight, Svg};

use crate::{Group, Row};

/// A row with a caption under its title is this tall (`Row.tsx:44`,
/// `size.rowTall`).
pub(super) const ROW_TALL: Pixels = px(design::height::touch::ROW_TALL as f32);

/// How far a row's caption sits under its title (`Row.tsx:58`, `gap: 1`).
const TITLE_GAP: Pixels = px(1.);

/// An account that no longer acts shows its face at this opacity
/// (`Avatar.tsx:38`, `off`).
const OFF: f32 = 0.5;

/// An account's initial (the kit's `Avatar`): round for a person, the
/// agent tint for an agent, square and in the data face for a program;
/// faded when it no longer acts.
pub(super) fn face(row: &Row, size: Face, theme: &Theme) -> Div {
    let avatar = design::avatar(&row.name, size, theme);
    let avatar = match row.group() {
        Group::People => avatar,
        Group::Agents => avatar.bg(theme.agent_soft).text_color(theme.agent),
        Group::Programs => avatar
            .rounded_none()
            .font_family(design::fonts::FAMILY_MONO),
    };
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
                    .child("  you"),
            )
        })
}

/// A row's middle column (the kit's `Row`): the title, and a caption under
/// it when the row has one.
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
pub(super) fn caption(text: impl Into<SharedString>, theme: &Theme, ladder: &Ladder) -> Div {
    div()
        .truncate()
        .text_type(ladder.caption)
        .text_color(theme.muted)
        .child(text.into())
}

/// The chevron at a row's end: the row opens a page.
pub(super) fn chevron(theme: &Theme, ladder: &Ladder) -> Svg {
    design::icon("chevron-right", ladder.icon_row).text_color(theme.faint)
}

/// One muted status line (the kit's `Quiet`): secondary, 16 all round.
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
