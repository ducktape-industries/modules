//! Files under a finger: the phone's screens, in the Expo app's kit look
//! (mobile `src/kit/*`). The Expo app has no files screen; its nearest is
//! the code browser, `mobile/app/(tabs)/forge/[repo]/code/[...path].tsx`,
//! whose page-per-folder stack and row shape these follow. One file per
//! screen; the root picks this tree once, from the device (`lib.rs`).
//!
//! The stack is the model's: the places are the root ([`places`]); a
//! folder, a file, the history or a snapshot goes over it with Back, and
//! a sheet over that (the page's writes, or one write's form). Reads the
//! route and the open sheet; writes Back. The desktop's pages (`ui/*.rs`)
//! are the other design: the reads, routes, writes and words both draw
//! from live outside `ui/`.
use view::design::{Ladder, Nav, space};
use view::prelude::*;
use view::{Div, FontWeight, Stateful};

use crate::state::{Route, Sheet};
use crate::{Cx, Files, words};

mod file;
mod folder;
mod history;
mod parts;
mod places;
mod sheets;
mod snapshot;
use parts::*;

/// What every piece draws with: the theme, and the touch ladder (the Expo
/// app's sizes, `design::TOUCH`).
#[derive(Clone, Copy)]
struct Kit {
    theme: Theme,
    ladder: Ladder,
}

pub(crate) fn render(view: &Files, cx: Cx) -> AnyElement {
    let k = &Kit {
        theme: *cx.global::<Theme>(),
        ladder: design::TOUCH,
    };
    let page = match view.page().clone() {
        Route::Places => places::places(view, cx, k),
        Route::Folder(path) => folder::folder(view, &path, cx, k),
        Route::File(path) => file::file(view, &path, cx, k),
        Route::History => history::history(view, cx, k),
        Route::Snapshot(seq) => snapshot::snapshot(view, seq, cx, k),
    };
    let sheet = match &view.sheet {
        Some(Sheet::Actions) => sheets::actions(view, cx, k),
        Some(Sheet::Write(form)) => Some(sheets::form(view, form, cx, k)),
        None => None,
    };
    // Back undoes what is drawn: the sheet on screen, else the page over
    // the places; with neither, the host leaves the program
    let back = (sheet.is_some() || view.pages.depth() > 0)
        .then(|| cx.listener(|view: &mut Files, _: &BackEvent, _, cx| view.back(cx)));
    div()
        .id("files")
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

/// Where a page sits: the places are the root, every other page has Back.
fn nav(view: &Files, cx: Cx) -> Nav {
    match view.pages.depth() {
        0 => Nav::Root,
        _ => Nav::back(cx.listener(|view: &mut Files, _: &ClickEvent, _, cx| view.back(cx))),
    }
}
