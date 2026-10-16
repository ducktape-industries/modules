//! Expo `app/(tabs)/explorer/_layout.tsx`: Explorer under a finger, the
//! Expo app's screens word for word (mobile `app/(tabs)/explorer/**`,
//! `src/network/explorer/*`, `src/kit/*`), one file per screen; the root
//! picks this tree once, from the device (`lib.rs`). The stack is the
//! model's [`Explorer::pages`]: the root ([`explorer`]) is the title row,
//! the page strip and the page's list; a block, a transaction, an account,
//! a program or a program's transactions goes over it with Back, and a
//! sheet over that. Reads the route and the open sheet; writes Back. The
//! desktop's pages (`ui/*.rs`) are the other design: the reads, routes,
//! writes and words both draw from live outside `ui/`.
use std::ops::Range;

use view::design::{CAPSULE_CLEARANCE, Face, Ladder, space};
use view::prelude::*;
use view::{Div, FontWeight, Stateful};

use crate::chain::{Line, lines};
use crate::decode::{ago, date, grouped, plural};
use crate::state::Sheet;
use crate::words::{self, unindexed};
use crate::{BlockRow, Cx, Explorer, Note, Route, TxRow};
use design::mono;

mod account;
mod block;
mod explorer;
mod form;
mod pages;
mod parts;
mod program;
mod programs;
mod tx;
mod txs;
use parts::*;

/// What every piece draws with: the theme, and the touch ladder (the Expo
/// app's sizes, `design::TOUCH`).
#[derive(Clone, Copy)]
struct Kit {
    theme: Theme,
    ladder: Ladder,
}

pub(crate) fn render(view: &Explorer, cx: Cx) -> AnyElement {
    let k = &Kit {
        theme: *cx.global::<Theme>(),
        ladder: design::TOUCH,
    };
    let page = match view.pages().top().clone() {
        Route::Block(height) => block::block(view, height, cx, k),
        Route::Tx { height, position } => tx::tx(view, height, position, cx, k),
        Route::Account(number) => account::account(view, number, cx, k),
        Route::Program(name) => program::program(view, &name, cx, k),
        Route::Transactions(Some(name)) => txs::transactions(view, &name, cx, k),
        Route::Overview
        | Route::Blocks
        | Route::Transactions(None)
        | Route::Accounts
        | Route::Programs => explorer::root(view, cx, k),
    };
    let sheet = match &view.sheet {
        Some(Sheet::More) => Some(programs::more_sheet(view, cx, k)),
        Some(Sheet::Program(name)) => programs::program_sheet(view, name, cx, k),
        None => form::sheet(view, cx, k),
    };
    // Back undoes what is drawn: the sheet on screen, else the page over
    // the root; with neither, the host leaves the program
    let back = (sheet.is_some() || view.pages().depth() > 0)
        .then(|| cx.listener(|view: &mut Explorer, _: &BackEvent, _, cx| view.back(cx)));
    div()
        .id("explorer")
        .size_full()
        .flex()
        .flex_col()
        .bg(k.theme.background)
        .text_color(k.theme.foreground)
        .text_type(k.ladder.body)
        .child(page)
        .children(sheet)
        .when_some(back, |root, back| root.on_back(back))
        .into_any_element()
}

/// Back on a page's title row.
fn back(cx: Cx) -> impl Fn(&ClickEvent, &mut Window, &mut App) + 'static {
    cx.listener(|view: &mut Explorer, _: &ClickEvent, _, cx| view.back(cx))
}
