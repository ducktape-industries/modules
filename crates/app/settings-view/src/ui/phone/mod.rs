//! Expo `app/account/index.tsx` and `src/account/Agents.tsx`: the Account
//! screen as the phone app draws it, the view's second design (owner
//! 2026-10-16: the phone gets its own screens; the model is shared).
//! `lib.rs` picks it once, when the window's
//! pointer is a finger; nothing under here asks again. It draws in the
//! SDK's touch look ([`design::TOUCH`]).
//!
//! It reads the state the desktop's `ui/` reads (`state.rs`), plus the
//! sheet open over it, and writes only through `actions.rs`. One file per
//! Expo source:
//! - `account.rs`: `app/account/index.tsx`, the screen and its Account pane;
//! - `agents.rs`: `src/account/Agents.tsx`, the Agents pane and its sheets;
//! - `parts.rs`: the Expo kit's pieces both draw that the SDK lacks.
use view::design::{Kind as ButtonKind, Ladder};
use view::prelude::*;
use view::{Div, FontWeight, Stateful};

use super::{column, problem_text, sections, shown};
use crate::Settings;
use crate::queries::Seat;
use crate::state::{Form, Section, Sheet};

mod account;
mod agents;
mod parts;

/// The screen, and the sheet open over it; Back closes the sheet.
pub(crate) fn render(view: &Settings, cx: &mut Context<Settings>) -> impl IntoElement {
    let theme = *cx.global::<Theme>();
    let ladder = &design::TOUCH;
    div()
        .id("settings")
        .size_full()
        .flex()
        .flex_col()
        .bg(theme.background)
        .text_color(theme.foreground)
        .text_type(ladder.body)
        .when(view.sheet.is_some(), |root| {
            root.on_back(cx.listener(|v: &mut Settings, _: &BackEvent, _, cx| v.close_sheet(cx)))
        })
        .child(account::screen(view, cx, &theme, ladder))
        .children(agents::sheet(view, cx, &theme, ladder))
}
