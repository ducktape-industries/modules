//! The repeated shapes of this view; the ones every view shares (button,
//! empty state, heading, quiet line) come from `view_guest::design`.
use std::ops::Range;
use std::rc::Rc;

use crate::Forge;
use crate::state::Menu;
use ducktape_view_guest::UniformListScrollHandle;
use ducktape_view_guest::design;
pub(crate) use ducktape_view_guest::design::{badge, button, empty_state, heading, short_hex};
use ducktape_view_guest::prelude::*;
use ducktape_view_guest::{
    AnchoredPositionMode, Div, Edges, KeyDownEvent, Point, Stateful, accesskit,
};

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
            .when(self.active.is_some() && !self.selected, |item| {
                item.bg(hovered)
            });
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

/// A list of [`row`]s as one Tab stop: ↑ ↓ walk the rows, Enter presses
/// the active one (`on_press`). The rows go in as the list's children.
pub(crate) fn list(
    id_: &'static str,
    label: &str,
    count: usize,
    forge: &Forge,
    cx: &mut Context<Forge>,
    on_press: impl Fn(&mut Forge, usize, &mut Window, &mut Context<Forge>) + 'static,
) -> Stateful<Div> {
    let (at, _) = cursor(forge, id_);
    design::composite(id(id_), Role::ListBox, label.to_owned())
        .active(at.min(count.saturating_sub(1)), count)
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
        .overflow_y_scroll()
        .flex()
        .flex_col()
}

/// A grid of [`row`]s (each [`Row::in_grid`]) as one Tab stop: ↑ ↓ walk the rows,
/// ← → a row's cells (its press, then each control), Enter presses the
/// active cell (`on_press(row, cell)`). `cells` is the active row's count.
pub(crate) fn grid(
    id_: &'static str,
    label: &str,
    count: usize,
    cells: usize,
    forge: &Forge,
    cx: &mut Context<Forge>,
    on_press: impl Fn(&mut Forge, usize, usize, &mut Window, &mut Context<Forge>) + 'static,
) -> Stateful<Div> {
    let (at, cell) = cursor(forge, id_);
    let at = at.min(count.saturating_sub(1));
    let cell = cell.min(cells.saturating_sub(1));
    design::composite(id(id_), Role::Grid, label.to_owned())
        .active(at, count)
        .cells(cell, cells)
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
        .overflow_y_scroll()
        .flex()
        .flex_col()
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

/// Under this many rows a list is drawn whole: a few hundred rows lay out
/// in well under a frame, so virtualizing them buys nothing, and a list
/// drawn whole needs no visible range from the renderer before its first
/// frame shows every row.
pub(crate) const VIRTUALIZE_ABOVE: usize = 200;

/// A scrolling list of `count` rows. Over [`VIRTUALIZE_ABOVE`] it is virtual;
/// at or under it the rows are drawn whole (see [`VIRTUALIZE_ABOVE`]): a
/// virtual list's first frame, before the renderer names a visible range,
/// holds only the one row it measured.
pub(crate) fn rows(
    element_id: &str,
    count: usize,
    widest: Option<usize>,
    scroll: Option<&UniformListScrollHandle>,
    paint: impl Fn(usize) -> AnyElement + 'static,
) -> AnyElement {
    if count <= VIRTUALIZE_ABOVE {
        let mut column = div()
            .id(id(element_id.to_owned()))
            .flex()
            .flex_col()
            .flex_1()
            .min_h(px(0.))
            .overflow_y_scroll();
        for index in 0..count {
            column = column.child(paint(index));
        }
        return column.into_any_element();
    }
    let mut list = uniform_list(
        id(element_id.to_owned()),
        count,
        move |range: Range<usize>, _, _| range.map(&paint).collect::<Vec<_>>(),
    )
    .with_width_from_item(widest);
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

/// A dropdown's width.
const MENU_W: Pixels = px(220.);

/// A button that names what is picked (`main ⌄`) and says that it opens a
/// menu (AX-113) and whether that menu is open; the menu itself ([`menu`])
/// floats in the view's modal overlay while it is.
pub(crate) fn dropdown(
    key: &str,
    label: String,
    open: bool,
    theme: &Theme,
    on_open: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
) -> AnyElement {
    let theme = *theme;
    div()
        .id(id(key.to_owned()))
        .h(design::size::ROW)
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
        .hover(move |style| style.bg(theme.surface))
        .role(Role::Button)
        .aria_has_popup(accesskit::HasPopup::Menu)
        .aria_expanded(open)
        .focusable()
        .on_click(on_open)
        .child(label)
        .child(div().text_color(theme.faint).child("⌄"))
        .into_any_element()
}

/// What a press on a menu item does.
type Pick = Rc<dyn Fn(&mut Window, &mut App)>;

/// One line of a [`menu`]: a group label, or an item with its pick.
pub(crate) enum MenuEntry {
    Label(AnyElement),
    Item(Box<Stateful<Div>>, Pick),
}

/// A dropdown's group label: `Branches`, `Tags`.
pub(crate) fn menu_label(text: &str, theme: &Theme) -> MenuEntry {
    MenuEntry::Label(
        div()
            .px(design::space::LG)
            .pt(design::space::SM)
            .pb(design::space::XXS)
            .font_family(design::fonts::FAMILY_MONO)
            .text_size(design::text::CAPTION)
            .text_color(theme.muted)
            .child(text.to_owned())
            .into_any_element(),
    )
}

/// One pick in a dropdown, mono, the picked one raised and checked;
/// `note` sits faint on its right.
pub(crate) fn menu_item(
    element_id: ElementId,
    label: String,
    note: Option<&str>,
    selected: bool,
    theme: &Theme,
    pick: impl Fn(&mut Window, &mut App) + 'static,
) -> MenuEntry {
    let theme = *theme;
    let pick: Pick = Rc::new(pick);
    let click = pick.clone();
    let item = div()
        .id(element_id)
        .h(design::size::ROW)
        .px(design::space::LG)
        .flex()
        .items_center()
        .gap(design::space::SM)
        .font_family(design::fonts::FAMILY_MONO)
        .text_size(design::text::CAPTION)
        .when(selected, |item| item.bg(theme.surface_raised))
        .hover(move |style| style.bg(theme.surface))
        .aria_toggled(selected.into())
        .on_click(move |_: &ClickEvent, window: &mut Window, app: &mut App| click(window, app))
        .child(div().flex_1().min_w(px(0.)).truncate().child(label))
        .children(note.map(|note| div().text_color(theme.faint).child(note.to_owned())));
    MenuEntry::Item(Box::new(item), pick)
}

/// The open dropdown of `menu`: one Tab stop under the press that opened
/// it, whose ↑ ↓ walk the items, Enter or Space picks the active one and
/// Esc closes it, giving the keys back to its button. It floats in the
/// view's modal overlay, so Tab never leaves it and a press outside
/// closes it.
pub(crate) fn menu(
    menu: Menu,
    label: &str,
    entries: Vec<MenuEntry>,
    forge: &Forge,
    cx: &mut Context<Forge>,
    theme: &Theme,
) -> AnyElement {
    let key = menu.key();
    let picks: Vec<Pick> = entries
        .iter()
        .filter_map(|entry| match entry {
            MenuEntry::Item(_, pick) => Some(pick.clone()),
            MenuEntry::Label(_) => None,
        })
        .collect();
    let active = forge.menu_cursor.min(picks.len().saturating_sub(1));
    let mut index = 0;
    let lines: Vec<AnyElement> = entries
        .into_iter()
        .map(|entry| match entry {
            MenuEntry::Label(label) => label,
            MenuEntry::Item(item, _) => {
                let line = design::item(*item, Role::MenuItemRadio, index == active);
                index += 1;
                line.into_any_element()
            }
        })
        .collect();
    let escape = cx.listener(move |forge, event: &KeyDownEvent, window, cx| {
        if event.keystroke.key == "escape" && !event.keystroke.modifiers.modified() {
            forge.close_dropdown(menu, window, cx);
        }
    });
    let frame = design::composite(id(format!("{key}-menu")), Role::Menu, label.to_owned())
        .active(active, picks.len())
        .on_move(cx.processor(|forge, index: usize, _, cx| {
            forge.menu_cursor = index;
            cx.notify();
        }))
        .on_press(move |index, window, app| picks[index](window, app))
        .build()
        .on_key_down(escape)
        .w(MENU_W)
        .py(design::space::XXS)
        .flex()
        .flex_col()
        .border_1()
        .border_color(theme.border_strong)
        .bg(theme.background)
        .shadow_lg()
        .occlude()
        .children(lines);
    // the ring would replace the shadow: the frame draws both
    let frame = design::focus_shown(frame, theme, |style| style.shadow_lg());
    let (x, y) = forge.menu_at;
    anchored()
        .position_mode(AnchoredPositionMode::Window)
        .position(Point { x: px(x), y: px(y) })
        .snap_to_window_with_margin(Edges::all(design::space::SM))
        .child(frame)
        .into_any_element()
}

/// A tab with its count beside the label, faint: `Changes 3`. Tabs sit
/// on a bar's hairline, full height, in a manual tab list: `selected` is
/// the open one, `active` the one the arrows are on, which Enter opens.
pub(crate) fn tab(
    element_id: ElementId,
    label: &str,
    count: Option<u64>,
    selected: bool,
    active: bool,
    theme: &Theme,
    click: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
) -> AnyElement {
    let tab = design::tab(element_id, label.to_owned(), selected, theme, click)
        .h_full()
        .px_0()
        .gap(design::space::XS)
        .text_size(design::text::BODY)
        .font_weight(ducktape_view_guest::FontWeight::NORMAL)
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
