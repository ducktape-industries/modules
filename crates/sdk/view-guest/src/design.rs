//! The shapes every view repeats, in one visual language: the design tokens
//! (re-exported whole), the type scale, heights and spacing as [`Pixels`]
//! (`text`, `size`, `space`), and the empty state, refused-with-retry
//! screen, quiet line, heading and mono run views used to copy between
//! them, and the controls: buttons, tabs, the segmented choice, the switch
//! and the row it sits in, and the pane divider. The number and time
//! formatters (`format.rs`) and Explorer's link paths (`explorer.rs`) sit
//! beside them for the same reason.
pub use ::design::*;

pub mod explorer;
mod format;
pub use format::{ago, clock, date, day, grouped, initial, local, plural, set_utc_offset};

use crate::prelude::*;
use crate::{BoxShadow, Div, FontWeight, Hsla, Pixels, Stateful, StyleRefinement};
pub use gpui::Orientation;

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

/// The focus ring: `color`, 2 px, just inside the edge, on a control the
/// keyboard has reached. The SDK draws it on every focusable node that has
/// no `focus_visible` of its own; a control whose own edge is already ink
/// shows it in the ink's foreground ([`focus_shown_on_ink`]).
pub fn focus_ring(color: Hsla) -> StyleRefinement {
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
    element.focus_visible(|_| style(focus_ring(theme.accent)))
}

/// The focus ring on an ink-filled control (a primary button, a switch that
/// is on): the ink's foreground, since ink on ink shows nothing.
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
    theme: &Theme,
    click: F,
) -> Button<F>
where
    F: Fn(&ClickEvent, &mut Window, &mut App) + 'static,
{
    Button {
        id: id.into(),
        label: label.into(),
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
        let theme = self.theme;
        let selected = self.selected == Some(true);
        let mut element = div()
            .id(self.id)
            .px_2()
            .py_1()
            .text_size(text::SECONDARY)
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

/// A control drawn as a glyph alone (a cross, a plus): muted until the
/// pointer is on it. `name` is what it does, in words, since the glyph
/// says nothing to a screen reader. The box is no smaller than
/// [`PRESS_TARGET`] each way, the glyph centred in it.
pub fn icon_button(
    id: impl Into<ElementId>,
    glyph: impl IntoElement,
    name: impl Into<SharedString>,
    theme: &Theme,
    click: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
) -> Stateful<Div> {
    let theme = *theme;
    div()
        .id(id)
        .min_w(PRESS_TARGET)
        .min_h(PRESS_TARGET)
        .flex()
        .items_center()
        .justify_center()
        .px_1()
        .text_color(theme.muted)
        .cursor_pointer()
        .hover(move |style| style.text_color(theme.foreground))
        .role(Role::Button)
        .aria_label(name)
        .focusable()
        .on_click(click)
        .child(glyph)
}

/// A tab: quiet text, the chosen one fg and underlined, no fill. A caller
/// sizes it to its bar (`h_full`, `flex_1`) and may label a glyph. It is
/// an [`item`] of a `TabList` [`composite`], which holds the focus and the
/// arrows: the caller wraps it in `item(.., Role::Tab, active)`.
pub fn tab(
    id: impl Into<ElementId>,
    label: impl Into<SharedString>,
    selected: bool,
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
        .text_size(text::SECONDARY)
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
        orientation: Orientation::Vertical,
        columns: None,
        cells: None,
        active: 0,
        count: 0,
        wrap: false,
        on_move: None,
        on_move_cell: None,
        on_press: None,
    }
}

type Picked = Box<dyn Fn(usize, &mut Window, &mut App)>;

pub struct Composite {
    element: Stateful<Div>,
    orientation: Orientation,
    columns: Option<usize>,
    /// the active row's active cell among its cells
    cells: Option<(usize, usize)>,
    active: usize,
    count: usize,
    wrap: bool,
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
    /// Enter or Space on the active item.
    pub fn on_press(mut self, f: impl Fn(usize, &mut Window, &mut App) + 'static) -> Self {
        self.on_press = Some(Box::new(f));
        self
    }
    pub fn build(self) -> Stateful<Div> {
        let Self {
            element,
            orientation,
            columns,
            cells,
            active,
            count,
            wrap,
            on_move,
            on_move_cell,
            on_press,
        } = self;
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
            if let Some((cell, cells)) = cells {
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
                    moved(to, window, app);
                }
                if let (Some(to), Some(moved)) = (to_cell.filter(|to| *to != cell), &on_move_cell) {
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
                moved(next, window, app);
            }
        };
        element
            .aria_orientation(orientation)
            .focusable()
            .on_key_down(keys)
    }
}

/// An item of a [`composite`]: its role, and the claim when it is the
/// active one. It keeps its `on_click` and the role's state
/// (`aria_selected`, `aria_toggled`) and is never focusable: the composite
/// holds the focus, so a control built focusable (a [`button`], a
/// [`block_link`]) leaves the Tab order here. A grid row is never an item:
/// the claim goes on a cell, or on the one control inside it.
pub fn item(mut element: Stateful<Div>, role: Role, active: bool) -> Stateful<Div> {
    element.interactivity().focusable = false;
    element.interactivity().tab_stop = None;
    element
        .role(role)
        .when(active, |item| item.aria_active_descendant())
}

/// A few choices side by side in one box, the picked one ink-filled: a
/// state filter, an object format, an invite's lifetime. `label` names the
/// choice, `choices` are each segment's id and label, `picked` the one
/// that is; the box draws the edge they share. A radio group: one Tab
/// stop, and ← → pick the next choice (`on_pick`), wrapping at the ends.
pub fn segmented(
    id: impl Into<ElementId>,
    label: impl Into<SharedString>,
    theme: &Theme,
    picked: usize,
    choices: impl IntoIterator<Item = (ElementId, SharedString)>,
    on_pick: impl Fn(usize, &mut Window, &mut App) + 'static,
) -> Stateful<Div> {
    let on_pick = std::rc::Rc::new(on_pick);
    let choices: Vec<_> = choices.into_iter().collect();
    let count = choices.len();
    let moved = on_pick.clone();
    let segments = choices.into_iter().enumerate().map(|(index, (id, label))| {
        let pick = on_pick.clone();
        let click =
            move |_: &ClickEvent, window: &mut Window, app: &mut App| pick(index, window, app);
        item(
            segment(id, label, index == picked, theme, click),
            Role::RadioButton,
            index == picked,
        )
    });
    composite(id, Role::RadioGroup, label)
        .orientation(Orientation::Horizontal)
        .wrap()
        .active(picked, count)
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
    theme: &Theme,
    click: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
) -> Stateful<Div> {
    let theme = *theme;
    div()
        .id(id)
        .h(size::ROW)
        .px(space::MD)
        .flex()
        .items_center()
        .border_l_1()
        .border_color(theme.border_strong)
        .text_size(text::SECONDARY)
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

/// An on/off switch: a pill with its knob at the on or the off end.
pub fn switch(
    id: impl Into<ElementId>,
    label: impl Into<SharedString>,
    on: bool,
    enabled: bool,
    theme: &Theme,
    toggle: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
) -> Stateful<Div> {
    let knob = div().size(px(12.)).rounded_full().bg(theme.background);
    let element = div()
        .id(id)
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
        .role(Role::Switch)
        .aria_label(label.into())
        .aria_toggled(on.into())
        .child(knob);
    match enabled {
        true => element
            .cursor_pointer()
            .focusable()
            .on_click(toggle)
            .when(on, |pill| focus_shown_on_ink(pill, theme)),
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
    theme: &Theme,
) -> Stateful<Div> {
    div()
        .id(id)
        .w_full()
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
                        .text_size(text::BODY)
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

/// How far an arrow key moves a [`divider`]; shift moves it four times as far.
const STEP: f32 = 8.;

/// The line between two panes, dragged to move it: `drag` takes the
/// horizontal delta (and clamps the layout it moves). `label` names it
/// ("Resize the room list"); focused, left and right move it by [`STEP`].
pub fn divider<V: crate::View>(
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

/// The smallest box a pointer presses, each way (the door's AX-017).
const PRESS_TARGET: Pixels = px(24.);

/// Subdued mono text that underlines under the pointer and opens
/// Explorer at `path` through `link.open`, in a box no smaller than
/// [`PRESS_TARGET`] with the text centred down it.
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
        .min_w(PRESS_TARGET)
        .min_h(PRESS_TARGET)
        .flex()
        .items_center()
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
    use super::*;
    use crate::{App, Lowering, wire};
    use gpui::Toggled;

    fn lower(element: impl IntoElement) -> wire::Node {
        let mut app = App::for_driver();
        let mut window = app.window();
        Lowering::new(&mut window, &mut app).lower(element)
    }

    fn interactivity(node: &wire::Node) -> &wire::Interactivity {
        match node {
            wire::Node::Container(wire::ContainerNode { interactivity, .. })
            | wire::Node::ResizeHandle { interactivity, .. } => interactivity,
            other => panic!("no interactivity: {other:?}"),
        }
    }

    fn faults(node: &wire::Node) -> Vec<wire::FaultKind> {
        wire::audit(node)
            .into_iter()
            .map(|fault| fault.kind)
            .collect()
    }

    #[test]
    fn an_icon_button_is_a_focusable_button_named_in_words() {
        let theme = Theme::light();
        let node = lower(icon_button("close", "✕", "Close", &theme, |_, _, _| {}));
        let control = interactivity(&node);
        assert_eq!(control.role, Some(Role::Button));
        assert_eq!(control.aria.label.as_deref(), Some("Close"));
        assert!(control.focusable && control.on_click.is_some());
        assert_eq!(faults(&node), []);
    }

    #[test]
    fn an_icon_button_is_at_least_24_px_each_way() {
        let theme = Theme::light();
        let node = lower(icon_button("close", "✕", "Close", &theme, |_, _, _| {}));
        let wire::Node::Container(container) = &node else {
            panic!("no container: {node:?}")
        };
        let floor = Some(px(24.).into());
        assert_eq!(
            (
                container.style.min_size.width,
                container.style.min_size.height
            ),
            (floor, floor)
        );
    }

    #[test]
    fn a_segmented_choice_is_a_radio_group_with_its_name() {
        let theme = Theme::light();
        let node = lower(segmented(
            "format",
            "Object format",
            &theme,
            0,
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
        let plain = lower(button("save", "Save", &theme, |_, _, _| {}));
        assert_eq!(
            interactivity(&plain).focus_visible,
            Some(focus_ring(theme.accent))
        );
        let ink = lower(button("send", "Send", &theme, |_, _, _| {}).kind(Kind::Primary));
        assert_eq!(
            interactivity(&ink).focus_visible,
            Some(focus_ring(theme.primary_foreground))
        );
        let own = lower(
            div()
                .id("menu")
                .focusable()
                .focus_visible(|style| style.opacity(0.5)),
        );
        assert_eq!(
            interactivity(&own).focus_visible,
            Some(StyleRefinement::default().opacity(0.5))
        );
        let still = lower(div().id("box").child("text"));
        assert_eq!(interactivity(&still).focus_visible, None);
    }

    #[test]
    fn a_switch_that_is_on_reports_toggled_true() {
        let theme = Theme::light();
        for (on, toggled) in [(true, Toggled::True), (false, Toggled::False)] {
            let node = lower(switch("dark", "Dark", on, true, &theme, |_, _, _| {}));
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
        let node = lower(segment("sha1", "SHA-1", true, &theme, |_, _, _| {}));
        let control = interactivity(&node);
        assert_eq!(control.role, Some(Role::RadioButton));
        assert_eq!(control.aria.toggled, Some(Toggled::True));
        assert_eq!(control.aria.selected, None);
        assert!(!control.focusable);
        let group = lower(segmented(
            "format",
            "Object format",
            &theme,
            1,
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
        let plain = lower(button("save", "Save", &theme, |_, _, _| {}));
        assert_eq!(interactivity(&plain).aria.toggled, None);
        for (selected, toggled) in [(true, Toggled::True), (false, Toggled::False)] {
            let node = lower(button("tree", "Tree", &theme, |_, _, _| {}).selected(selected));
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

    impl crate::Capabilities for Panes {
        const CAPABILITIES: &'static [crate::methods::Capability] = &[];
    }

    impl crate::View for Panes {
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

    impl crate::Capabilities for Format {
        const CAPABILITIES: &'static [crate::methods::Capability] = &[];
    }

    impl crate::View for Format {
        fn new(_: &mut Window, _: &mut crate::Context<Self>) -> Self {
            Self::default()
        }
    }

    impl crate::Render for Format {
        fn render(&mut self, _: &mut Window, cx: &mut crate::Context<Self>) -> impl IntoElement {
            segmented(
                "format",
                "Object format",
                &Theme::light(),
                self.picked,
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

    /// A composite of `count` items under one test's settings.
    #[derive(Default, serde::Serialize, serde::Deserialize)]
    struct Picker {
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
    }

    impl crate::Capabilities for Picker {
        const CAPABILITIES: &'static [crate::methods::Capability] = &[];
    }

    impl crate::View for Picker {
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
                    view.moved.push(index);
                    view.active = index;
                    cx.notify();
                }))
                .on_press(cx.processor(|view: &mut Self, index, _, cx| {
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
            list.build().children((0..self.count).map(move |index| {
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
                    Role::GridCell => div().id(format!("row-{index}")).role(Role::Row).child(cell),
                    _ => cell,
                }
            }))
        }
    }

    fn picker(
        set: impl FnOnce(&mut Picker),
    ) -> (crate::testing::TestAppContext, crate::Entity<Picker>) {
        let mut cx = crate::testing::TestAppContext::new();
        let picker = cx.open::<Picker>();
        picker.update(&mut cx, |view, _, cx| {
            set(view);
            cx.notify();
        });
        cx.run_until_parked();
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

    #[test]
    fn a_button_as_an_item_leaves_the_tab_order_and_claims() {
        let theme = Theme::light();
        let cell = lower(button("compare", "Compare", &theme, |_, _, _| {}).item(true));
        let control = interactivity(&cell);
        assert!(!control.focusable && control.tab_stop.is_none());
        assert!(control.aria.active_descendant && control.on_click.is_some());
        assert_eq!(control.role, Some(Role::Button));
    }

    #[test]
    fn an_item_built_focusable_leaves_the_tab_order() {
        let theme = Theme::light();
        let cell = lower(item(block_link("activity", 12, &theme), Role::Link, true));
        let link = interactivity(&cell);
        assert!(!link.focusable && link.tab_stop.is_none());
        assert!(link.aria.active_descendant && link.on_click.is_some());
        assert_eq!(link.role, Some(Role::Link));
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

    #[test]
    fn a_side_pane_docks_only_beside_the_whole_of_what_the_screen_keeps() {
        assert!(docks(1000., 576., 320.));
        assert!(docks(896., 576., 320.));
        assert!(!docks(895., 576., 320.));
        assert!(!docks(720., 400., 440.));
    }
}
