//! The repeated shapes of this view; the ones every view shares (button,
//! empty state, heading, quiet line) come from `view::design`.
use std::ops::Range;
use std::rc::Rc;

use crate::Forge;
use crate::state::Menu;
use view::design::Ladder;
pub(crate) use view::design::{badge, button, empty_state, heading, short_hex};
use view::prelude::*;
use view::wire::{
    self,
    keyboard::{Key, Named},
};
use view::{AnchoredPositionMode, Div, Edges, Point, Stateful, accesskit};

/// The smallest box a pointer presses, each way (the door's AX-017).
pub(crate) const PRESS_TARGET: Pixels = px(24.);

pub(crate) fn id(text: impl Into<String>) -> ElementId {
    ElementId::Name(text.into().into())
}

/// A list row: the one interactive line every list of this view uses. A
/// row that acts is a list item holding a button of its cells, its
/// controls beside that button.
#[derive(IntoElement)]
pub(crate) struct Row<F>
where
    F: Fn(&ClickEvent, &mut Window, &mut App) + 'static,
{
    key: String,
    theme: Theme,
    selected: bool,
    /// `Some(cell)`: the arrows are on this row, at that cell
    active: Option<usize>,
    /// the arrows have walked this list: the active row is drawn, not
    /// only claimed
    walked: bool,
    /// a row of a [`grid`]: a `Row` of cells even with no control
    in_grid: bool,
    children: Vec<AnyElement>,
    controls: Vec<AnyElement>,
    click: Option<F>,
}

/// A row keyed `key` of a [`list`] or a [`grid`]; its press, when it has
/// one, is `{key}-open`.
pub(crate) fn row<F>(key: impl Into<String>, theme: &Theme) -> Row<F>
where
    F: Fn(&ClickEvent, &mut Window, &mut App) + 'static,
{
    Row {
        key: key.into(),
        theme: *theme,
        selected: false,
        active: None,
        walked: false,
        in_grid: false,
        children: Vec::new(),
        controls: Vec::new(),
        click: None,
    }
}

impl<F> Row<F>
where
    F: Fn(&ClickEvent, &mut Window, &mut App) + 'static,
{
    pub fn on_click(mut self, click: F) -> Self {
        self.click = Some(click);
        self
    }
    pub fn selected(mut self, selected: bool) -> Self {
        self.selected = selected;
        self
    }
    /// The arrows are on this row, at `cell`: 0 is its press, then each
    /// control in order. A control claims for itself when it is the cell.
    pub fn active(mut self, cell: Option<usize>) -> Self {
        self.active = cell;
        self
    }
    /// The list has been walked with the arrows: its active row shows. At
    /// rest the first row is claimed for the keys but drawn like any other,
    /// since a view cannot see whether its list holds the focus.
    pub fn walked(mut self, walked: bool) -> Self {
        self.walked = walked;
        self
    }
    /// A row of a [`grid`]: its press is the first cell of a `Row` even
    /// when no control sits beside it (the default head's ref, a tag), as
    /// a grid holds rows of cells and never a list box's option.
    pub fn in_grid(mut self) -> Self {
        self.in_grid = true;
        self
    }
    pub fn cell(mut self, child: impl IntoElement) -> Self {
        self.children.push(child.into_any_element());
        self
    }
    /// A control of its own at the row's end (a ref's Compare), beside
    /// the row's press rather than inside it: a cell of the grid row.
    pub fn control(mut self, child: impl IntoElement) -> Self {
        self.controls.push(child.into_any_element());
        self
    }
}

impl<F> RenderOnce for Row<F>
where
    F: Fn(&ClickEvent, &mut Window, &mut App) + 'static,
{
    fn render(self, _: &mut Window, _: &mut App) -> impl IntoElement {
        let theme = self.theme;
        let (chosen, hovered) = (theme.accent_soft, theme.hover);
        let item = div()
            .id(id(self.key.clone()))
            .w_full()
            .flex()
            .items_center()
            .gap_2()
            .min_h(design::size::CONTROL)
            .px_2()
            .when(self.selected, |item| item.bg(chosen))
            .when(
                self.walked && self.active.is_some() && !self.selected,
                |item| item.bg(hovered),
            );
        let Some(click) = self.click else {
            return item
                .role(Role::ListItem)
                .children(self.children)
                .children(self.controls);
        };
        // no controls, in a list: the row is the option, and its press
        if self.controls.is_empty() && !self.in_grid {
            let option = item
                .hover(move |style| style.bg(hovered))
                .aria_selected(self.selected)
                .on_click(click)
                .children(self.children);
            return design::item(option, Role::ListBoxOption, self.active.is_some());
        }
        // a grid row whose first cell is the press
        let press = div()
            .id(id(format!("{}-open", self.key)))
            .size_full()
            .flex()
            .items_center()
            .gap_2()
            .when(self.selected, |press| {
                press.aria_current(accesskit::AriaCurrent::True)
            })
            .on_click(click)
            .children(self.children);
        let press = div()
            .id(id(format!("{}-open-cell", self.key)))
            .flex_1()
            .min_w(px(0.))
            .self_stretch()
            .role(Role::GridCell)
            .child(design::item(press, Role::Button, self.active == Some(0)));
        let key = self.key.clone();
        item.role(Role::Row)
            .hover(move |style| style.bg(hovered))
            .child(press)
            .children(
                self.controls
                    .into_iter()
                    .enumerate()
                    .map(|(index, control)| {
                        div()
                            .id(id(format!("{key}-control-{index}")))
                            .role(Role::GridCell)
                            .child(control)
                    }),
            )
    }
}

/// Which row of the list `id` the arrows are on, and which of its cells:
/// the view's cursor there, else the first row.
pub(crate) fn cursor(forge: &Forge, id: &'static str) -> (usize, usize) {
    match forge.list_cursor {
        Some((list, row, cell)) if list == id => (row, cell),
        _ => (0, 0),
    }
}

/// Whether the arrows have walked the list `id`: only then is its active
/// row drawn. At rest every list claims its first row for the keys and
/// draws none, as a view cannot see whether the list holds the focus.
pub(crate) fn walked(forge: &Forge, id: &'static str) -> bool {
    matches!(forge.list_cursor, Some((list, ..)) if list == id)
}

/// A list of [`row`]s as one Tab stop: ↑ ↓ walk the rows (a virtual list
/// tracking `scroll` is scrolled to the active one before it claims), Enter
/// presses the active one (`on_press`). The rows go in as the list's
/// children: drawn whole (no `scroll`) they scroll here; a virtual list
/// scrolls itself, and the one bar and its gutter are that list's.
pub(crate) fn list(
    id_: &'static str,
    label: &str,
    count: usize,
    scroll: Option<&UniformListScrollHandle>,
    forge: &Forge,
    cx: &mut Context<Forge>,
    on_press: impl Fn(&mut Forge, usize, &mut Window, &mut Context<Forge>) + 'static,
) -> Stateful<Div> {
    let (at, _) = cursor(forge, id_);
    let mut list = design::composite(id(id_), Role::ListBox, label.to_owned())
        .active(at.min(count.saturating_sub(1)), count);
    if let Some(scroll) = scroll {
        list = list.track_scroll(scroll);
    }
    let list = list
        .on_move(cx.processor(move |forge, index: usize, _, cx| {
            forge.list_cursor = Some((id_, index, 0));
            cx.notify();
        }))
        .on_press(
            cx.processor(move |forge, index: usize, window, cx| on_press(forge, index, window, cx)),
        )
        .build()
        .flex_1()
        .min_h(px(0.))
        .flex()
        .flex_col();
    match scroll {
        Some(_) => list,
        None => list.overflow_y_scroll(),
    }
}

/// A grid of [`row`]s (each [`Row::in_grid`]) as one Tab stop: ↑ ↓ walk the rows,
/// ← → a row's cells (its press, then each control), Enter presses the
/// active cell (`on_press(row, cell)`). `cells` is the active row's count.
/// The rows go in as [`list`]'s do: drawn whole (no `scroll`) they scroll
/// here; a virtual list tracking `scroll` scrolls itself, to the active
/// row before it claims.
#[allow(clippy::too_many_arguments)]
pub(crate) fn grid(
    id_: &'static str,
    label: &str,
    count: usize,
    cells: usize,
    scroll: Option<&UniformListScrollHandle>,
    forge: &Forge,
    cx: &mut Context<Forge>,
    on_press: impl Fn(&mut Forge, usize, usize, &mut Window, &mut Context<Forge>) + 'static,
) -> Stateful<Div> {
    let (at, cell) = cursor(forge, id_);
    let at = at.min(count.saturating_sub(1));
    let cell = cell.min(cells.saturating_sub(1));
    let mut grid = design::composite(id(id_), Role::Grid, label.to_owned())
        .active(at, count)
        .cells(cell, cells);
    if let Some(scroll) = scroll {
        grid = grid.track_scroll(scroll);
    }
    let grid = grid
        .on_move(cx.processor(move |forge, index: usize, _, cx| {
            forge.list_cursor = Some((id_, index, 0));
            cx.notify();
        }))
        .on_move_cell(cx.processor(move |forge, cell: usize, _, cx| {
            forge.list_cursor = Some((id_, at, cell));
            cx.notify();
        }))
        .on_press(cx.processor(move |forge, index: usize, window, cx| {
            let (_, cell) = cursor(forge, id_);
            on_press(forge, index, cell, window, cx)
        }))
        .build()
        .flex_1()
        .min_h(px(0.))
        .flex()
        .flex_col();
    match scroll {
        Some(_) => grid,
        None => grid.overflow_y_scroll(),
    }
}

pub(crate) fn loading(id: impl Into<ElementId>, text: &str, theme: &Theme) -> AnyElement {
    div()
        .id(id.into())
        .p_3()
        .text_size(design::text::SECONDARY)
        .text_color(theme.muted)
        .child(text.to_owned())
        .into_any_element()
}

/// A scrolling virtual list of `count` rows.
pub(crate) fn rows(
    element_id: &str,
    count: usize,
    widest: Option<usize>,
    scroll: Option<&UniformListScrollHandle>,
    paint: impl Fn(usize) -> AnyElement + 'static,
) -> AnyElement {
    window(element_id, count, widest, scroll, move |range, _, _| {
        range.map(&paint).collect()
    })
}

/// [`rows`], painted a window at a time: `paint` is handed the rows the
/// list lowers, so a list that reads on as it is scrolled hears how far.
pub(crate) fn window(
    element_id: &str,
    count: usize,
    widest: Option<usize>,
    scroll: Option<&UniformListScrollHandle>,
    paint: impl Fn(Range<usize>, &mut Window, &mut App) -> Vec<AnyElement> + 'static,
) -> AnyElement {
    let mut list =
        uniform_list(id(element_id.to_owned()), count, paint).with_width_from_item(widest);
    if let Some(scroll) = scroll {
        list = list.track_scroll(scroll);
    }
    list.flex_1().min_h(px(0.)).into_any_element()
}

/// [`design::quiet`], as the screens that return it hand it on.
pub(crate) fn quiet(text: impl Into<SharedString>, theme: &Theme) -> AnyElement {
    design::quiet(text, theme).into_any_element()
}

/// A path's bytes as something a screen can say out loud.
pub(crate) fn path_text(path: &[u8]) -> String {
    String::from_utf8_lossy(path).into_owned()
}

/// A ref's full name shortened to what a reader calls it.
pub(crate) fn ref_label(name: &[u8]) -> String {
    let text = path_text(name);
    text.strip_prefix("refs/heads/")
        .or_else(|| text.strip_prefix("refs/tags/"))
        .unwrap_or(&text)
        .to_owned()
}

/// A dropdown's width, at most: a narrower window takes it in.
const MENU_W: Pixels = px(320.);
/// The rows a dropdown's list shows before it scrolls.
const MENU_ROWS: usize = 12;
/// With fewer rows than this free under the press, the list takes the
/// window's height instead and the menu is lifted to fit.
const MENU_MIN_ROWS: usize = 5;
/// The picked item's check, a column of its own.
const CHECK_W: Pixels = px(12.);

/// A button that names what is picked (`main ⌄`) and says whether its
/// menu is open; the menu itself ([`menu`]) floats in the view's modal
/// overlay while it is. A long name is cut in its middle, whole on hover.
pub(crate) fn dropdown(
    key: &str,
    label: String,
    open: bool,
    theme: &Theme,
    on_open: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
) -> AnyElement {
    let button = div()
        .id(id(key.to_owned()))
        .h(design::size::ROW)
        .max_w(MENU_W)
        .px(design::space::SM)
        .flex()
        .flex_none()
        .items_center()
        .gap(design::space::XS)
        .border_1()
        .border_color(theme.border_strong)
        .bg(theme.background)
        .font_family(design::fonts::FAMILY_MONO)
        .text_size(design::text::CAPTION)
        .whitespace_nowrap()
        .hover({
            let theme = *theme;
            move |style| style.bg(theme.surface)
        })
        .role(Role::Button)
        .aria_expanded(open)
        .focusable()
        .on_click(on_open)
        .child(name_cell(label.clone()))
        .child(div().text_color(theme.faint).child("⌄"));
    // open, its menu says every name: no tip over it
    match open {
        true => button.into_any_element(),
        false => whole_name(button, label, theme).into_any_element(),
    }
}

/// A ref's name in a column that may be too narrow for it: cut in the
/// middle, so both the prefix and the distinguishing end stay
/// (`people/ada/a-rath…-work-89`).
pub(crate) fn name_cell(label: impl Into<SharedString>) -> Div {
    div()
        .min_w(px(0.))
        .truncate()
        .text_ellipsis_middle()
        .child(label.into())
}

/// The longest name every column that shows one holds whole: the Refs
/// tab's name column holds some 30 characters, a menu row more.
const WHOLE_UP_TO: usize = 24;

/// `element`, which shows `name` in a column that may cut it short, says
/// it whole on hover; a name short enough to be shown whole says nothing
/// more.
// ponytail: counts characters, not the drawn width (the view does not
// measure text); a name of wide glyphs, or a menu squeezed by a window
// under ~340 px, can be cut below the count with no tip. The host knows
// when it cut a text: a tip it shows only then would replace the count.
pub(crate) fn whole_name<E: StatefulInteractiveElement>(
    element: E,
    name: impl Into<SharedString>,
    theme: &Theme,
) -> E {
    let (text, theme) = (name.into(), *theme);
    if text.chars().count() <= WHOLE_UP_TO {
        return element;
    }
    element.tooltip(move |_, cx| {
        cx.new(|_| WholeName {
            text: text.clone(),
            theme,
        })
        .into()
    })
}

/// A hover's whole name, mono on the view's own surface.
struct WholeName {
    text: SharedString,
    theme: Theme,
}

impl Render for WholeName {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div()
            .id("forge-whole-name")
            .px(design::space::SM)
            .py(design::space::XXS)
            .border_1()
            .border_color(self.theme.border_strong)
            .bg(self.theme.background)
            .text_color(self.theme.foreground)
            .font_family(design::fonts::FAMILY_MONO)
            .text_size(design::text::CAPTION)
            .child(self.text.clone())
    }
}

/// A key the filter box hears instead of its text: ↑ ↓ walk the list, Esc
/// closes it.
fn claimed(key: Named) -> wire::KeyClaim {
    wire::KeyClaim {
        key: Key::Named(key),
        modifiers: Default::default(),
        command: false,
    }
}

/// The open dropdown of `menu`, a list of refs. Its filter box takes the
/// keys on open: what is typed narrows the list by name, in any case, ↑ ↓
/// walk it, Enter picks the active item and Esc closes the menu, giving
/// the keys back to its button. The ref picker splits branches from tags
/// under it. The list scrolls past [`MENU_ROWS`] and keeps to the window;
/// the default branch is first and marked, the picked ref checked. The
/// list is a Tab stop of its own whose keys do the same. The menu floats
/// in the view's modal overlay, so Tab never leaves it and a press outside
/// closes it.
pub(crate) fn menu(
    menu: Menu,
    label: &str,
    forge: &Forge,
    cx: &mut Context<Forge>,
    theme: &Theme,
    ladder: &Ladder,
) -> AnyElement {
    let key = menu.key();
    let refs = forge.menu_list(menu);
    let what = match menu == Menu::Ref && forge.menu_tags {
        true => "tags",
        false => "branches",
    };
    let filter_id = id(format!("{key}-filter"));
    let filter = Input::new(
        filter_id.clone(),
        &forge.menu_search,
        format!("Filter {what}"),
    )
    .placeholder(format!("Filter {what}"))
    .claims([Named::ArrowUp, Named::ArrowDown, Named::Escape].map(claimed))
    .on_key(cx.listener(move |forge, event: &KeyDownEvent, window, cx| {
        let last = forge.menu_list(menu).len().saturating_sub(1);
        let at = forge.menu_cursor.min(last);
        match event.keystroke.key.as_str() {
            "up" => forge.menu_step(at.saturating_sub(1), cx),
            "down" => forge.menu_step((at + 1).min(last), cx),
            "escape" => forge.close_dropdown(menu, window, cx),
            _ => {}
        }
    }))
    // a new filter starts the arrows at its first match
    .on_change(cx.listener(|forge, _: &wire::TextChange, _, cx| forge.menu_step(0, cx)))
    .on_submit(cx.listener(move |forge, _: &(), window, cx| {
        let refs = forge.menu_list(menu);
        let at = forge.menu_cursor.min(refs.len().saturating_sub(1));
        if let Some(name) = refs.get(at) {
            forge.menu_pick(menu, name.clone(), window, cx);
        }
    }))
    .h(design::size::ROW)
    .w_full()
    .px_2()
    .border_1()
    .border_color(theme.border_strong)
    .bg(theme.surface)
    .text_color(theme.foreground)
    .text_size(design::text::CAPTION);
    let mut head = div()
        .flex()
        .flex_col()
        .gap(design::space::SM)
        .p(design::space::SM)
        .border_b_1()
        .border_color(theme.border)
        .child(filter);
    // the rows the list has room for: the frame keeps one height as the
    // filter narrows it and as the tabs switch, and stays in the window
    let mut chrome = 2. * BORDER + 2. * design::spacing::SM as f32 + ROW + 2. * LIST_PAD + BORDER;
    if menu == Menu::Ref {
        let counts = [false, true].map(|tags| forge.menu_refs(tags).len());
        let choices = [("branches", "Branches"), ("tags", "Tags")]
            .into_iter()
            .zip(counts)
            .map(|((slug, word), count)| {
                (
                    id(format!("{key}-{slug}")),
                    format!("{word} {count}").into(),
                )
            });
        let pick = cx.processor(move |forge, index: usize, _, cx| {
            forge.menu_tags = index == 1;
            forge.menu_step(0, cx);
        });
        head = head.child(
            design::segmented(
                id(format!("{key}-kind")),
                "Branches or tags",
                ladder,
                theme,
                Some(usize::from(forge.menu_tags)),
                choices,
                pick,
            )
            .self_start()
            // a press focuses the tabs: it hands the keys back to the
            // filter, where the typing goes on; ← → keep them on the tabs
            .on_click(move |_: &ClickEvent, window: &mut Window, _: &mut App| {
                window.focus(filter_id.clone())
            }),
        );
        // the segments' box draws its top and bottom edges around them
        chrome += design::spacing::SM as f32 + ROW + 2. * BORDER;
    }
    let total = match menu {
        Menu::Ref => forge.refs().map_or(0, <[_]>::len),
        Menu::Head => forge.branches().len(),
    };
    let (_, y) = forge.menu_at;
    let height = forge.layout.height;
    let room = |space: f32| ((space - chrome) / ROW).floor().max(1.) as usize;
    let below = room(height - y - design::spacing::SM as f32);
    let fits = match below >= MENU_MIN_ROWS {
        true => below,
        false => room(height - 2. * design::spacing::SM as f32),
    };
    let shown = MENU_ROWS.min(fits).min(total.max(1));
    let body: AnyElement = match forge.refs() {
        None => quiet("Reading refs…", theme),
        Some(_) if refs.is_empty() => {
            let none = match forge.menu_search.is_blank() {
                true => format!("No {what}"),
                false => format!("No {what} match"),
            };
            div()
                .id(id(format!("{key}-none")))
                .px(design::space::SM)
                .child(quiet(none, theme))
                .into_any_element()
        }
        Some(_) => menu_list(menu, label, refs, forge, cx, theme),
    };
    let frame = div()
        .id(id(format!("{key}-frame")))
        .w(MENU_W.min(px(forge.layout.width - 2. * design::spacing::SM as f32)))
        .flex()
        .flex_col()
        .border_1()
        .border_color(theme.border_strong)
        .bg(theme.background)
        .shadow_lg()
        .occlude()
        .child(head)
        .child(
            div()
                .id(id(format!("{key}-rows")))
                .h(px(shown as f32 * ROW + 2. * LIST_PAD))
                .py(px(LIST_PAD))
                .flex()
                .flex_col()
                .child(body),
        );
    let (x, y) = forge.menu_at;
    anchored()
        .position_mode(AnchoredPositionMode::Window)
        .position(Point { x: px(x), y: px(y) })
        .snap_to_window_with_margin(Edges::all(design::space::SM))
        .child(frame)
        .into_any_element()
}

/// A dropdown's row, its frame's edge and the list's inset, as the
/// arithmetic that fits the menu in the window reads them.
const ROW: f32 = design::height::ROW as f32;
const BORDER: f32 = 1.;
const LIST_PAD: f32 = design::spacing::XXS as f32;

/// The items of an open dropdown: one Tab stop, a virtual list scrolled to
/// the active item, ↑ ↓ walking it and Enter or Space picking.
fn menu_list(
    menu: Menu,
    label: &str,
    refs: Vec<Vec<u8>>,
    forge: &Forge,
    cx: &mut Context<Forge>,
    theme: &Theme,
) -> AnyElement {
    let count = refs.len();
    let active = forge.menu_cursor.min(count.saturating_sub(1));
    let (default, picked) = (forge.default_head(), forge.menu_picked(menu));
    let pick = Rc::new(cx.processor(move |forge, name: Vec<u8>, window, cx| {
        forge.menu_pick(menu, name, window, cx)
    }));
    let pressed = pick.clone();
    let escape = cx.listener(move |forge, event: &KeyDownEvent, window, cx| {
        if event.keystroke.key == "escape" && !event.keystroke.modifiers.modified() {
            forge.close_dropdown(menu, window, cx);
        }
    });
    let theme = *theme;
    let refs = Rc::new(refs);
    let names = refs.clone();
    let paint = move |range: Range<usize>, _: &mut Window, _: &mut App| {
        range
            .map(|index| {
                let name = names[index].clone();
                let pick = pick.clone();
                let click = move |_: &ClickEvent, window: &mut Window, app: &mut App| {
                    pick(name.clone(), window, app)
                };
                let name = &names[index];
                menu_item(
                    id(format!("forge-ref-{}", path_text(name))),
                    ref_label(name),
                    (*name == default).then_some("default"),
                    *name == picked,
                    index == active,
                    &theme,
                    click,
                )
                .into_any_element()
            })
            .collect()
    };
    let list = uniform_list(id(format!("{}-items", menu.key())), count, paint)
        .track_scroll(&forge.menu_scroll)
        .size_full();
    design::composite(
        id(format!("{}-menu", menu.key())),
        Role::Menu,
        label.to_owned(),
    )
    .active(active, count)
    .track_scroll(&forge.menu_scroll)
    .on_move(cx.processor(|forge, index: usize, _, cx| {
        forge.menu_cursor = index;
        cx.notify();
    }))
    .on_press(move |index, window, app| pressed(refs[index].clone(), window, app))
    .build()
    // Escape is the menu's: the overlay around it does not close it again
    .on_key_down(escape)
    .consumes_keys(["escape"])
    .size_full()
    .child(list)
    .into_any_element()
}

/// One ref of a dropdown, mono: its check when it is the picked one, its
/// name cut in the middle and whole on hover, `note` faint on its right;
/// the arrows' item raised.
fn menu_item(
    element_id: ElementId,
    label: String,
    note: Option<&str>,
    selected: bool,
    active: bool,
    theme: &Theme,
    click: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
) -> Stateful<Div> {
    let theme = *theme;
    // the check is drawn, not said: `aria_toggled` says it
    let name = match note {
        Some(note) => format!("{label} ({note})"),
        None => label.clone(),
    };
    let item = div()
        .id(element_id)
        .w_full()
        .h(design::size::ROW)
        .px(design::space::SM)
        .flex()
        .items_center()
        .gap(design::space::XS)
        .font_family(design::fonts::FAMILY_MONO)
        .text_size(design::text::CAPTION)
        .when(active, |item| item.bg(theme.surface_raised))
        .hover(move |style| style.bg(theme.surface))
        .aria_label(name)
        .aria_toggled(selected.into())
        .on_click(click)
        .child(
            div()
                .w(CHECK_W)
                .flex_none()
                .children(selected.then_some("✓")),
        )
        .child(name_cell(label.clone()).flex_1())
        .children(note.map(|note| div().text_color(theme.faint).child(note.to_owned())));
    design::item(whole_name(item, label, &theme), Role::MenuItemRadio, active)
}

/// A tab with its count beside the label, faint: `Changes 3`. Tabs sit
/// on a bar's hairline, full height, in a manual tab list: `selected` is
/// the open one, `active` the one the arrows are on, which Enter opens.
#[allow(
    clippy::too_many_arguments,
    reason = "the ladder rides beside the theme into every sized helper; the phone rework of this view reshapes the row"
)]
pub(crate) fn tab(
    element_id: ElementId,
    label: &str,
    count: Option<u64>,
    selected: bool,
    active: bool,
    theme: &Theme,
    ladder: &Ladder,
    click: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
) -> AnyElement {
    let tab = design::tab(element_id, label.to_owned(), selected, ladder, theme, click)
        .h_full()
        .px_0()
        .gap(design::space::XS)
        .text_size(design::text::BODY)
        .font_weight(view::FontWeight::NORMAL)
        // the arrows' tab, not yet open: a quiet fill says where they are
        .when(active && !selected, |tab| tab.bg(theme.surface_raised))
        .children(count.map(|count| {
            div()
                .text_size(design::text::CAPTION)
                .text_color(theme.faint)
                .child(count.to_string())
        }));
    design::item(tab, Role::Tab, active).into_any_element()
}
