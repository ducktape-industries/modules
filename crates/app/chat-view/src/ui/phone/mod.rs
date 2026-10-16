//! The phone's chat: the Expo app's chat stack (mobile
//! `app/(tabs)/chat/_layout.tsx`), one screen at a time, chosen once at the
//! view's root by `window.touch()`. Reads the page stack ([`Chat::pages`]),
//! the open message menu (an actions or picker sheet, the delete
//! confirmation) and the session; writes nothing itself: Back pops the
//! stack through [`Chat::back`]. Nothing under here asks whether the device
//! is touch: every screen is the Expo design, on the ladder's
//! [`design::TOUCH`] values. Without a node only the rooms page shows, and
//! says so.
use view::prelude::*;

use crate::{Chat, Page};
use view::design::Ladder;

mod actions;
mod composer;
mod details;
mod message;
mod new;
mod parts;
mod picker;
mod rich;
mod room;
mod rooms;
mod search;
mod thread;
mod timeline;

pub fn render(chat: &Chat, cx: &mut Context<Chat>) -> AnyElement {
    let theme = *cx.global::<Theme>();
    let ladder = &design::TOUCH;
    // Back has somewhere to go while a page is over the rooms or a sheet
    // is over the page (rule 13); the host leaves the program else
    let somewhere = chat.pages().depth() > 0 || chat.dialog_open();
    let back = (chat.session.connected && somewhere).then(|| {
        cx.listener(|chat, _: &BackEvent, _window, cx| {
            chat.back();
            cx.notify();
        })
    });
    let screen = div()
        .id("chat-root")
        .relative()
        .size_full()
        .flex()
        .flex_col()
        .bg(theme.background)
        .text_color(theme.foreground)
        .text_type(ladder.body)
        .role(Role::Group)
        .aria_label("Chat")
        .child(screen(chat, cx, &theme, ladder))
        .children(actions::sheet(chat, cx, &theme, ladder))
        .children(picker::sheet(chat, cx, &theme, ladder))
        .into_any_element();
    let screen = actions::with_confirm(chat, screen, cx, &theme, ladder);
    div()
        .id("chat")
        .size_full()
        .when_some(back, |root, back| root.on_back(back))
        .child(screen)
        .into_any_element()
}

/// The screen on top of the stack.
fn screen(chat: &Chat, cx: &mut Context<Chat>, theme: &Theme, ladder: &Ladder) -> AnyElement {
    if !chat.session.connected {
        return rooms::screen(chat, cx, theme, ladder);
    }
    let back = cx.listener(|chat, _: &ClickEvent, _window, cx| {
        chat.back();
        cx.notify();
    });
    match (chat.pages().top(), &chat.room) {
        (Page::Room, Some(room)) => room::screen(chat, room, back, cx, theme, ladder),
        (Page::Thread, Some(room)) => thread::screen(chat, room, back, cx, theme, ladder),
        (Page::Details, Some(room)) => details::screen(chat, room, back, cx, theme, ladder),
        (Page::New, _) => new::screen(chat, back, cx, theme, ladder),
        (Page::Search, _) => search::screen(chat, back, cx, theme, ladder),
        _ => rooms::screen(chat, cx, theme, ladder),
    }
}
