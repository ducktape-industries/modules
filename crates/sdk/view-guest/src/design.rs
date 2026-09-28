//! The shapes every view repeats, in one visual language: the design tokens
//! (re-exported whole), the type scale, heights and spacing as [`Pixels`]
//! (`text`, `size`, `space`), and the empty state, refused-with-retry
//! screen, quiet line, heading and mono run views used to copy between
//! them, with the controls (`controls.rs`), the number and time formatters
//! (`format.rs`) and Explorer's link paths (`explorer.rs`) beside them for
//! the same reason.
pub use ::design::*;

mod controls;
pub mod explorer;
mod format;
pub use controls::{Button, Kind, button, divider, segment, segmented, setting_row, switch, tab};
pub use format::{ago, clock, date, day, grouped, initial, local, plural, set_utc_offset};

use crate::prelude::*;
use crate::{Div, FontWeight, Hsla, Pixels, Stateful};

/// [`type_scale`] as sizes an element takes.
pub mod text {
    use crate::{Pixels, px};

    pub const TITLE: Pixels = px(::design::type_scale::TITLE as f32);
    pub const SECTION: Pixels = px(::design::type_scale::SECTION as f32);
    pub const BODY: Pixels = px(::design::type_scale::BODY as f32);
    pub const SECONDARY: Pixels = px(::design::type_scale::SECONDARY as f32);
    pub const CAPTION: Pixels = px(::design::type_scale::CAPTION as f32);
    pub const MONO: Pixels = px(::design::type_scale::MONO as f32);
}

/// [`height`] as sizes an element takes.
pub mod size {
    use crate::{Pixels, px};

    pub const ROW: Pixels = px(::design::height::ROW as f32);
    pub const CONTROL: Pixels = px(::design::height::CONTROL as f32);
    pub const AVATAR_SM: Pixels = px(::design::height::AVATAR_SM as f32);
    pub const AVATAR: Pixels = px(::design::height::AVATAR as f32);
    pub const AVATAR_LG: Pixels = px(::design::height::AVATAR_LG as f32);
    /// The host's vertical scroll bar, its hover width and insets: a
    /// scroller keeps this much of its right edge clear.
    pub const SCROLLBAR: Pixels = px(16.);
}

/// [`spacing`] as gaps and insets an element takes.
pub mod space {
    use crate::{Pixels, px};

    pub const HAIR: Pixels = px(::design::spacing::HAIR as f32);
    pub const XXS: Pixels = px(::design::spacing::XXS as f32);
    pub const XS: Pixels = px(::design::spacing::XS as f32);
    pub const SM: Pixels = px(::design::spacing::SM as f32);
    pub const MD: Pixels = px(::design::spacing::MD as f32);
    pub const LG: Pixels = px(::design::spacing::LG as f32);
    pub const BLOCK: Pixels = px(::design::spacing::BLOCK as f32);
    pub const XL: Pixels = px(::design::spacing::XL as f32);
}

/// Nothing to show yet: what is missing, then what would fill it.
pub fn empty_state(
    id: impl Into<ElementId>,
    title: impl Into<SharedString>,
    detail: impl Into<SharedString>,
    theme: &Theme,
) -> Stateful<Div> {
    div()
        .id(id)
        .flex()
        .flex_col()
        .gap_1()
        .p_6()
        .max_w(px(420.))
        .child(
            div()
                .text_size(text::SECTION)
                .font_weight(FontWeight::MEDIUM)
                .child(title.into()),
        )
        .child(
            div()
                .text_size(text::SECONDARY)
                .text_color(theme.muted)
                .child(detail.into()),
        )
}

/// A refused read: the reason, and Retry. The screen is `{name}-refused`,
/// its retry control `{name}-retry`.
pub fn refused(
    name: &str,
    sentence: impl Into<SharedString>,
    theme: &Theme,
    retry: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
) -> Stateful<Div> {
    let theme = *theme;
    div()
        .id(ElementId::Name(format!("{name}-refused").into()))
        .flex()
        .flex_col()
        .gap_2()
        .p_3()
        .border_1()
        .border_color(theme.danger)
        .bg(theme.danger_soft)
        .text_size(text::SECONDARY)
        .child(sentence.into())
        .child(
            div()
                .id(ElementId::Name(format!("{name}-retry").into()))
                .px_2()
                .py_1()
                .w(px(64.))
                .bg(theme.surface)
                .hover(move |style| style.bg(theme.surface_raised))
                .role(Role::Button)
                .focusable()
                .on_click(retry)
                .child("Retry"),
        )
}

/// One muted line of status text.
pub fn quiet(text: impl Into<SharedString>, theme: &Theme) -> Div {
    div()
        .text_size(text::SECONDARY)
        .text_color(theme.muted)
        .child(text.into())
}

/// A heading: the title size at level 1, the section size under it.
pub fn heading(
    id: impl Into<ElementId>,
    text: impl Into<SharedString>,
    level: usize,
    theme: &Theme,
) -> Stateful<Div> {
    div()
        .id(id)
        .text_size(if level == 1 {
            text::TITLE
        } else {
            text::SECTION
        })
        .font_weight(FontWeight::SEMIBOLD)
        .text_color(theme.foreground)
        .role(Role::Heading)
        .aria_level(level)
        .child(text.into())
}

/// A run of monospaced text on one line: hashes, keys, code.
pub fn mono(text: impl Into<SharedString>) -> Div {
    div()
        .font_family(fonts::FAMILY_MONO)
        .text_size(text::MONO)
        .whitespace_nowrap()
        .child(text.into())
}

/// A person's round initial at `size`, on the raised surface. A caller
/// recolours it (an agent, a speaker) with `bg` / `text_color`.
pub fn avatar(name: &str, size: Pixels, theme: &Theme) -> Div {
    div()
        .size(size)
        .flex_shrink_0()
        .rounded_full()
        .flex()
        .items_center()
        .justify_center()
        .bg(theme.surface_raised)
        .text_color(theme.muted)
        .text_size(size * 0.45)
        .child(initial(name))
}

/// Whether a side pane `side` wide fits in `width` beside what the screen
/// `keeps` (its list and its body's narrowest). When it does not, the pane
/// floats over the body ([`over`]) and the body keeps its whole width.
pub fn docks(width: f32, keeps: f32, side: f32) -> bool {
    width >= keeps + side
}

/// A side pane that does not [`docks`]: it covers the whole of its
/// `relative` parent, list and body alike, at the pane's own width no
/// more, so nothing underneath stays half in view; a click on it stops
/// there. The pane carries its own close control.
pub fn over(
    id: impl Into<ElementId>,
    pane: impl IntoElement + Styled,
    theme: &Theme,
) -> Stateful<Div> {
    div()
        .id(id)
        .absolute()
        .inset_0()
        .flex()
        .bg(theme.background)
        .occlude()
        .child(pane.w_full().h_full())
}

/// A small tag: a state, a role, a count, in its own colours.
pub fn badge(
    id: impl Into<ElementId>,
    label: impl Into<SharedString>,
    foreground: Hsla,
    background: Hsla,
) -> Stateful<Div> {
    div()
        .id(id)
        .px_1()
        .py_0p5()
        .bg(background)
        .text_color(foreground)
        .text_size(text::CAPTION)
        .child(label.into())
}

/// `block 1,024`, quiet and mono, opening Explorer at that block. A view
/// that draws it on a clickable card replaces the click (`on_click`) with
/// its own that claims it and opens the same [`explorer::link`].
pub fn block_link(id: impl Into<ElementId>, height: u64, theme: &Theme) -> Stateful<Div> {
    let label = format!("block {}", grouped(height));
    explorer_link(id, label, explorer::block_path(height), theme)
}

/// Subdued mono text that underlines under the pointer and opens
/// Explorer at `path` through `link.open`.
fn explorer_link(
    id: impl Into<ElementId>,
    label: String,
    path: String,
    theme: &Theme,
) -> Stateful<Div> {
    let theme = *theme;
    let link = explorer::link(&path);
    div()
        .id(id)
        .text_size(text::CAPTION)
        .text_color(theme.muted)
        .font_family(fonts::FAMILY_MONO)
        .whitespace_nowrap()
        .cursor_pointer()
        .hover(move |style| style.text_color(theme.foreground).text_decoration_1())
        .role(Role::Link)
        .aria_label(format!("Open {label} in Explorer"))
        .focusable()
        .on_click(move |_: &ClickEvent, _: &mut Window, cx: &mut App| cx.host().open_link(&link))
        .child(label)
}

#[cfg(test)]
mod tests {
    #[test]
    fn a_side_pane_docks_only_beside_the_whole_of_what_the_screen_keeps() {
        assert!(super::docks(1000., 576., 320.));
        assert!(super::docks(896., 576., 320.));
        assert!(!super::docks(895., 576., 320.));
        assert!(!super::docks(720., 400., 440.));
    }
}
