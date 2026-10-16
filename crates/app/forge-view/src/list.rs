//! A list as one Tab stop, for either design: the arrows walk its rows
//! (the view's `list_cursor`), Enter presses the active one, and a long
//! list is virtual, painted a window at a time.
use std::ops::Range;

use view::prelude::*;
use view::{Div, Stateful};

use crate::{Forge, id};

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

/// A list of rows as one Tab stop: ↑ ↓ walk the rows (a virtual list
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

/// A scrolling virtual list of `count` rows, painted a window at a time:
/// `paint` is handed the rows the list lowers, so a list that reads on as
/// it is scrolled hears how far.
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
