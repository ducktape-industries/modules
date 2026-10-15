//! The shapes every view repeats, in one visual language: the design tokens
//! (re-exported whole), the type scale, heights and spacing as [`Pixels`]
//! (`text`, `size`, `space`), and the empty state, refused-with-retry
//! screen, quiet line, heading and mono run views used to copy between
//! them, and the controls: buttons, tabs, the segmented choice, the switch
//! and the row it sits in, the pane divider and the link. The number and
//! time formatters (`format.rs`) sit beside them for the same reason. The
//! phone's shapes sit here too: the width class a view lays out by
//! ([`width_class`]), the size ladder it picks by the session's pointer
//! ([`ladder`]), the page stack ([`Pages`]) and the page frame ([`page`])
//! a Compact layout is made of, and the sheet ([`sheet`]).
pub use ::design::*;

mod format;
mod pages;
pub(crate) use format::set_utc_offset;
pub use format::{ago, clock, date, day, grouped, initial, local, plural};
pub use pages::{CAPSULE_CLEARANCE, Page, Pages, page};

use crate::prelude::*;
use crate::{AnyElement, Div, FontWeight, Hsla, Pixels, Stateful, StyleRefinement};
pub use gpui::Orientation;
use gpui::{BoxShadow, ScrollStrategy};

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

/// How wide the body a view is laid out in is, for the one-pane question
/// ([`width::COMPACT_BELOW`], [`width::WIDE_FROM`]): a Compact body shows
/// one page of the view's [`Pages`] at a time; Medium and Wide dock panes,
/// by the view's own [`docks`] arithmetic. Read once per render from the
/// viewport (`design::width_class(window.viewport_size().width.into())`)
/// and matched visibly, never a view-private threshold.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WidthClass {
    Compact,
    Medium,
    Wide,
}

/// The [`WidthClass`] of a body `width` logical px wide.
pub fn width_class(width: f32) -> WidthClass {
    if width < width::COMPACT_BELOW as f32 {
        WidthClass::Compact
    } else if width < width::WIDE_FROM as f32 {
        WidthClass::Medium
    } else {
        WidthClass::Wide
    }
}

/// A text role: its size, and the line height it is set on, if the role
/// names one. Set on an element with [`TextType::text_type`]. A pointer's
/// roles name none (the host's line height stands, as on the desktop
/// today); a finger's are the Expo app's (`theme.ts` `type`).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Type {
    pub size: Pixels,
    pub line: Option<Pixels>,
}

impl Type {
    /// A role at `size` on the host's line height.
    pub const fn size(size: Pixels) -> Self {
        Self { size, line: None }
    }
    /// A role at `size` on a `line` tall line.
    pub const fn lined(size: f64, line: f64) -> Self {
        Self {
            size: px(size as f32),
            line: Some(px(line as f32)),
        }
    }
}

/// [`Type`] on any styled element: `div().text_type(ladder.body)`.
pub trait TextType: Styled + Sized {
    /// The role's size, and its line height where it names one.
    fn text_type(self, role: Type) -> Self {
        let element = self.text_size(role.size);
        match role.line {
            Some(line) => element.line_height(line),
            None => element,
        }
    }
}

impl<E: Styled> TextType for E {}

/// An [`avatar`]'s size: its side, its initial's text role and weight. A
/// side alone (`design::size::AVATAR`) sets its initial at 0.45 of it; a
/// finger's rungs ([`Ladder::avatar`] and the rest) are the Expo kit's
/// `Avatar`, whose initial sizes are a table, not one ratio, and medium.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Face {
    pub side: Pixels,
    pub text: Type,
    pub weight: Option<FontWeight>,
}

impl Face {
    /// A side of `side` px with its initial at 0.45 of it, in the face's
    /// own weight.
    pub const fn of(side: f32) -> Self {
        Self {
            side: px(side),
            text: Type::size(px(side * 0.45)),
            weight: None,
        }
    }
    /// The Expo kit's face: `text` medium on a line 1.25 times it.
    const fn kit(side: f64, text: f64) -> Self {
        Self {
            side: px(side as f32),
            text: Type::lined(text, text * 1.25),
            weight: Some(FontWeight::MEDIUM),
        }
    }
}

impl From<Pixels> for Face {
    fn from(side: Pixels) -> Self {
        Self::of(side.into())
    }
}

/// The sizes a view draws its controls, rows and text at: one set under a
/// pointer ([`height`], [`type_scale`]), one under a finger (their `touch`
/// modules, the Expo app's theme; every pressable thing at least
/// [`Ladder::hit`] each way). Two constant sets, nothing computed. Picked
/// once per render from the device (`design::ladder(window.touch())`) and
/// passed down to every helper that sizes something, so the choice is
/// visible where it is made.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Ladder {
    /// The session's pointer is a finger: the set this is.
    pub touch: bool,
    /// a page's title row
    pub header: Pixels,
    /// a list row
    pub row: Pixels,
    /// a row of a sheet
    pub sheet_row: Pixels,
    /// a control on one line
    pub control: Pixels,
    /// the smallest box a press lands in, each way
    pub hit: Pixels,
    /// a person's initial in a row or beside a message
    pub avatar: Face,
    /// a person's initial inline: a title row, a mention
    pub avatar_inline: Face,
    /// a person's initial heading a sheet
    pub avatar_sheet: Face,
    /// a person's initial heading their own page
    pub avatar_detail: Face,
    /// an icon on an icon button or in a title row
    pub icon: Pixels,
    /// an icon leading a list row
    pub icon_row: Pixels,
    /// an icon leading a sheet row
    pub icon_sheet: Pixels,
    /// a view's first page's title
    pub big_title: Type,
    /// a detail head's name
    pub heading: Type,
    /// a page title
    pub title: Type,
    /// body text
    pub body: Type,
    /// a control's label
    pub label: Type,
    /// strip items, key/value rows
    pub small: Type,
    /// secondary copy
    pub secondary: Type,
    /// a caption, a timestamp
    pub caption: Type,
    /// a badge, a count
    pub tiny: Type,
    /// code, in the data face
    pub code: Type,
}

/// The pointer's ladder: today's desktop numbers, unchanged, on the host's
/// line height.
pub const POINTER: Ladder = Ladder {
    touch: false,
    header: size::ROW,
    row: size::ROW,
    sheet_row: size::ROW,
    control: size::CONTROL,
    hit: px(::design::height::HIT as f32),
    avatar: Face::of(::design::height::AVATAR as f32),
    avatar_inline: Face::of(::design::height::AVATAR_SM as f32),
    avatar_sheet: Face::of(::design::height::AVATAR_LG as f32),
    avatar_detail: Face::of(::design::height::AVATAR_LG as f32),
    icon: px(::design::height::ICON as f32),
    icon_row: px(::design::height::ICON as f32),
    icon_sheet: px(::design::height::ICON as f32),
    big_title: Type::size(text::TITLE),
    heading: Type::size(text::TITLE),
    title: Type::size(text::TITLE),
    body: Type::size(text::BODY),
    label: Type::size(text::SECONDARY),
    small: Type::size(text::BODY),
    secondary: Type::size(text::SECONDARY),
    caption: Type::size(text::CAPTION),
    tiny: Type::size(text::CAPTION),
    code: Type::size(text::MONO),
};

/// The finger's ladder: the Expo app's numbers (mobile `src/design/theme.ts`
/// `type` and `size`).
pub const TOUCH: Ladder = {
    use ::design::{height::touch as h, type_scale::touch as t};
    Ladder {
        touch: true,
        header: px(h::HEADER as f32),
        row: px(h::ROW as f32),
        sheet_row: px(h::SHEET_ROW as f32),
        control: px(h::CONTROL as f32),
        hit: px(h::HIT as f32),
        avatar: Face::kit(h::AVATAR, h::AVATAR_TEXT),
        avatar_inline: Face::kit(h::AVATAR_INLINE, h::AVATAR_INLINE_TEXT),
        avatar_sheet: Face::kit(h::AVATAR_SHEET, h::AVATAR_SHEET_TEXT),
        avatar_detail: Face::kit(h::AVATAR_DETAIL, h::AVATAR_DETAIL_TEXT),
        icon: px(h::ICON as f32),
        icon_row: px(h::ICON_ROW as f32),
        icon_sheet: px(h::ICON_SHEET as f32),
        big_title: Type::lined(t::BIG_TITLE, t::BIG_TITLE_LINE),
        heading: Type::lined(t::HEADING, t::HEADING_LINE),
        title: Type::lined(t::TITLE, t::TITLE_LINE),
        body: Type::lined(t::BODY, t::BODY_LINE),
        // a button's label is body text (Expo `Button`)
        label: Type::lined(t::BODY, t::BODY_LINE),
        small: Type::lined(t::SMALL, t::SMALL_LINE),
        secondary: Type::lined(t::SECONDARY, t::SECONDARY_LINE),
        caption: Type::lined(t::CAPTION, t::CAPTION_LINE),
        tiny: Type::lined(t::TINY, t::TINY_LINE),
        code: Type::lined(t::CODE, t::CODE_LINE),
    }
};

/// The [`Ladder`] for a device: [`TOUCH`] when its primary pointer is a
/// finger (`window.touch()`), else [`POINTER`].
pub const fn ladder(touch: bool) -> Ladder {
    if touch { TOUCH } else { POINTER }
}

/// A [`Sheet`](crate::Sheet) in the view's colours. Under a finger it is
/// the Expo kit's `Sheet`: its panel on the window's ground under a
/// `border_strong` hairline, edge to edge, so a row's ground and a hairline
/// reach both sides; what it holds pads itself ([`sheet_head`],
/// [`sheet_row`]). Under a pointer the panel pads its content by
/// [`space::BLOCK`]. `design::sheet("actions", "Message actions", ladder,
/// theme).detents([0.6, 1.0]).on_dismiss(..).child(..)`; the host draws the
/// scrim and the handle and docks or centres it by the body's width class.
pub fn sheet(
    id: impl Into<ElementId>,
    label: impl Into<String>,
    ladder: &Ladder,
    theme: &Theme,
) -> crate::Sheet {
    let sheet = crate::sheet(id, label).bg(theme.background);
    if ladder.touch {
        sheet.border_t_1().border_color(theme.border_strong)
    } else {
        sheet.p(space::BLOCK)
    }
}

/// How far a sheet's head and rows sit in from its sides (the Expo kit's
/// `SheetHead`, `SheetRow`).
const SHEET_INSET: Pixels = px(20.);

/// What a sheet is about, at its top (the Expo kit's `SheetHead`): 4 over,
/// 12 under, [`SHEET_INSET`] in from each side, a hairline under it. The
/// caller adds its lines (who and when in `ladder.caption`, then the
/// subject in `ladder.body`).
pub fn sheet_head(id: impl Into<ElementId>, theme: &Theme) -> Stateful<Div> {
    div()
        .id(id)
        .flex()
        .flex_col()
        .pt(space::XXS)
        .pb(space::LG)
        .px(SHEET_INSET)
        .border_b_1()
        .border_color(theme.border)
}

/// One action of a sheet (the Expo kit's `SheetRow`): [`Ladder::sheet_row`]
/// tall, the whole row the press, an icon at [`Ladder::icon_sheet`] in
/// muted (`None` for a row with none), then `label` in body text; a
/// `danger` row (Delete) wears the danger tone on both.
pub fn sheet_row(
    id: impl Into<ElementId>,
    icon: Option<&str>,
    label: impl Into<SharedString>,
    danger: bool,
    ladder: &Ladder,
    theme: &Theme,
    click: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
) -> Stateful<Div> {
    let (glyph, words) = match danger {
        true => (theme.danger, theme.danger),
        false => (theme.muted, theme.foreground),
    };
    div()
        .id(id)
        .w_full()
        .h(ladder.sheet_row)
        .flex()
        .items_center()
        .gap(px(14.))
        .px(SHEET_INSET)
        .when(ladder.touch, pressed)
        .role(Role::Button)
        .focusable()
        .on_click(click)
        .children(icon.map(|name| self::icon(name, ladder.icon_sheet).text_color(glyph)))
        .child(
            div()
                .flex_1()
                .min_w_0()
                .truncate()
                .text_type(ladder.body)
                .text_color(words)
                .child(label.into()),
        )
}

/// A Lucide icon of the set the app bundles (`icons/<name>.svg`, the Expo
/// kit's `Icon`), `size` each way, drawn in the text colour it is given or
/// inherits: the caller names the tone (`.text_color(theme.muted)`). It
/// says nothing to a screen reader; the control it sits in is named.
pub fn icon(name: &str, size: Pixels) -> crate::Svg {
    crate::svg()
        .path(format!("icons/{name}.svg"))
        .size(size)
        .flex_none()
}

/// What a press shows under a finger: the control dims while it is held
/// (the Expo kit's `platform.pressed`); a finger has no hover.
fn pressed<E: StatefulInteractiveElement>(element: E) -> E {
    element.active(|style| style.opacity(0.6))
}

/// A text field's box (the Expo kit's `Field`): at least
/// [`Ladder::control`] tall, a `border_strong` hairline on the window's
/// ground, body text 12 in from its sides. The host draws the edge the
/// focused field wears.
pub fn field(input: crate::Input, ladder: &Ladder, theme: &Theme) -> crate::Input {
    input
        .min_h(ladder.control)
        .px(space::LG)
        .border_1()
        .border_color(theme.border_strong)
        .bg(theme.background)
        .text_color(theme.foreground)
        .text_type(ladder.body)
}

/// The column a [`row`] leads with (the Expo kit's `Row` `lead`): 24 wide,
/// its child centred in it: an icon at [`Ladder::icon_row`], an avatar.
pub fn lead(child: impl IntoElement) -> Div {
    div()
        .w(px(24.))
        .flex_none()
        .flex()
        .items_center()
        .justify_center()
        .child(child)
}

/// A list row (the Expo kit's `Row`): at least [`Ladder::row`] tall, 16 in
/// from each side, 12 between its parts, a hairline under it. The caller
/// adds a [`lead`], the title in `ladder.body` (with a caption under it in
/// `ladder.caption`, and the row then at least 56: `.min_h(..)`), what
/// trails it, and its press and role; the whole row is the press.
pub fn row(id: impl Into<ElementId>, ladder: &Ladder, theme: &Theme) -> Stateful<Div> {
    div()
        .id(id)
        .w_full()
        .min_h(ladder.row)
        .flex()
        .items_center()
        .gap(space::LG)
        .py(space::XS)
        .px(space::BLOCK)
        .border_b_1()
        .border_color(theme.border)
        .when(ladder.touch, pressed)
}

/// A list's section head (the Expo kit's `SectionHead`): at least 40
/// tall, 12 over and 4 under, 16 in; `label` in caption medium, `count`
/// (how many rows the section has) at the right, both muted.
pub fn section_head(
    id: impl Into<ElementId>,
    label: impl Into<SharedString>,
    count: Option<usize>,
    ladder: &Ladder,
    theme: &Theme,
) -> Stateful<Div> {
    div()
        .id(id)
        .min_h(px(40.))
        .flex()
        .items_center()
        .justify_between()
        .pt(space::LG)
        .pb(space::XXS)
        .px(space::BLOCK)
        .text_type(ladder.caption)
        .text_color(theme.muted)
        .role(Role::Heading)
        .aria_level(2)
        .child(div().font_weight(FontWeight::MEDIUM).child(label.into()))
        .children(count.map(|count| count.to_string()))
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
    ladder: &Ladder,
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
                .min_h(ladder.hit)
                .bg(theme.surface)
                // the Expo kit's `Notice` action: medium
                .when(ladder.touch, |retry| retry.font_weight(FontWeight::MEDIUM))
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

/// The focus ring: `color`, 2 px, just inside the edge, on a control the
/// keyboard has reached. The SDK draws it on every focusable node that has
/// no `focus_visible` of its own; a control whose own edge is already ink
/// shows it in the ink's foreground ([`focus_shown_on_ink`]).
pub(crate) fn focus_ring(color: Hsla) -> StyleRefinement {
    StyleRefinement::default()
        .shadow(vec![ring_shadow(color, 2., true)])
        // inset shadows paint under the border: a bordered control would
        // otherwise show one pixel of the two
        .border_color(color)
}

fn ring_shadow(color: Hsla, spread: f32, inset: bool) -> BoxShadow {
    BoxShadow {
        color,
        offset: gpui::point(px(0.), px(0.)),
        blur_radius: px(0.),
        spread_radius: px(spread),
        inset,
    }
}

/// The focus ring in ink, for an element whose own `focus_visible` would
/// otherwise replace it: `style` is what the element adds to it.
pub fn focus_shown<E: InteractiveElement>(
    element: E,
    theme: &Theme,
    style: impl FnOnce(StyleRefinement) -> StyleRefinement,
) -> E {
    let ring = focus_ring(theme.accent);
    element.focus_visible(|_| {
        use gpui::Refineable;
        let mut shown = style(StyleRefinement::default());
        // the ring's shadow joins the element's own (a menu's `shadow_lg`)
        // instead of replacing it
        let mut shadows = ring.box_shadow.clone().unwrap_or_default();
        shadows.extend(shown.box_shadow.take().unwrap_or_default());
        shown.refine(&ring);
        shown.box_shadow = Some(shadows);
        shown
    })
}

/// The focus ring on an ink-filled control (a primary button): the ink's
/// foreground, since ink on ink shows nothing.
pub fn focus_shown_on_ink<E: InteractiveElement>(element: E, theme: &Theme) -> E {
    element.focus_visible(|_| focus_ring(theme.primary_foreground))
}

/// What a [`Button`] is among its neighbours.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Kind {
    /// a surface fill
    #[default]
    Plain,
    /// the one action a screen leads with: the primary fill
    Primary,
    /// a choice among many: no fill, muted text
    Quiet,
    /// a secondary action beside the primary one: the window's ground in a
    /// hairline box
    Outline,
}

/// A button. Disabled keeps it visible, drops the click and says so.
/// Selected is the chosen one: fg text on the window and an fg edge. A
/// button told whether it is selected is a toggle, pressed or not; one
/// never told is a plain button.
#[derive(IntoElement)]
pub struct Button<F>
where
    F: Fn(&ClickEvent, &mut Window, &mut App) + 'static,
{
    id: ElementId,
    label: SharedString,
    ladder: Ladder,
    theme: Theme,
    enabled: bool,
    kind: Kind,
    selected: Option<bool>,
    /// `Some(active)`: a cell of a [`composite`] grid, never focusable
    item: Option<bool>,
    click: F,
}

pub fn button<F>(
    id: impl Into<ElementId>,
    label: impl Into<SharedString>,
    ladder: &Ladder,
    theme: &Theme,
    click: F,
) -> Button<F>
where
    F: Fn(&ClickEvent, &mut Window, &mut App) + 'static,
{
    Button {
        id: id.into(),
        label: label.into(),
        ladder: *ladder,
        theme: *theme,
        enabled: true,
        kind: Kind::Plain,
        selected: None,
        item: None,
        click,
    }
}

impl<F> Button<F>
where
    F: Fn(&ClickEvent, &mut Window, &mut App) + 'static,
{
    pub fn enabled(mut self, enabled: bool) -> Self {
        self.enabled = enabled;
        self
    }
    pub fn kind(mut self, kind: Kind) -> Self {
        self.kind = kind;
        self
    }
    pub fn selected(mut self, selected: bool) -> Self {
        self.selected = Some(selected);
        self
    }
    /// The button as an [`item`] of a composite: it leaves the Tab order
    /// and claims the active descendant when `active`.
    pub fn item(mut self, active: bool) -> Self {
        self.item = Some(active);
        self
    }
}

impl<F> RenderOnce for Button<F>
where
    F: Fn(&ClickEvent, &mut Window, &mut App) + 'static,
{
    fn render(self, _: &mut Window, _: &mut App) -> impl IntoElement {
        if self.ladder.touch {
            return self.touched();
        }
        let theme = self.theme;
        let selected = self.selected == Some(true);
        let mut element = div()
            .id(self.id)
            .px_2()
            .py_1()
            .min_h(self.ladder.hit)
            .flex()
            .items_center()
            .text_type(self.ladder.label)
            .role(Role::Button)
            .child(self.label);
        // The chosen one is the ink one: fg text on the window, an fg edge
        // around it; the rest stay quiet.
        element = match (self.kind, selected) {
            // a primary that cannot run greys out but keeps its place
            (Kind::Primary, _) if !self.enabled => {
                element.bg(theme.faint).text_color(theme.primary_foreground)
            }
            (Kind::Primary, _) => focus_shown_on_ink(
                element
                    .bg(theme.primary)
                    .text_color(theme.primary_foreground),
                &theme,
            ),
            // the edge is ink already: the ring reaches one pixel further in
            (_, true) => element
                .bg(theme.background)
                .text_color(theme.foreground)
                .border_1()
                .border_color(theme.foreground)
                .focus_visible(move |style| {
                    style.shadow(vec![ring_shadow(theme.accent, 3., true)])
                }),
            (Kind::Quiet, false) => element
                .text_color(theme.muted)
                .border_1()
                .border_color(crate::hsla(0., 0., 0., 0.)),
            (Kind::Plain, false) => element
                .bg(theme.surface)
                .text_color(theme.foreground)
                .border_1()
                .border_color(theme.surface),
            (Kind::Outline, false) => element
                .bg(theme.background)
                .text_color(theme.foreground)
                .border_1()
                .border_color(theme.border_strong),
        };
        if let Some(pressed) = self.selected {
            element = element.aria_toggled(pressed.into());
        }
        if selected {
            element = element.font_weight(FontWeight::MEDIUM);
        }
        if !self.enabled {
            return match self.kind {
                Kind::Primary => element.aria_disabled(true),
                _ => element.text_color(theme.muted).aria_disabled(true),
            };
        }
        element = match (self.kind, selected) {
            (Kind::Quiet, false) => element.hover(move |style| style.text_color(theme.foreground)),
            (Kind::Plain, false) => element
                .hover(move |style| style.bg(theme.surface_raised))
                .active(move |style| style.bg(theme.accent_soft)),
            (Kind::Outline, false) => element.hover(move |style| style.bg(theme.surface)),
            _ => element,
        };
        let element = element.on_click(self.click);
        match self.item {
            Some(active) => item(element, Role::Button, active),
            None => element.focusable(),
        }
    }
}

impl<F> Button<F>
where
    F: Fn(&ClickEvent, &mut Window, &mut App) + 'static,
{
    /// Under a finger, the Expo kit's `Button`: square, [`Ladder::control`]
    /// tall (48 for the primary), 16 in, a hairline edge; the label in body
    /// text, medium but for a quiet one. A plain and an outline button draw
    /// the kit's outline (it has no grey fill); a disabled one goes faint
    /// on a quiet edge. No hover: a press dims it.
    fn touched(self) -> Stateful<Div> {
        let theme = self.theme;
        let selected = self.selected == Some(true);
        let clear = crate::hsla(0., 0., 0., 0.);
        // (ground, edge, text)
        let (ground, edge, tone) = match (self.kind, self.enabled, selected) {
            (Kind::Primary, false, _) => (
                Some(theme.surface_raised),
                theme.surface_raised,
                theme.faint,
            ),
            (_, false, _) => (None, theme.border, theme.faint),
            (Kind::Primary, true, _) => {
                (Some(theme.primary), theme.primary, theme.primary_foreground)
            }
            // the chosen one of a toggle wears the ink edge
            (_, true, true) => (None, theme.foreground, theme.foreground),
            (Kind::Quiet, true, false) => (None, clear, theme.muted),
            (Kind::Plain | Kind::Outline, true, false) => {
                (None, theme.border_strong, theme.foreground)
            }
        };
        let height = match self.kind {
            Kind::Primary => px(::design::height::touch::PRIMARY as f32),
            _ => self.ladder.control,
        };
        let mut element = div()
            .id(self.id)
            .h(height)
            .px(space::BLOCK)
            .flex()
            .items_center()
            .justify_center()
            .gap(space::SM)
            .border_1()
            .border_color(edge)
            .text_color(tone)
            .text_type(self.ladder.label)
            .when(self.kind != Kind::Quiet || selected, |button| {
                button.font_weight(FontWeight::MEDIUM)
            })
            .when_some(ground, |button, ground| button.bg(ground))
            .role(Role::Button)
            .child(self.label);
        if let Some(pressed) = self.selected {
            element = element.aria_toggled(pressed.into());
        }
        if !self.enabled {
            return element.aria_disabled(true);
        }
        let mut element = pressed(element).on_click(self.click);
        if self.kind == Kind::Primary {
            element = focus_shown_on_ink(element, &theme);
        }
        match self.item {
            Some(active) => item(element, Role::Button, active),
            None => element.focusable(),
        }
    }
}

/// A control drawn as a glyph alone (the Expo kit's `Hit`): an
/// [`icon`] at [`Ladder::icon`], a cross, a plus. `name` is what it does,
/// in words, since the glyph says nothing to a screen reader. Under a
/// pointer the box is no smaller than [`Ladder::hit`] each way and the
/// glyph is muted until the pointer is on it; under a finger the box is
/// [`Ladder::hit`] each way exactly, the glyph in the foreground, and a
/// press dims it. A caller may recolour the glyph (`.text_color(..)`).
pub fn icon_button<F>(
    id: impl Into<ElementId>,
    glyph: impl IntoElement,
    name: impl Into<SharedString>,
    ladder: &Ladder,
    theme: &Theme,
    click: F,
) -> IconButton<F>
where
    F: Fn(&ClickEvent, &mut Window, &mut App) + 'static,
{
    let theme = *theme;
    let element = div().id(id).flex().items_center().justify_center();
    let element = match ladder.touch {
        true => element
            .size(ladder.hit)
            .flex_none()
            .text_color(theme.foreground),
        false => element
            .min_w(ladder.hit)
            .min_h(ladder.hit)
            .px_1()
            .text_color(theme.muted)
            .cursor_pointer()
            .hover(move |style| style.text_color(theme.foreground)),
    };
    IconButton {
        element: element.role(Role::Button).aria_label(name).child(glyph),
        touch: ladder.touch,
        theme,
        filled: false,
        enabled: true,
        click,
    }
}

/// An [`icon_button`] being built: `.filled(true)` makes it the ink square
/// with the glyph in the ink's foreground (a composer's Send once there is
/// something to send); `.enabled(false)` keeps it visible, faint and
/// unpressable, and says so.
#[derive(IntoElement)]
pub struct IconButton<F>
where
    F: Fn(&ClickEvent, &mut Window, &mut App) + 'static,
{
    element: Stateful<Div>,
    touch: bool,
    theme: Theme,
    filled: bool,
    enabled: bool,
    click: F,
}

impl<F> IconButton<F>
where
    F: Fn(&ClickEvent, &mut Window, &mut App) + 'static,
{
    pub fn filled(mut self, filled: bool) -> Self {
        self.filled = filled;
        self
    }
    pub fn enabled(mut self, enabled: bool) -> Self {
        self.enabled = enabled;
        self
    }
}

impl<F> Styled for IconButton<F>
where
    F: Fn(&ClickEvent, &mut Window, &mut App) + 'static,
{
    fn style(&mut self) -> &mut StyleRefinement {
        self.element.style()
    }
}

impl<F> RenderOnce for IconButton<F>
where
    F: Fn(&ClickEvent, &mut Window, &mut App) + 'static,
{
    fn render(self, _: &mut Window, _: &mut App) -> impl IntoElement {
        let theme = self.theme;
        let mut element = self.element;
        if self.filled {
            element = element
                .bg(theme.primary)
                .text_color(theme.primary_foreground);
        }
        if !self.enabled {
            return element.text_color(theme.faint).aria_disabled(true);
        }
        if self.touch {
            element = pressed(element);
        }
        if self.filled {
            element = focus_shown_on_ink(element, &theme);
        }
        element.focusable().on_click(self.click)
    }
}

/// A tab: quiet text, the chosen one fg and underlined, no fill. A caller
/// sizes it to its bar (`h_full`, `flex_1`) and may label a glyph. It is
/// an [`item`] of a `TabList` [`composite`], which holds the focus and the
/// arrows: the caller wraps it in `item(.., Role::Tab, active)`.
pub fn tab(
    id: impl Into<ElementId>,
    label: impl Into<SharedString>,
    selected: bool,
    ladder: &Ladder,
    theme: &Theme,
    click: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
) -> Stateful<Div> {
    let theme = *theme;
    div()
        .id(id)
        .flex()
        .items_center()
        .px_2()
        .py_1()
        .min_h(ladder.hit)
        .text_type(ladder.label)
        .text_color(if selected {
            theme.foreground
        } else {
            theme.muted
        })
        .border_b_2()
        .border_color(if selected {
            theme.foreground
        } else {
            theme.background
        })
        .when(selected, |tab| tab.font_weight(FontWeight::MEDIUM))
        .hover(move |style| style.text_color(theme.foreground))
        .role(Role::Tab)
        .aria_selected(selected)
        .on_click(click)
        .child(label.into())
}

/// One Tab stop whose items the arrows pick: a tab list, a radio group, a
/// menu, a list box, a tree, a grid. The composite takes the focus and the
/// keys; the picked item is its active descendant ([`item`]) and is never
/// focusable. [`Composite::build`] gives the element; the caller styles it
/// and adds the items.
pub fn composite(
    id: impl Into<ElementId>,
    role: Role,
    label: impl Into<SharedString>,
) -> Composite {
    Composite {
        element: div().id(id).role(role).aria_label(label),
        items: Vec::new(),
        orientation: Orientation::Vertical,
        columns: None,
        cells: None,
        active: 0,
        count: 0,
        wrap: false,
        scroll: None,
        on_move: None,
        on_move_cell: None,
        on_press: None,
    }
}

type Picked = Box<dyn Fn(usize, &mut Window, &mut App)>;

/// A [`composite`] being built: its role, name, axis, active item and the
/// callbacks its keys reach; [`Composite::build`] is the element.
pub struct Composite {
    element: Stateful<Div>,
    items: Vec<AnyElement>,
    orientation: Orientation,
    columns: Option<usize>,
    /// the active row's active cell among its cells
    cells: Option<(usize, usize)>,
    active: usize,
    count: usize,
    wrap: bool,
    /// The uniform list the items are rows of: a move scrolls it to the
    /// row moved to, ahead of the render that claims it.
    scroll: Option<crate::UniformListScrollHandle>,
    on_move: Option<Picked>,
    on_move_cell: Option<Picked>,
    on_press: Option<Picked>,
}

impl Composite {
    /// Which arrows step: `Horizontal` ← →, `Vertical` ↑ ↓ (the default).
    /// Sent as `aria_orientation` too.
    pub fn orientation(mut self, orientation: Orientation) -> Self {
        self.orientation = orientation;
        self
    }
    /// A grid of `columns` cells a row: ← → step by one, ↑ ↓ by a row;
    /// Home/End go to the row's ends, Ctrl+Home/End to the grid's.
    pub fn grid(mut self, columns: usize) -> Self {
        self.columns = Some(columns.max(1));
        self
    }
    /// The active item among `count`. The view keeps the index; a live list
    /// keeps the item's id and maps it to an index each render.
    pub fn active(mut self, index: usize, count: usize) -> Self {
        self.active = index;
        self.count = count;
        self
    }
    /// A grid whose rows have cells of their own (a message and its
    /// controls): `active` is the active row's active cell among `count`.
    /// ← → step the cells ([`Self::on_move_cell`]), ↑ ↓ the rows, Home/End
    /// reach the row's ends, Ctrl+Home/End the first and last row; Enter
    /// and Space press the row, whose active cell the view knows.
    pub fn cells(mut self, active: usize, count: usize) -> Self {
        self.cells = Some((active, count));
        self
    }
    /// ← → or Home/End picked cell `index` of the active row.
    pub fn on_move_cell(mut self, f: impl Fn(usize, &mut Window, &mut App) + 'static) -> Self {
        self.on_move_cell = Some(Box::new(f));
        self
    }
    /// The arrows wrap at the ends (a tab list, a radio group); the default
    /// stops there (a list box, a tree, a menu, a grid).
    pub fn wrap(mut self) -> Self {
        self.wrap = true;
        self
    }
    /// An arrow, Home or End picked item `index`: the view stores it and
    /// renders that item active.
    pub fn on_move(mut self, f: impl Fn(usize, &mut Window, &mut App) + 'static) -> Self {
        self.on_move = Some(Box::new(f));
        self
    }
    /// The items are the rows of a `uniform_list` tracking `handle`: a move
    /// scrolls the list to the row moved to (`Nearest`) before the view
    /// hears of it, so the frame that answers the key lowers that row and
    /// its claim, and a row off screen is scrolled to, not skipped.
    pub fn track_scroll(mut self, handle: &crate::UniformListScrollHandle) -> Self {
        self.scroll = Some(handle.clone());
        self
    }
    /// Enter or Space on the active item.
    pub fn on_press(mut self, f: impl Fn(usize, &mut Window, &mut App) + 'static) -> Self {
        self.on_press = Some(Box::new(f));
        self
    }
    /// The items, each an [`item`] (or a row holding them); a caller may
    /// add more on the built element.
    pub fn children(mut self, items: impl IntoIterator<Item = impl IntoElement>) -> Self {
        self.items
            .extend(items.into_iter().map(IntoElement::into_any_element));
        self
    }
    /// The element: roled, named, oriented, focusable (so a Tab stop) and
    /// hearing the keys of §3 — the orientation's arrows, Home/End, Enter
    /// and Space, unmodified; a grid's ↑ ↓ by a row and Ctrl+Home/End. A
    /// move the view answers by re-rendering is remembered until then, so
    /// a press in the same frame lands on the item just moved to.
    pub fn build(self) -> Stateful<Div> {
        let Self {
            element,
            items,
            orientation,
            columns,
            cells,
            active,
            count,
            wrap,
            scroll,
            on_move,
            on_move_cell,
            on_press,
        } = self;
        let on_move = on_move.map(|moved| -> Picked {
            match scroll {
                Some(scroll) => Box::new(move |index, window, app| {
                    scroll.scroll_to_item(index, ScrollStrategy::Nearest);
                    moved(index, window, app)
                }),
                None => moved,
            }
        });
        // the active item and cell as of the last key, ahead of the render
        // that shows the move
        let at = std::cell::Cell::new(active);
        let at_cell = std::cell::Cell::new(cells.map_or(0, |(cell, _)| cell));
        let keys = move |event: &KeyDownEvent, window: &mut Window, app: &mut App| {
            let keystroke = &event.keystroke;
            let modifiers = keystroke.modifiers;
            let plain = !modifiers.modified();
            let ctrl = modifiers.control
                && !(modifiers.alt || modifiers.shift || modifiers.platform || modifiers.function);
            let key = keystroke.key.as_str();
            if count == 0 {
                return;
            }
            let last = count - 1;
            let active = at.get();
            if plain && matches!(key, "enter" | "space") {
                if let Some(press) = &on_press {
                    press(active, window, app);
                }
                return;
            }
            // one step along the arrows' axis, or a row up or down a grid
            let step = |by: isize| -> Option<usize> {
                let to = active as isize + by;
                match (wrap, columns) {
                    (true, None) => Some(to.rem_euclid(count as isize) as usize),
                    _ => usize::try_from(to).ok().filter(|to| *to <= last),
                }
            };
            if let Some((_, cells)) = cells {
                let cell = at_cell.get();
                let last_cell = cells.saturating_sub(1);
                let (to_row, to_cell) = match (key, plain, ctrl) {
                    ("up", true, _) => (step(-1), None),
                    ("down", true, _) => (step(1), None),
                    ("left", true, _) => (None, cell.checked_sub(1)),
                    ("right", true, _) => (None, Some((cell + 1).min(last_cell))),
                    ("home", true, _) => (None, Some(0)),
                    ("end", true, _) => (None, Some(last_cell)),
                    ("home", _, true) => (Some(0), None),
                    ("end", _, true) => (Some(last), None),
                    _ => (None, None),
                };
                if let (Some(to), Some(moved)) = (to_row.filter(|to| *to != active), &on_move) {
                    at.set(to);
                    at_cell.set(0);
                    moved(to, window, app);
                }
                if let (Some(to), Some(moved)) = (to_cell.filter(|to| *to != cell), &on_move_cell) {
                    at_cell.set(to);
                    moved(to, window, app);
                }
                return;
            }
            let row_start = |columns: usize| active - active % columns;
            let next = match (key, plain, ctrl, orientation, columns) {
                ("left", true, _, Orientation::Horizontal, None)
                | ("up", true, _, Orientation::Vertical, None)
                | ("left", true, _, _, Some(_)) => step(-1),
                ("right", true, _, Orientation::Horizontal, None)
                | ("down", true, _, Orientation::Vertical, None)
                | ("right", true, _, _, Some(_)) => step(1),
                ("up", true, _, _, Some(columns)) => step(-(columns as isize)),
                ("down", true, _, _, Some(columns)) => step(columns as isize),
                ("home", true, _, _, None) | ("home", _, true, _, Some(_)) => Some(0),
                ("end", true, _, _, None) | ("end", _, true, _, Some(_)) => Some(last),
                ("home", true, _, _, Some(columns)) => Some(row_start(columns)),
                ("end", true, _, _, Some(columns)) => {
                    Some((row_start(columns) + columns - 1).min(last))
                }
                _ => None,
            };
            if let (Some(next), Some(moved)) = (next.filter(|next| *next != active), &on_move) {
                at.set(next);
                moved(next, window, app);
            }
        };
        element
            .aria_orientation(orientation)
            .focusable()
            .on_key_down(keys)
            .children(items)
    }
}

/// An item of a [`composite`]: its role, and the claim when it is the
/// active one. It keeps its `on_click` and the role's state
/// (`aria_selected`, `aria_toggled`) and is never focusable: the composite
/// holds the focus, so a control built focusable (a [`button`], a
/// [`link`]) leaves the Tab order here. A grid row is never an item:
/// the claim goes on a cell, or on the one control inside it.
pub fn item(mut element: Stateful<Div>, role: Role, active: bool) -> Stateful<Div> {
    let interactions = element.interactivity().interactions();
    interactions.focusable = false;
    interactions.tab_stop = None;
    element
        .role(role)
        .when(active, |item| item.aria_active_descendant())
}

/// A few choices side by side in one box, the picked one ink-filled: a
/// state filter, an object format, an invite's lifetime. `label` names the
/// choice, `choices` are each segment's id and label, `picked` the one
/// that is, or none (the arrows then start at the first); the box draws
/// the edge they share. A radio group: one Tab stop, and ← → pick the
/// next choice (`on_pick`), wrapping at the ends.
pub fn segmented(
    id: impl Into<ElementId>,
    label: impl Into<SharedString>,
    ladder: &Ladder,
    theme: &Theme,
    picked: Option<usize>,
    choices: impl IntoIterator<Item = (ElementId, SharedString)>,
    on_pick: impl Fn(usize, &mut Window, &mut App) + 'static,
) -> Stateful<Div> {
    let on_pick = std::rc::Rc::new(on_pick);
    let choices: Vec<_> = choices.into_iter().collect();
    let count = choices.len();
    let moved = on_pick.clone();
    let active = picked.unwrap_or(0);
    let segments = choices.into_iter().enumerate().map(|(index, (id, label))| {
        let pick = on_pick.clone();
        let click =
            move |_: &ClickEvent, window: &mut Window, app: &mut App| pick(index, window, app);
        item(
            segment(id, label, picked == Some(index), ladder, theme, click),
            Role::RadioButton,
            index == active,
        )
    });
    composite(id, Role::RadioGroup, label)
        .orientation(Orientation::Horizontal)
        .wrap()
        .active(active, count)
        .on_move(move |index, window, app| moved(index, window, app))
        .build()
        .flex()
        .flex_none()
        .items_center()
        .border_t_1()
        .border_b_1()
        .border_r_1()
        .border_color(theme.border_strong)
        .children(segments)
}

/// One choice of a [`segmented`] box.
fn segment(
    id: impl Into<ElementId>,
    label: impl Into<SharedString>,
    selected: bool,
    ladder: &Ladder,
    theme: &Theme,
    click: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
) -> Stateful<Div> {
    let theme = *theme;
    div()
        .id(id)
        .h(ladder.row)
        .px(space::MD)
        .flex()
        .items_center()
        .border_l_1()
        .border_color(theme.border_strong)
        .text_type(ladder.label)
        // the Expo kit's `Segmented`: the chosen one medium
        .when(ladder.touch && selected, |segment| {
            segment.font_weight(FontWeight::MEDIUM)
        })
        .whitespace_nowrap()
        .map(|segment| match selected {
            true => segment
                .bg(theme.primary)
                .text_color(theme.primary_foreground),
            false => segment
                .text_color(theme.muted)
                .hover(move |style| style.text_color(theme.foreground)),
        })
        .role(Role::RadioButton)
        .aria_toggled(selected.into())
        .on_click(click)
        .child(label.into())
}

/// An on/off switch: a pill with its knob at the on or the off end, in a
/// box no smaller than [`Ladder::hit`] each way.
pub fn switch(
    id: impl Into<ElementId>,
    label: impl Into<SharedString>,
    on: bool,
    enabled: bool,
    ladder: &Ladder,
    theme: &Theme,
    toggle: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
) -> Stateful<Div> {
    let knob = div().size(px(12.)).rounded_full().bg(theme.background);
    let pill = div()
        .w(px(28.))
        .h(px(16.))
        .flex_none()
        .flex()
        .items_center()
        .px(px(2.))
        .rounded_full()
        .bg(if on {
            theme.primary
        } else {
            theme.border_strong
        })
        .when(on, |pill| pill.justify_end())
        .child(knob);
    let element = div()
        .id(id)
        .min_w(ladder.hit)
        .min_h(ladder.hit)
        .flex_none()
        .flex()
        .items_center()
        .justify_center()
        .rounded_full()
        .role(Role::Switch)
        .aria_label(label.into())
        .aria_toggled(on.into())
        .child(pill);
    match enabled {
        // the hit box around the pill sits on the window's ground, so the
        // ring is the plain one whether the pill is ink or not
        true => focus_shown(
            element.cursor_pointer().focusable().on_click(toggle),
            theme,
            |style| style,
        ),
        false => element.opacity(0.5).aria_disabled(true),
    }
}

/// One setting: its title and a line about it on the left, its control on
/// the right, a hairline under it.
pub fn setting_row(
    id: impl Into<ElementId>,
    title: impl Into<SharedString>,
    description: impl Into<SharedString>,
    control: impl IntoElement,
    ladder: &Ladder,
    theme: &Theme,
) -> Stateful<Div> {
    div()
        .id(id)
        .w_full()
        .min_h(ladder.row)
        // wrapped, it grows; a column around it must not squeeze it
        .flex_none()
        .flex()
        .flex_wrap()
        .items_center()
        .gap(space::LG)
        .py(space::MD)
        .border_b_1()
        .border_color(theme.border)
        .child(
            div()
                .flex_1()
                .min_w(px(200.))
                .flex()
                .flex_col()
                .gap(space::HAIR)
                .child(
                    div()
                        .text_type(ladder.body)
                        .font_weight(FontWeight::MEDIUM)
                        .child(title.into()),
                )
                .child(
                    div()
                        .text_size(text::SECONDARY)
                        .text_color(theme.muted)
                        .child(description.into()),
                ),
        )
        .child(
            div()
                .flex_none()
                .flex()
                .items_center()
                .gap(space::SM)
                .child(control),
        )
}

/// A person's round initial at `size` (a [`Face`]: a ladder's rung, or a
/// side alone), on the raised surface. A caller recolours it (an agent, a
/// speaker) with `bg` / `text_color`.
pub fn avatar(name: &str, size: impl Into<Face>, theme: &Theme) -> Div {
    let Face { side, text, weight } = size.into();
    div()
        .size(side)
        .flex_shrink_0()
        .rounded_full()
        .flex()
        .items_center()
        .justify_center()
        .bg(theme.surface_raised)
        .text_color(theme.muted)
        .text_type(text)
        .when_some(weight, |face, weight| face.font_weight(weight))
        .child(initial(name))
}

/// How far an arrow key moves a [`divider`]; shift moves it four times as far.
const STEP: f32 = 8.;

/// The line between two panes, dragged to move it: `drag` takes the
/// horizontal delta (and clamps the layout it moves). `label` names it
/// ("Resize the room list"); focused, left and right move it by [`STEP`].
pub fn divider<V: 'static>(
    id: impl Into<ElementId>,
    label: impl Into<SharedString>,
    theme: &Theme,
    cx: &mut crate::Context<V>,
    drag: impl Fn(&mut V, f32) + 'static,
) -> crate::ResizeHandle {
    let drag = std::rc::Rc::new(drag);
    let dragged = cx.listener({
        let drag = drag.clone();
        move |view, delta: &(Pixels, Pixels), _window, cx| {
            drag(view, delta.0.into());
            cx.notify();
        }
    });
    let stepped = cx.listener(move |view, event: &KeyDownEvent, _window, cx| {
        let step = match event.keystroke.modifiers.shift {
            true => STEP * 4.,
            false => STEP,
        };
        let delta = match event.keystroke.key.as_str() {
            "left" => -step,
            "right" => step,
            _ => return,
        };
        drag(view, delta);
        cx.notify();
    });
    crate::resize_handle(id, div().w(crate::px(1.)).h_full().bg(theme.border))
        .on_drag(dragged)
        .role(Role::Splitter)
        .aria_label(label)
        .aria_orientation(gpui::Orientation::Vertical)
        .focusable()
        .tab_stop(true)
        // the line covers an inset ring whole: this one sits outside it
        .focus_visible(|style| style.shadow(vec![ring_shadow(theme.accent, 2., false)]))
        .on_key_down(stepped)
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
    ladder: &Ladder,
    foreground: Hsla,
    background: Hsla,
) -> Stateful<Div> {
    let tag = div().id(id).bg(background).text_color(foreground);
    match ladder.touch {
        // the Expo kit's `Badge`: 20 tall, 6 in, tiny medium
        true => tag
            .h(px(::design::height::touch::BADGE as f32))
            .flex()
            .flex_none()
            .items_center()
            .px(space::XS)
            .text_type(ladder.tiny)
            .font_weight(FontWeight::MEDIUM),
        false => tag.px_1().py_0p5().text_type(ladder.caption),
    }
    .child(label.into())
}

/// A link: subdued mono text that underlines under the pointer and opens
/// `href` through `link.open`, in a box no smaller than [`Ladder::hit`]
/// with the text centred down it. Its text is its name; a caller that
/// says more adds `.aria_label(..)`. Its click consumes the press: a
/// clickable card under it does not hear it.
pub fn link(
    id: impl Into<ElementId>,
    label: impl Into<SharedString>,
    href: impl Into<String>,
    ladder: &Ladder,
    theme: &Theme,
) -> Stateful<Div> {
    let theme = *theme;
    let href = href.into();
    div()
        .id(id)
        .min_w(ladder.hit)
        .min_h(ladder.hit)
        .flex()
        .items_center()
        .text_type(ladder.caption)
        .text_color(theme.muted)
        .font_family(fonts::FAMILY_MONO)
        .whitespace_nowrap()
        .cursor_pointer()
        .hover(move |style| style.text_color(theme.foreground).text_decoration_1())
        .role(Role::Link)
        .focusable()
        .on_click(move |_: &ClickEvent, _: &mut Window, cx: &mut App| cx.host().open_link(&href))
        .consumes_click()
        .consumes_keys(["enter", "space"])
        .child(label.into())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::wire;
    use gpui::Toggled;

    fn lower(element: impl IntoElement) -> wire::Node {
        lower_styled(element).0
    }

    /// The lowered element, and the table its nodes' styles are in.
    fn lower_styled(element: impl IntoElement) -> (wire::Node, wire::Styles) {
        crate::testing::lower(|_: &mut Context<()>| element)
    }

    fn interactivity(node: &wire::Node) -> &wire::Interactivity {
        node.interactivity()
            .unwrap_or_else(|| panic!("no interactivity: {node:?}"))
    }

    fn faults(node: &wire::Node) -> Vec<wire::FaultKind> {
        wire::audit(node)
            .into_iter()
            .map(|fault| fault.kind)
            .collect()
    }

    /// A glyph alone names nothing: the button is named in words, and is
    /// at least a press target each way.
    #[test]
    fn an_icon_button_is_a_focusable_button_named_in_words_and_a_press_target() {
        let theme = Theme::light();
        let (node, styles) = lower_styled(icon_button(
            "close",
            "✕",
            "Close",
            &POINTER,
            &theme,
            |_, _, _| {},
        ));
        let control = interactivity(&node);
        assert_eq!(control.role, Some(Role::Button));
        assert_eq!(control.aria.label.as_deref(), Some("Close"));
        assert!(control.focusable && control.on_click.is_some());
        assert_eq!(faults(&node), []);
        let style = &styles[node.style().expect("a styled container")];
        let floor = Some(POINTER.hit.into());
        assert_eq!(
            (style.min_size.width, style.min_size.height),
            (floor, floor)
        );
    }

    /// The finger's ladder is the Expo app's theme (`theme.ts` `type`,
    /// `size`), number for number.
    #[test]
    fn the_touch_ladder_is_the_expo_theme() {
        let lined = |size: f32, line: f32| Type {
            size: px(size),
            line: Some(px(line)),
        };
        let t = TOUCH;
        assert_eq!(
            [
                t.header,
                t.row,
                t.sheet_row,
                t.control,
                t.hit,
                t.icon,
                t.icon_row,
                t.icon_sheet
            ],
            [56., 48., 52., 44., 44., 22., 18., 20.].map(px)
        );
        assert_eq!(
            [
                t.big_title,
                t.heading,
                t.title,
                t.body,
                t.label,
                t.small,
                t.secondary,
                t.caption,
                t.tiny,
                t.code
            ],
            [
                (22., 27.),
                (20., 24.),
                (17., 21.),
                (15., 21.),
                (15., 21.),
                (14., 19.),
                (13., 18.),
                (12., 16.),
                (11., 14.),
                (12., 20.)
            ]
            .map(|(size, line)| lined(size, line))
        );
        let faces = [t.avatar_inline, t.avatar, t.avatar_sheet, t.avatar_detail];
        assert_eq!(
            faces.map(|face| (face.side, face.text.size, face.weight)),
            [(24., 11.), (32., 13.), (40., 16.), (56., 22.)].map(|(side, text)| (
                px(side),
                px(text),
                Some(FontWeight::MEDIUM)
            ))
        );
        // a pointer's roles name no line height: the desktop's text stands
        assert!(
            [POINTER.title, POINTER.body, POINTER.label, POINTER.caption]
                .iter()
                .all(|role| role.line.is_none())
        );
    }

    /// Under a finger an icon button is the kit's `Hit`: 44 each way, the
    /// glyph in the foreground; filled it is the ink square, disabled it
    /// is faint and hears no press.
    #[test]
    fn under_touch_an_icon_button_is_the_kits_hit() {
        let theme = Theme::light();
        let send = |filled: bool| {
            lower_styled(
                icon_button(
                    "send",
                    icon("send-horizontal", TOUCH.icon),
                    "Send",
                    &TOUCH,
                    &theme,
                    |_, _, _| {},
                )
                .filled(filled)
                .enabled(filled),
            )
        };
        let (node, styles) = send(true);
        let style = &styles[node.style().expect("a styled container")];
        assert_eq!(
            (style.size.width, style.size.height),
            (Some(px(44.).into()), Some(px(44.).into()))
        );
        assert_eq!(style.background, Some(theme.primary.into()));
        assert_eq!(style.text.color, Some(theme.primary_foreground));
        assert!(interactivity(&node).on_click.is_some());
        let (node, styles) = send(false);
        let style = &styles[node.style().expect("a styled container")];
        assert_eq!(style.background, None);
        assert_eq!(style.text.color, Some(theme.faint));
        let control = interactivity(&node);
        assert!(control.on_click.is_none() && control.aria.disabled == Some(true));
        assert_eq!(faults(&node), []);
    }

    /// Under a finger a button is the kit's: 44 tall (48 the primary), an
    /// outline for a plain one, the ink fill for the primary.
    #[test]
    fn under_touch_a_button_is_the_kits_button() {
        let theme = Theme::light();
        let style = |kind: Kind| {
            let (node, styles) =
                lower_styled(button("save", "Save", &TOUCH, &theme, |_, _, _| {}).kind(kind));
            styles[node.style().expect("a styled container")].clone()
        };
        let plain = style(Kind::Plain);
        assert_eq!(plain.size.height, Some(px(44.).into()));
        assert_eq!(plain.border_color, Some(theme.border_strong));
        assert_eq!(plain.background, None);
        let primary = style(Kind::Primary);
        assert_eq!(primary.size.height, Some(px(48.).into()));
        assert_eq!(primary.background, Some(theme.primary.into()));
    }

    /// What the Expo kit sets `weight="medium"` asks for 500 under a
    /// finger (the app draws Instrument Sans Medium); a pointer's weights
    /// are the desktop's.
    #[test]
    fn under_touch_the_kits_medium_roles_ask_for_500() {
        let theme = Theme::light();
        let weight = |(node, styles): (wire::Node, wire::Styles)| {
            styles[node.style().expect("a styled node")]
                .text
                .font_weight
        };
        let medium = Some(FontWeight::MEDIUM);
        let label = |kind: Kind| {
            weight(lower_styled(
                button("save", "Save", &TOUCH, &theme, |_, _, _| {}).kind(kind),
            ))
        };
        assert_eq!(label(Kind::Plain), medium);
        assert_eq!(label(Kind::Quiet), None);
        let tag = |ladder: &Ladder| {
            weight(lower_styled(badge(
                "n",
                "3",
                ladder,
                theme.foreground,
                theme.surface,
            )))
        };
        assert_eq!(tag(&TOUCH), medium);
        let chosen = |ladder: &Ladder| {
            let (node, styles) = lower_styled(segmented(
                "mode",
                "Mode",
                ladder,
                &theme,
                Some(1),
                [("a".into(), "A".into()), ("b".into(), "B".into())],
                |_, _, _| {},
            ));
            let picked = node.children()[1].clone();
            [
                weight((node.children()[0].clone(), styles.clone())),
                weight((picked, styles)),
            ]
        };
        assert_eq!(chosen(&TOUCH), [None, medium]);
        assert_eq!(chosen(&POINTER), [None, None]);
        let retry = |ladder: &Ladder| {
            let (node, styles) = lower_styled(refused("feed", "No.", ladder, &theme, |_, _, _| {}));
            weight((node.children()[1].clone(), styles))
        };
        assert_eq!(retry(&TOUCH), medium);
        assert_eq!(retry(&POINTER), None);
    }

    #[test]
    fn a_segmented_choice_is_a_radio_group_with_its_name() {
        let theme = Theme::light();
        let node = lower(segmented(
            "format",
            "Object format",
            &POINTER,
            &theme,
            Some(0),
            [("sha1".into(), "SHA-1".into())],
            |_, _, _| {},
        ));
        let group = interactivity(&node);
        assert_eq!(group.role, Some(Role::RadioGroup));
        assert_eq!(group.aria.label.as_deref(), Some("Object format"));
        assert!(group.focusable && group.on_key_down.is_some());
        assert_eq!(faults(&node), []);
    }

    #[test]
    fn a_focusable_node_shows_the_focus_ring_unless_it_draws_its_own() {
        let theme = Theme::light();
        // the style a node shows while it holds the keyboard
        let shown = |element: crate::AnyElement| {
            let (node, styles) = lower_styled(element);
            node.interactivity()
                .and_then(|interactivity| interactivity.focus_visible)
                .map(|style| styles[style].clone())
        };
        let plain = button("save", "Save", &POINTER, &theme, |_, _, _| {});
        assert_eq!(
            shown(plain.into_any_element()),
            Some(focus_ring(theme.accent))
        );
        let ink = button("send", "Send", &POINTER, &theme, |_, _, _| {}).kind(Kind::Primary);
        assert_eq!(
            shown(ink.into_any_element()),
            Some(focus_ring(theme.primary_foreground))
        );
        let own = div()
            .id("menu")
            .focusable()
            .focus_visible(|style| style.opacity(0.5));
        assert_eq!(
            shown(own.into_any_element()),
            Some(StyleRefinement::default().opacity(0.5))
        );
        let still = div().id("box").child("text");
        assert_eq!(shown(still.into_any_element()), None);
        // a frame that adds to the ring keeps its own shadow beside it
        let frame = focus_shown(div().id("frame").focusable(), &theme, |style| {
            style.shadow_lg()
        });
        let shown = shown(frame.into_any_element()).unwrap();
        assert_eq!(shown.border_color, Some(theme.accent));
        let shadows = shown.box_shadow.unwrap();
        assert_eq!(shadows.len(), 3);
        assert_eq!(shadows[0], ring_shadow(theme.accent, 2., true));
    }

    #[test]
    fn a_switch_that_is_on_reports_toggled_true() {
        let theme = Theme::light();
        for (on, toggled) in [(true, Toggled::True), (false, Toggled::False)] {
            let node = lower(switch(
                "dark",
                "Dark",
                on,
                true,
                &POINTER,
                &theme,
                |_, _, _| {},
            ));
            let control = interactivity(&node);
            assert_eq!(control.role, Some(Role::Switch));
            assert_eq!(control.aria.toggled, Some(toggled));
            assert_eq!(control.aria.selected, None);
            assert_eq!(faults(&node), []);
        }
    }

    #[test]
    fn the_picked_segment_reports_toggled_true_and_is_the_active_one() {
        let theme = Theme::light();
        let node = lower(segment(
            "sha1",
            "SHA-1",
            true,
            &POINTER,
            &theme,
            |_, _, _| {},
        ));
        let control = interactivity(&node);
        assert_eq!(control.role, Some(Role::RadioButton));
        assert_eq!(control.aria.toggled, Some(Toggled::True));
        assert_eq!(control.aria.selected, None);
        assert!(!control.focusable);
        let group = lower(segmented(
            "format",
            "Object format",
            &POINTER,
            &theme,
            Some(1),
            [
                ("sha256".into(), "SHA-256".into()),
                ("sha1".into(), "SHA-1".into()),
            ],
            |_, _, _| {},
        ));
        let claims: Vec<bool> = group
            .children()
            .iter()
            .map(|segment| interactivity(segment).aria.active_descendant)
            .collect();
        assert_eq!(claims, [false, true]);
    }

    #[test]
    fn a_button_is_a_toggle_only_once_told_it_is_selected() {
        let theme = Theme::light();
        let plain = lower(button("save", "Save", &POINTER, &theme, |_, _, _| {}));
        assert_eq!(interactivity(&plain).aria.toggled, None);
        for (selected, toggled) in [(true, Toggled::True), (false, Toggled::False)] {
            let node =
                lower(button("tree", "Tree", &POINTER, &theme, |_, _, _| {}).selected(selected));
            let control = interactivity(&node);
            assert_eq!(control.role, Some(Role::Button));
            assert_eq!(control.aria.toggled, Some(toggled));
            assert_eq!(control.aria.selected, None);
        }
    }

    #[derive(Default, serde::Serialize, serde::Deserialize)]
    struct Panes {
        moved: Vec<f32>,
    }

    impl crate::View for Panes {
        const NAME: &'static str = "Panes";
        fn new(_: &mut Window, _: &mut crate::Context<Self>) -> Self {
            Self::default()
        }
    }

    impl crate::Render for Panes {
        fn render(&mut self, _: &mut Window, cx: &mut crate::Context<Self>) -> impl IntoElement {
            let theme = Theme::light();
            divider(
                "panes-resize",
                "Resize the list",
                &theme,
                cx,
                |panes: &mut Self, dx| panes.moved.push(dx),
            )
        }
    }

    #[test]
    fn a_divider_is_a_named_focusable_splitter_the_arrows_move() {
        let mut cx = crate::testing::TestAppContext::new();
        let panes = cx.open::<Panes>();
        let node = cx.find("panes-resize").expect("the divider").clone();
        let handle = interactivity(&node);
        assert_eq!(handle.role, Some(Role::Splitter));
        assert_eq!(handle.aria.label.as_deref(), Some("Resize the list"));
        assert_eq!(handle.aria.orientation, Some(gpui::Orientation::Vertical));
        assert!(handle.focusable && handle.tab_stop == Some(true));
        assert_eq!(faults(&node), []);
        cx.simulate_focus("panes-resize");
        for keystroke in ["left", "right", "shift-left", "shift-right", "up", "a"] {
            cx.simulate_key_down("panes-resize", keystroke);
        }
        cx.simulate_drag("panes-resize", 5., 0.);
        panes.read(|panes| assert_eq!(panes.moved, [-8., 8., -32., 32., 5.]));
    }

    #[derive(Default, serde::Serialize, serde::Deserialize)]
    struct Format {
        picked: usize,
    }

    impl crate::View for Format {
        const NAME: &'static str = "Format";
        fn new(_: &mut Window, _: &mut crate::Context<Self>) -> Self {
            Self::default()
        }
    }

    impl crate::Render for Format {
        fn render(&mut self, _: &mut Window, cx: &mut crate::Context<Self>) -> impl IntoElement {
            segmented(
                "format",
                "Object format",
                &POINTER,
                &Theme::light(),
                Some(self.picked),
                [
                    ("sha256".into(), "SHA-256".into()),
                    ("sha1".into(), "SHA-1".into()),
                ],
                cx.processor(|view: &mut Self, index, _, cx| {
                    view.picked = index;
                    cx.notify();
                }),
            )
        }
    }

    #[test]
    fn a_segmented_choice_checks_the_next_choice_on_an_arrow() {
        let mut cx = crate::testing::TestAppContext::new();
        let format = cx.open::<Format>();
        cx.simulate_focus("format");
        cx.simulate_key_down("format", "right");
        format.read(|view| assert_eq!(view.picked, 1));
        assert_eq!(
            interactivity(cx.find("sha1").expect("the picked segment"))
                .aria
                .toggled,
            Some(Toggled::True)
        );
        // a radio group wraps
        cx.simulate_key_down("format", "right");
        format.read(|view| assert_eq!(view.picked, 0));
        cx.simulate_click("sha1");
        format.read(|view| assert_eq!(view.picked, 1));
    }

    thread_local! {
        /// The moves and presses of a `silent` [`Picker`], which keeps no
        /// state of them and so never re-renders.
        static SILENT: std::cell::RefCell<(Vec<usize>, Vec<usize>)> = Default::default();
    }

    /// A composite of `count` items under one test's settings.
    #[derive(Default, serde::Serialize, serde::Deserialize)]
    struct Picker {
        /// moves and presses go to [`SILENT`], and the view does not re-render
        silent: bool,
        role: Option<Role>,
        horizontal: bool,
        columns: Option<usize>,
        /// the active row's cells, and the active one
        cells: Option<(usize, usize)>,
        wrap: bool,
        active: usize,
        count: usize,
        moved: Vec<usize>,
        moved_cell: Vec<usize>,
        pressed: Vec<usize>,
        /// a focusable link inside the composite, beside its items
        link: bool,
    }

    impl crate::View for Picker {
        const NAME: &'static str = "Picker";
        const CAPABILITIES: &'static [crate::methods::Capability] =
            &[crate::methods::Capability::Link];
        fn new(_: &mut Window, _: &mut crate::Context<Self>) -> Self {
            Self {
                role: Some(Role::ListBox),
                count: 3,
                ..Self::default()
            }
        }
    }

    impl crate::Render for Picker {
        fn render(&mut self, _: &mut Window, cx: &mut crate::Context<Self>) -> impl IntoElement {
            let role = self.role.unwrap_or(Role::ListBox);
            let mut list = composite("picker", role, "Pick one")
                .active(self.active, self.count)
                .on_move(cx.processor(|view: &mut Self, index, _, cx| {
                    if view.silent {
                        return SILENT.with_borrow_mut(|log| log.0.push(index));
                    }
                    view.moved.push(index);
                    view.active = index;
                    cx.notify();
                }))
                .on_press(cx.processor(|view: &mut Self, index, _, cx| {
                    if view.silent {
                        return SILENT.with_borrow_mut(|log| log.1.push(index));
                    }
                    view.pressed.push(index);
                    cx.notify();
                }));
            if self.horizontal {
                list = list.orientation(Orientation::Horizontal);
            }
            if let Some(columns) = self.columns {
                list = list.grid(columns);
            }
            if let Some((cell, cells)) = self.cells {
                list = list.cells(cell, cells).on_move_cell(cx.processor(
                    |view: &mut Self, index, _, cx| {
                        if view.silent {
                            return SILENT.with_borrow_mut(|log| log.0.push(index));
                        }
                        view.moved_cell.push(index);
                        view.cells = view.cells.map(|(_, cells)| (index, cells));
                        cx.notify();
                    },
                ));
            }
            if self.wrap {
                list = list.wrap();
            }
            let item_role = match role {
                Role::TabList => Role::Tab,
                Role::Grid => Role::GridCell,
                _ => Role::ListBoxOption,
            };
            let active = self.active;
            let link = self.link;
            list.build()
                .children((0..self.count).map(move |index| {
                    let row = div()
                        .id(format!("pick-{index}"))
                        .on_click(|_, _, _| {})
                        .child(format!("Choice {index}"));
                    let row = match item_role {
                        Role::GridCell => row,
                        _ => row.aria_selected(index == active),
                    };
                    let cell = item(row, item_role, index == active);
                    match item_role {
                        Role::GridCell => {
                            div().id(format!("row-{index}")).role(Role::Row).child(cell)
                        }
                        _ => cell,
                    }
                }))
                .when(link, |list| {
                    list.child(super::link(
                        "inside",
                        "block 12",
                        "duck://a/b",
                        &POINTER,
                        &Theme::light(),
                    ))
                })
        }
    }

    fn picker(
        set: impl FnOnce(&mut Picker),
    ) -> (crate::testing::TestAppContext, crate::Entity<Picker>) {
        let mut cx = crate::testing::TestAppContext::new();
        let picker = cx.open::<Picker>();
        cx.update(&picker, |view, _, cx| {
            set(view);
            cx.notify();
        });
        cx.run_until_parked();
        // a person Tabs to the composite: its one Tab stop
        cx.simulate_focus("picker");
        (cx, picker)
    }

    fn found(cx: &crate::testing::TestAppContext, key: &str) -> wire::Interactivity {
        interactivity(cx.find(key).expect(key)).clone()
    }

    #[test]
    fn a_composite_is_one_tab_stop_whose_arrows_pick_its_items() {
        let (mut cx, picker) = picker(|_| {});
        let list = found(&cx, "picker");
        assert_eq!(list.role, Some(Role::ListBox));
        assert_eq!(list.aria.label.as_deref(), Some("Pick one"));
        assert_eq!(list.aria.orientation, Some(Orientation::Vertical));
        assert!(list.focusable && list.tab_stop == Some(true));
        assert!(list.on_key_down.is_some());
        for index in 0..3 {
            let row = found(&cx, &format!("pick-{index}"));
            assert!(!row.focusable && row.tab_stop.is_none());
            assert_eq!(row.aria.active_descendant, index == 0);
        }
        cx.simulate_key_down("picker", "down");
        assert!(found(&cx, "pick-1").aria.active_descendant);
        cx.simulate_key_down("picker", "end");
        cx.simulate_key_down("picker", "left");
        cx.simulate_key_down("picker", "enter");
        cx.simulate_key_down("picker", "home");
        cx.simulate_key_down("picker", "up");
        cx.simulate_key_down("picker", "space");
        cx.simulate_key_down("picker", "a");
        picker.read(|view| {
            assert_eq!(view.moved, [1, 2, 0]);
            assert_eq!(view.pressed, [2, 0]);
        });
    }

    /// A link inside a composite consumes the keys that press it:
    /// Space or Enter on it opens the link (gpui's click on the key's way
    /// up) and never reaches the composite around it, which would press its
    /// own active item too.
    #[test]
    fn a_link_inside_a_composite_keeps_its_press_from_it() {
        let (mut cx, picker) = picker(|view| view.link = true);
        cx.simulate_focus("inside");
        cx.simulate_key_down("inside", "space");
        cx.simulate_key_down("inside", "enter");
        picker.read(|view| assert_eq!(view.pressed, Vec::<usize>::new()));
        // a press opens the href it was given
        cx.simulate_click("inside");
        assert_eq!(
            cx.host().requests::<crate::methods::LinkOpen>(),
            ["duck://a/b"]
        );
        // the composite's own Enter still presses
        cx.simulate_focus("picker");
        cx.simulate_key_down("picker", "enter");
        picker.read(|view| assert_eq!(view.pressed, [0]));
    }

    /// The helper remembers where the arrows went before the view draws
    /// it: ↓ then Enter in one frame presses the item moved to.
    #[test]
    fn a_press_right_after_a_move_lands_on_the_item_moved_to() {
        SILENT.take();
        let (mut cx, _) = picker(|view| view.silent = true);
        cx.simulate_key_down("picker", "down");
        cx.simulate_key_down("picker", "enter");
        assert_eq!(SILENT.take(), (vec![1], vec![1]));
        // a grid of rows with cells: the cell moves, then the row (its
        // cell back at 0), then the cell again
        let (mut cx, _) = picker(|view| {
            view.silent = true;
            view.role = Some(Role::Grid);
            view.cells = Some((0, 3));
            view.count = 2;
        });
        for keystroke in ["right", "right", "down", "right", "enter"] {
            cx.simulate_key_down("picker", keystroke);
        }
        assert_eq!(SILENT.take(), (vec![1, 2, 1, 1], vec![1]));
    }

    /// A control built focusable (a button, a link) made an item leaves
    /// the Tab order, keeps its role and its press, and claims when active.
    #[test]
    fn a_focusable_control_as_an_item_leaves_the_tab_order_and_claims() {
        let theme = Theme::light();
        let cells = [
            (
                lower(button("compare", "Compare", &POINTER, &theme, |_, _, _| {}).item(true)),
                Role::Button,
            ),
            (
                lower(item(
                    link("activity", "block 12", "duck://a/b", &POINTER, &theme),
                    Role::Link,
                    true,
                )),
                Role::Link,
            ),
        ];
        for (cell, role) in cells {
            let control = interactivity(&cell);
            assert!(!control.focusable && control.tab_stop.is_none(), "{role:?}");
            assert!(control.aria.active_descendant && control.on_click.is_some());
            assert_eq!(control.role, Some(role));
        }
    }

    #[test]
    fn a_tab_list_wraps_and_a_list_box_stops() {
        let (mut cx, tabs) = picker(|view| {
            view.role = Some(Role::TabList);
            view.horizontal = true;
            view.wrap = true;
            view.active = 2;
        });
        assert_eq!(
            found(&cx, "picker").aria.orientation,
            Some(Orientation::Horizontal)
        );
        cx.simulate_key_down("picker", "right");
        cx.simulate_key_down("picker", "left");
        cx.simulate_key_down("picker", "down");
        tabs.read(|view| assert_eq!(view.moved, [0, 2]));
        let (mut cx, list) = picker(|view| view.active = 2);
        cx.simulate_key_down("picker", "down");
        cx.simulate_key_down("picker", "home");
        cx.simulate_key_down("picker", "up");
        list.read(|view| assert_eq!(view.moved, [0]));
    }

    #[test]
    fn a_modified_arrow_is_not_the_composites() {
        let (mut cx, list) = picker(|_| {});
        for keystroke in [
            "alt-down",
            "cmd-down",
            "shift-down",
            "ctrl-down",
            "ctrl-end",
        ] {
            cx.simulate_key_down("picker", keystroke);
        }
        cx.simulate_key_down("picker", "shift-enter");
        list.read(|view| assert!(view.moved.is_empty() && view.pressed.is_empty()));
    }

    #[test]
    fn a_grid_steps_a_row_by_its_columns_and_home_end_by_its_row() {
        let (mut cx, grid) = picker(|view| {
            view.role = Some(Role::Grid);
            view.columns = Some(3);
            view.count = 8;
            view.active = 4;
        });
        for keystroke in [
            "down",
            "up",
            "up",
            "down",
            "home",
            "end",
            "right",
            "left",
            "ctrl-home",
            "ctrl-end",
            "down",
            "end",
        ] {
            cx.simulate_key_down("picker", keystroke);
        }
        grid.read(|view| assert_eq!(view.moved, [7, 4, 1, 4, 3, 5, 6, 5, 0, 7]));
        assert!(found(&cx, "pick-7").aria.active_descendant);
    }

    #[test]
    fn a_grid_of_rows_with_their_own_cells_steps_cells_sideways_and_rows_up_and_down() {
        let (mut cx, grid) = picker(|view| {
            view.role = Some(Role::Grid);
            view.cells = Some((0, 3));
            view.count = 4;
            view.active = 1;
        });
        for keystroke in [
            "right",
            "right",
            "right",
            "down",
            "home",
            "left",
            "end",
            "up",
            "up",
            "up",
            "ctrl-end",
            "ctrl-home",
            "enter",
        ] {
            cx.simulate_key_down("picker", keystroke);
        }
        grid.read(|view| {
            assert_eq!(view.moved_cell, [1, 2, 0, 2]);
            assert_eq!(view.moved, [2, 1, 0, 3, 0]);
            assert_eq!(view.pressed, [0]);
        });
    }
}
