//! What a pressed message offers (Expo `src/chat/Actions.tsx` `Actions`,
//! design §4.4, boards 40/41/43): a sheet with who wrote it, when and in
//! which block, the six reactions the reader uses most and the way to all
//! of them, then Reply in thread, Copy text, Copy link, and Edit or Delete
//! where the reader may; and the kit `Dialog` a delete asks first. Reads
//! the open menu, the message and the reader's recent reactions; each
//! action closes the sheet and runs through the model (react, the thread,
//! the clipboard, an edit, the delete).
use view::prelude::*;
use view::{Div, Stateful};

use super::picker::{self, Cell};
use crate::composer::Target;
use crate::message::ChatMessage;
use crate::ui::menu::CANCEL_DELETE;
use crate::{Chat, Mode, Pane, emoji};
use view::FontWeight;
use view::design::Ladder;

/// How many of the reader's reactions the sheet offers (mobile
/// `src/chat/Actions.tsx:91`), and the all-reactions button's plus
/// (`:102`).
const QUICK: usize = 6;
const ALL_REACTIONS_ICON: Pixels = px(20.);
/// The delete dialog's inset (mobile `src/kit/Dialog.tsx:32`).
const DIALOG_INSET: Pixels = px(20.);
/// The scrim under a dialog, by appearance (mobile
/// `src/design/theme.ts:12` and `:13`).
const SCRIM_LIGHT: f32 = 0.4;
const SCRIM_DARK: f32 = 0.55;

/// The actions sheet, while a message's menu is open.
pub(super) fn sheet(
    chat: &Chat,
    cx: &mut Context<Chat>,
    theme: &Theme,
    ladder: &Ladder,
) -> Option<AnyElement> {
    let menu = chat
        .menu
        .as_ref()
        .filter(|menu| matches!(menu.mode, Mode::Toolbar | Mode::More))?;
    let pane = menu.pane;
    let messages = chat.messages(pane);
    let message = messages.iter().find(|message| message.seq == menu.seq)?;
    let close = cx.listener(|chat, _: &(), _window, cx| {
        chat.close_menu();
        cx.notify();
    });
    let body = div()
        .id("chat-actions")
        .flex()
        .flex_col()
        .child(head(message, theme, ladder))
        .children((!message.deleted).then(|| quick(chat, message, pane, cx, theme)))
        .children(rows(chat, message, pane, cx, theme, ladder));
    Some(
        design::sheet("chat-actions-sheet", "Message actions", ladder, theme)
            .on_dismiss(close)
            .child(body)
            .into_any_element(),
    )
}

/// Who wrote it, when and in which block; what it says, on one line.
fn head(message: &ChatMessage, theme: &Theme, ladder: &Ladder) -> Stateful<Div> {
    let when = (message.time > 0).then(|| design::clock(message.time));
    let about = [
        Some(message.author.clone()),
        when,
        Some(format!("block {}", message.height)),
    ]
    .into_iter()
    .flatten()
    .collect::<Vec<_>>()
    .join(" · ");
    design::sheet_head("chat-actions-head", theme)
        .child(
            div()
                .truncate()
                .text_type(ladder.caption)
                .text_color(theme.muted)
                .child(about),
        )
        .child(
            div()
                .truncate()
                .text_type(ladder.body)
                .child(message.body.clone()),
        )
}

/// The reader's six reactions (theirs on this message on the accent), and
/// the way to all of them.
fn quick(
    chat: &Chat,
    message: &ChatMessage,
    pane: Pane,
    cx: &mut Context<Chat>,
    theme: &Theme,
) -> Stateful<Div> {
    let (seq, rev) = (message.seq, message.rev);
    let cells: Vec<AnyElement> = emoji::frequent(&chat.recent_emoji)
        .into_iter()
        .take(QUICK)
        .map(|emoji| {
            let mine = message
                .reactions
                .iter()
                .any(|reaction| reaction.emoji == emoji && reaction.reacted_by_me);
            let react = emoji.clone();
            let press = cx.listener(move |chat, _: &ClickEvent, _window, cx| {
                cx.notify();
                chat.close_menu();
                chat.react(seq, react.clone(), !mine, cx)
            });
            picker::cell(
                Cell {
                    id: format!("chat-actions-react-{emoji}").into(),
                    emoji,
                    mine,
                },
                press,
                theme,
            )
        })
        .collect();
    let all = cx.listener(move |chat, _: &ClickEvent, window, cx| {
        cx.notify();
        chat.open_menu(pane, seq, rev, Mode::Reactions, window, cx)
    });
    let all = div()
        .id("chat-actions-all-reactions")
        .size(picker::CELL)
        .flex()
        .items_center()
        .justify_center()
        .border_1()
        .border_color(theme.border_strong)
        .role(Role::Button)
        .aria_label("All reactions")
        .focusable()
        .on_click(all)
        .child(design::icon("plus", ALL_REACTIONS_ICON).text_color(theme.muted));
    div()
        .id("chat-actions-quick")
        .flex()
        .justify_between()
        .px(design::space::BLOCK)
        .py(design::space::MD)
        .border_b_1()
        .border_color(theme.border)
        .children(cells)
        .child(all)
}

/// The sheet's rows: only what the reader may do to this message.
fn rows(
    chat: &Chat,
    message: &ChatMessage,
    pane: Pane,
    cx: &mut Context<Chat>,
    theme: &Theme,
    ladder: &Ladder,
) -> Vec<AnyElement> {
    let (seq, rev, deleted) = (message.seq, message.rev, message.deleted);
    let in_thread = pane == Pane::Thread || message.thread.is_some();
    let mine = chat.wrote(pane, seq);
    let link = chat.message_link(seq);
    let row =
        |id: &'static str, icon: &'static str, label: &'static str, danger: bool, click: Press| {
            design::sheet_row(id, Some(icon), label, danger, ladder, theme, click)
                .into_any_element()
        };
    let mut rows = Vec::new();
    if !deleted && !in_thread {
        let channel = chat.room_id();
        let reply = cx.listener(move |chat, _: &ClickEvent, window, cx| {
            cx.notify();
            chat.open_thread(seq, cx);
            let reply = Target::Post {
                channel: channel.clone(),
                thread: Some(seq),
            };
            crate::composer::focus(window, &reply.key());
        });
        rows.push(row(
            "chat-actions-reply",
            "corner-up-left",
            "Reply in thread",
            false,
            Box::new(reply),
        ));
    }
    if !deleted {
        let text = message.body.clone();
        let copy = cx.listener(move |chat, _: &ClickEvent, _window, cx| {
            chat.close_menu();
            chat.copy_text(text.clone(), "message text", cx);
        });
        rows.push(row(
            "chat-actions-copy",
            "copy",
            "Copy text",
            false,
            Box::new(copy),
        ));
    }
    if let Some(link) = link {
        let copy = cx.listener(move |chat, _: &ClickEvent, _window, cx| {
            chat.close_menu();
            chat.copy_text(link.clone(), "message link", cx);
        });
        rows.push(row(
            "chat-actions-copy-link",
            "link-2",
            "Copy link",
            false,
            Box::new(copy),
        ));
    }
    let reopen = |mode: Mode, cx: &mut Context<Chat>| -> Press {
        Box::new(cx.listener(move |chat, _: &ClickEvent, window, cx| {
            cx.notify();
            chat.open_menu(pane, seq, rev, mode, window, cx)
        }))
    };
    if !deleted && mine {
        rows.push(row(
            "chat-actions-edit",
            "pencil",
            "Edit message",
            false,
            reopen(Mode::Editing, cx),
        ));
    }
    if !deleted && (mine || chat.owns_room()) {
        rows.push(row(
            "chat-actions-delete",
            "trash",
            "Delete message",
            true,
            reopen(Mode::Delete, cx),
        ));
    }
    rows
}

/// A press on one of the sheet's rows.
type Press = Box<dyn Fn(&ClickEvent, &mut Window, &mut App) + 'static>;

/// `screen` under the delete dialog, while a delete asks: dimmed, a press
/// on the scrim (or Back) keeps the message.
pub(super) fn with_confirm(
    chat: &Chat,
    screen: AnyElement,
    cx: &mut Context<Chat>,
    theme: &Theme,
    ladder: &Ladder,
) -> AnyElement {
    let asking = chat
        .menu
        .as_ref()
        .is_some_and(|menu| menu.mode == Mode::Delete);
    let cancel = cx.listener(|chat, _: &(), _window, cx| {
        chat.close_menu();
        cx.notify();
    });
    let scrim = match theme.dark {
        true => SCRIM_DARK,
        false => SCRIM_LIGHT,
    };
    let dialog = asking.then(|| delete_dialog(cx, theme, ladder));
    modal_overlay(
        "chat-delete-overlay",
        "Delete this message?",
        screen,
        dialog,
    )
    .flex()
    .items_center()
    .justify_center()
    .px(design::space::XL)
    .backdrop(hsla(0., 0., 0., scrim))
    .on_dismiss(cancel)
    .into_any_element()
}

/// The kit `Dialog` for a delete: its question, what it does, then Cancel
/// (quiet, where the keys land) and Delete (danger) at the right.
fn delete_dialog(cx: &mut Context<Chat>, theme: &Theme, ladder: &Ladder) -> Stateful<Div> {
    let cancel = cx.listener(|chat, _: &ClickEvent, _window, cx| {
        chat.close_menu();
        cx.notify();
    });
    let delete = cx.listener(|chat, _: &ClickEvent, _window, cx| {
        cx.notify();
        chat.delete_armed(cx)
    });
    // the kit's danger button: its words and edge in the danger tone
    let delete = div()
        .id("chat-menu-confirm-delete")
        .h(ladder.control)
        .px(design::space::BLOCK)
        .flex()
        .items_center()
        .border_1()
        .border_color(theme.danger)
        .text_color(theme.danger)
        .text_type(ladder.label)
        .font_weight(FontWeight::MEDIUM)
        .role(Role::Button)
        .focusable()
        .active(|style| style.opacity(super::parts::PRESSED))
        .on_click(delete)
        .child("Delete");
    div()
        .id("chat-delete-dialog")
        .w_full()
        .flex()
        .flex_col()
        .pt(DIALOG_INSET)
        .px(DIALOG_INSET)
        .pb(design::space::LG)
        .border_1()
        .border_color(theme.border_strong)
        .bg(theme.background)
        .role(Role::AlertDialog)
        .aria_label("Delete this message?")
        .occlude()
        .child(
            div()
                .mb(design::space::SM)
                .text_type(ladder.title)
                .font_weight(FontWeight::MEDIUM)
                .child("Delete this message?"),
        )
        .child(
            div()
                .text_type(ladder.small)
                .text_color(theme.muted)
                .child("It leaves the room for everyone. What stays is its place in the timeline."),
        )
        .child(
            div()
                .flex()
                .justify_end()
                .gap(design::space::XXS)
                .mt(design::space::BLOCK)
                .child(
                    design::button(CANCEL_DELETE, "Cancel", ladder, theme, cancel)
                        .kind(design::Kind::Quiet),
                )
                .child(delete),
        )
}
