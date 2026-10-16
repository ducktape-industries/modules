//! The phone's screens. Under a finger the view is the Expo app's Nodes
//! tab, word for word (mobile `6f23151`): the list (`nodes.rs`, Expo
//! `app/(tabs)/nodes/index.tsx`) and one member's page (`node.rs`,
//! `app/(tabs)/nodes/[key].tsx`), with what both draw in `parts.rs` (the
//! confirm sheet a press that enacts raises, a vote's outcome strip, the
//! kit pieces the SDK has no helper for yet). The root picks this tree
//! once (`lib.rs`); every size is the SDK's touch ladder.
//!
//! Reads the whole view state; writes only through the view's own
//! presses: a member's page opens (`Nodes::open`), the door's sheet
//! (`Nodes::door`), Back (`Nodes::back`) and a vote (`Nodes::press`).
use view::design::Ladder;
use view::prelude::*;

use crate::{Nodes, Page};

mod node;
mod nodes;
mod parts;

/// Every size on the phone: the SDK's touch ladder (the Expo app's theme
/// and kit, mobile `src/design/theme.ts`).
const LADDER: Ladder = design::TOUCH;

/// The open page, the sheets over it, and Back exactly while a sheet is
/// drawn or a member's page is open (rule 13; the host leaves the program
/// else): both read off what this frame draws.
pub(crate) fn render(view: &Nodes, cx: &mut Context<Nodes>) -> impl IntoElement {
    let theme = &cx.global::<Theme>().clone();
    let pages = view.pages();
    let page = match pages.top() {
        Page::Nodes => nodes::render(view, cx, theme).into_any_element(),
        Page::Node(key) => node::render(view, key, cx, theme).into_any_element(),
    };
    let sheets: Vec<AnyElement> = nodes::door(view, cx, theme)
        .into_iter()
        .chain(parts::confirm(view, cx, theme))
        .collect();
    let back = (pages.depth() > 0 || !sheets.is_empty())
        .then(|| cx.listener(|view: &mut Nodes, _: &BackEvent, _, cx| view.back(cx)));
    div()
        .id("nodes")
        .flex()
        .flex_col()
        .size_full()
        .bg(theme.background)
        .text_color(theme.foreground)
        .text_type(LADDER.body)
        .when_some(back, |root, back| root.on_back(back))
        .child(page)
        .children(sheets)
}
