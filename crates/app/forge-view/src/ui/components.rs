//! The repeated shapes of this view; the ones every view shares (button,
//! empty state, heading, quiet line) come from `view_guest::design`.
use std::ops::Range;

use ducktape_view_guest::UniformListScrollHandle;
use ducktape_view_guest::design;
pub(crate) use ducktape_view_guest::design::{badge, button, empty_state, heading, short_hex};
use ducktape_view_guest::prelude::*;
use ducktape_view_guest::{AnchoredPositionMode, Edges, MouseDownEvent, Point};

pub(crate) fn id(text: impl Into<String>) -> ElementId {
    ElementId::Name(text.into().into())
}

/// A list row: the one interactive line every list of this view uses.
#[derive(IntoElement)]
pub(crate) struct Row<F>
where
    F: Fn(&ClickEvent, &mut Window, &mut App) + 'static,
{
    id: ElementId,
    theme: Theme,
    selected: bool,
    children: Vec<AnyElement>,
    click: Option<F>,
}

pub(crate) fn row<F>(id: impl Into<ElementId>, theme: &Theme) -> Row<F>
where
    F: Fn(&ClickEvent, &mut Window, &mut App) + 'static,
{
    Row {
        id: id.into(),
        theme: *theme,
        selected: false,
        children: Vec::new(),
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
    pub fn cell(mut self, child: impl IntoElement) -> Self {
        self.children.push(child.into_any_element());
        self
    }
}

impl<F> RenderOnce for Row<F>
where
    F: Fn(&ClickEvent, &mut Window, &mut App) + 'static,
{
    fn render(self, _: &mut Window, _: &mut App) -> impl IntoElement {
        let theme = self.theme;
        let mut element = div()
            .id(self.id)
            .w_full()
            .flex()
            .items_center()
            .gap_2()
            .min_h(design::size::CONTROL)
            .px_2()
            .children(self.children);
        let (chosen, hovered) = (theme.accent_soft, theme.hover);
        if self.selected {
            element = element.bg(chosen).aria_selected(true);
        }
        if let Some(click) = self.click {
            element = element
                .hover(move |style| style.bg(hovered))
                .role(Role::Button)
                .focusable()
                .on_click(click);
        }
        element
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
/// How far under its button's top a dropdown opens.
const MENU_DROP: Pixels = px(30.);

/// A button that names what is picked (`main ⌄`) and, while `open`, the
/// menu under it: `items` in a box that a press anywhere else closes.
/// The button only opens it.
pub(crate) fn dropdown(
    key: &str,
    label: String,
    open: bool,
    items: Vec<AnyElement>,
    theme: &Theme,
    on_open: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
    on_close: impl Fn(&MouseDownEvent, &mut Window, &mut App) + 'static,
) -> AnyElement {
    let theme = *theme;
    let button = div()
        .id(id(key.to_owned()))
        .h(design::size::ROW)
        .px(design::space::SM)
        .flex()
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
        .aria_expanded(open)
        .focusable()
        .on_click(on_open)
        .child(label)
        .child(div().text_color(theme.faint).child("⌄"));
    let mut wrapper = div().relative().flex_none().child(button);
    if open {
        let menu = div()
            .id(id(format!("{key}-menu")))
            .w(MENU_W)
            .py(design::space::XXS)
            .flex()
            .flex_col()
            .border_1()
            .border_color(theme.border_strong)
            .bg(theme.background)
            .shadow_lg()
            .occlude()
            .role(Role::Menu)
            .on_mouse_down_out(on_close)
            .children(items);
        // pinned to the button's top-left, so the drop is measured from there
        wrapper = wrapper.child(
            div().absolute().top_0().left_0().child(deferred(
                anchored()
                    .position_mode(AnchoredPositionMode::Local)
                    .position(Point {
                        x: px(0.),
                        y: MENU_DROP,
                    })
                    .snap_to_window_with_margin(Edges::all(design::space::SM))
                    .child(menu),
            )),
        );
    }
    wrapper.into_any_element()
}

/// A dropdown's group label: `Branches`, `Tags`.
pub(crate) fn menu_label(text: &str, theme: &Theme) -> AnyElement {
    div()
        .px(design::space::LG)
        .pt(design::space::SM)
        .pb(design::space::XXS)
        .font_family(design::fonts::FAMILY_MONO)
        .text_size(design::text::CAPTION)
        .text_color(theme.muted)
        .child(text.to_owned())
        .into_any_element()
}

/// One pick in a dropdown, mono, the picked one raised; `note` sits
/// faint on its right.
pub(crate) fn menu_item(
    element_id: ElementId,
    label: String,
    note: Option<&str>,
    selected: bool,
    theme: &Theme,
    click: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
) -> AnyElement {
    let theme = *theme;
    div()
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
        .role(Role::MenuItem)
        .aria_selected(selected)
        .focusable()
        .on_click(click)
        .child(div().flex_1().min_w(px(0.)).truncate().child(label))
        .children(note.map(|note| div().text_color(theme.faint).child(note.to_owned())))
        .into_any_element()
}

/// A tab with its count beside the label, faint: `Changes 3`. Tabs sit
/// on a bar's hairline, full height.
pub(crate) fn tab(
    element_id: ElementId,
    label: &str,
    count: Option<u64>,
    selected: bool,
    theme: &Theme,
    click: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
) -> AnyElement {
    design::tab(element_id, label.to_owned(), selected, theme, click)
        .h_full()
        .px_0()
        .gap(design::space::XS)
        .text_size(design::text::BODY)
        .font_weight(ducktape_view_guest::FontWeight::NORMAL)
        .children(count.map(|count| {
            div()
                .text_size(design::text::CAPTION)
                .text_color(theme.faint)
                .child(count.to_string())
        }))
        .into_any_element()
}
