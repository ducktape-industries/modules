//! The phone's members: the Expo app's two members screens, one page of
//! the view's [`Pages`](view::design::Pages) at a time. The view's root
//! picks this tree once, for a finger (`window.touch()`); nothing in it
//! asks again. It draws on the SDK's touch look (`design::TOUCH`).
//!
//! - [`members`]: the roster, Expo `app/(tabs)/members/index.tsx`.
//! - [`member`]: one member, Expo `app/(tabs)/members/[number].tsx`.
//! - [`parts`]: what both screens draw the same way.
//!
//! Reads the model the desk reads ([`Members`]); writes only where the
//! reader is: the page stack, the filter, the chip, the cursor.
use view::design::TOUCH;
use view::prelude::*;
use view::{Div, Stateful};

use crate::{Members, Page};

mod member;
mod members;
mod parts;

/// The page on top of the stack, under a root that hears Back while a
/// member is open (SPEC §9 rule 13; the host leaves the program else).
pub(crate) fn render(view: &Members, cx: &mut Context<Members>) -> Stateful<Div> {
    let theme = *cx.global::<Theme>();
    let ladder = &TOUCH;
    let page = match *view.pages.top() {
        Page::Roster => members::page(view, cx, &theme, ladder),
        Page::Member(number) => member::page(view, number, cx, &theme, ladder),
    };
    div()
        .id("members")
        .flex()
        .flex_col()
        .size_full()
        .bg(theme.background)
        .text_color(theme.foreground)
        .text_type(ladder.body)
        .when(view.pages.depth() > 0, |root| {
            root.on_back(cx.listener(|view, _: &BackEvent, _, cx| view.back(cx)))
        })
        .child(page)
}
