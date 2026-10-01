//! The small controls on a message card: the action strip's buttons, a
//! reaction, and the way into a thread.
use ducktape_view_guest::design;
use ducktape_view_guest::prelude::*;
use ducktape_view_guest::{
    ClickEvent, ElementId, ParentElement, Role, Styled, Theme, Window, div, px,
};

/// An action strip button's height, the smallest box a pointer presses
/// (the door's AX-017): the strip, borders and all, is 26. Its width is
/// the kit's row height, already over 24.
const STRIP_BUTTON_HEIGHT: f32 = 24.;
/// A reaction chip's height, the thread button's: the smallest box a
/// pointer presses (the door's AX-017), and a chip is no narrower.
const REACTION_HEIGHT: f32 = 24.;
/// The thread button's height under a root.
const REPLIES_HEIGHT: f32 = 24.;

/// `active`: the grid's arrows are on this button (it is never a Tab stop
/// of its own; the pane's message list is).
pub(super) fn action_button(
    id: impl Into<ElementId>,
    label: impl Into<String>,
    accessible: &str,
    theme: &Theme,
    enabled: bool,
    active: bool,
    click: impl Fn(&ClickEvent, &mut Window, &mut ducktape_view_guest::App) + 'static,
) -> impl IntoElement {
    let control = div()
        .id(id)
        .w(design::size::ROW)
        .h(px(STRIP_BUTTON_HEIGHT))
        .flex()
        .items_center()
        .justify_center()
        .bg(theme.background)
        .text_color(if enabled {
            theme.foreground
        } else {
            theme.faint
        })
        .role(ducktape_view_guest::Role::Button)
        .aria_label(accessible)
        .aria_disabled(!enabled)
        .text_size(design::text::SECONDARY)
        .child(label.into());
    if enabled {
        design::item(
            control
                .cursor_pointer()
                .hover(|s| s.bg(theme.surface_raised))
                .on_click(click),
            Role::Button,
            active,
        )
    } else {
        control
    }
}

/// What a reaction button shows: an emoji, how many chose it and whether
/// the reader is one of them, or the `+` that opens the picker.
pub(super) enum Face<'a> {
    Emoji {
        emoji: &'a str,
        count: u64,
        mine: bool,
    },
    Add,
}

/// `subject`: what the row is about, said after the chip's and the `+`'s
/// own name, as every reacted row shows the same controls.
pub(super) fn reaction_button(
    id: impl Into<ElementId>,
    face: Face,
    subject: &str,
    theme: &Theme,
    enabled: bool,
    active: bool,
    click: impl Fn(&ClickEvent, &mut Window, &mut ducktape_view_guest::App) + 'static,
) -> impl IntoElement {
    let mine = matches!(face, Face::Emoji { mine: true, .. });
    let mut control = div()
        .id(id)
        .h(px(REACTION_HEIGHT))
        .min_w(px(REACTION_HEIGHT))
        .px(design::space::XS)
        .flex()
        .items_center()
        .justify_center()
        .gap_1()
        .border_1()
        // the reader's own wear the strong line: accent_soft is the
        // hover grey in the calm palette, so a fill alone can't say "mine"
        .border_color(if mine { theme.accent } else { theme.border })
        .bg(if mine {
            theme.accent_soft
        } else {
            theme.background
        })
        .text_color(if enabled {
            theme.foreground
        } else {
            theme.muted
        })
        .role(ducktape_view_guest::Role::Button)
        .aria_disabled(!enabled)
        .text_size(design::text::SECONDARY);
    control = match face {
        // the count in the data face, as every count here is
        Face::Emoji { emoji, count, mine } => control
            // one name whether it is yours or not: the toggle says which
            .aria_label(format!("{emoji} reaction, {subject}"))
            .aria_description(count.to_string())
            .aria_toggled(mine.into())
            .child(emoji.to_owned())
            .child(
                div()
                    .font_family(design::fonts::FAMILY_MONO)
                    .text_size(design::text::CAPTION)
                    .child(count.to_string()),
            ),
        // `+` is the picker's method, not a toggle
        Face::Add => control
            .aria_label(format!("Add reaction, {subject}"))
            .child("+"),
    };
    if enabled {
        design::item(
            control
                .cursor_pointer()
                .hover(|style| {
                    style
                        .bg(theme.surface_raised)
                        .border_color(theme.border_strong)
                })
                .on_click(click),
            Role::Button,
            active,
        )
    } else {
        control
    }
}

/// Under a message with replies: how many, and the way into them, drawn as
/// the button it is. Its name ends with the row's `subject`, as two roots
/// with as many replies said the same.
pub(super) fn replies_button(
    id: impl Into<ElementId>,
    count: u64,
    subject: &str,
    theme: &Theme,
    active: bool,
    click: impl Fn(&ClickEvent, &mut Window, &mut ducktape_view_guest::App) + 'static,
) -> impl IntoElement {
    let replies = design::plural(count, "reply", "replies");
    let button = div()
        .id(id)
        .h(px(REPLIES_HEIGHT))
        .px_2()
        .flex()
        .items_center()
        .gap(design::space::XS)
        .border_1()
        .border_color(theme.border)
        .bg(theme.background)
        .text_size(design::text::SECONDARY)
        .text_color(theme.foreground)
        .cursor_pointer()
        .hover(|style| {
            style
                .bg(theme.surface_raised)
                .border_color(theme.border_strong)
        })
        .active(|style| style.bg(theme.accent_soft))
        .aria_label(format!("Open thread, {replies}, {subject}"))
        .on_click(click)
        .child(replies)
        .child(div().text_color(theme.muted).child("Open thread →"));
    design::item(button, Role::Button, active)
}
