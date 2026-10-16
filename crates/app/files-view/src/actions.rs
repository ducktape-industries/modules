//! Where the reader goes: a page opened over the one on screen, a page
//! opened straight over the places (a place, a crumb of the path, the
//! History tab), and Back, which closes the sheet on screen before it
//! leaves a page.
use view::design::Pages;
use view::prelude::*;

use crate::Files;
use crate::state::{Route, Sheet};

impl Files {
    /// Opens `route` over the page on screen. A file's or a snapshot's
    /// page is only ever opened from a list (a wide desktop shows each
    /// beside it), so a route opened from one takes its place: the stack
    /// does not grow with every row looked at, and Back leaves the list.
    pub(crate) fn open(&mut self, route: Route, cx: &mut Context<Self>) {
        match self.page() {
            Route::File(_) | Route::Snapshot(_) => self.pages.replace(route),
            Route::Places | Route::Folder(_) | Route::History => self.pages.push(route),
        }
        self.moved(cx);
    }

    /// Opens `route` with only the places under it.
    pub(crate) fn open_from_places(&mut self, route: Route, cx: &mut Context<Self>) {
        self.pages = Pages::root(Route::Places);
        if route != Route::Places {
            self.pages.push(route);
        }
        self.moved(cx);
    }

    /// Back: the sheet on screen, else the page under this one; at the
    /// places with nothing open, nothing.
    pub(crate) fn back(&mut self, cx: &mut Context<Self>) {
        if self.sheet_shown().is_some() {
            self.sheet = None;
            cx.notify();
            return;
        }
        if self.pages.pop().is_some() {
            self.moved(cx);
        }
    }

    /// The page's writes, as a list over it.
    pub(crate) fn open_actions(&mut self, cx: &mut Context<Self>) {
        self.sheet = Some(Sheet::Actions);
        cx.notify();
    }

    pub(crate) fn close_sheet(&mut self, cx: &mut Context<Self>) {
        self.sheet = None;
        cx.notify();
    }

    /// A new page is on top: what the last one said and had open goes,
    /// and what this one shows is read.
    fn moved(&mut self, cx: &mut Context<Self>) {
        self.sheet = None;
        self.note = None;
        self.read_page(cx);
        cx.notify();
    }
}
