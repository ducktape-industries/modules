//! Native GPUI message lists. The room and thread use native variable-height lists;
//! callbacks still call the root view's existing message operations.

use ducktape_view_guest::design;
use ducktape_view_guest::prelude::*;
use ducktape_view_guest::{
    ClickEvent, Context, FollowMode, ListAlignment, ListSizingBehavior, ListState, ParentElement,
    Role, Styled, Theme, div, list as gpui_list, px,
};

use ducktape_view_guest::AnyElement;
use ducktape_view_guest::Loadable;

use crate::message::{ChatMessage, new_day, unread_seq};
use crate::ui::room::selection_bar;
use crate::ui::{message, quiet};
use crate::{Chat, Pane};

/// A row's height before the list has measured it: an overdraw budget, not
/// a layout size.
const UNMEASURED_ROW: f32 = 160.;

/// A pane's messages: the room's timeline or the open thread. Around the
/// list: paging older, the copy range's bar, the way back to the latest
/// message, and the edit field.
pub fn list(chat: &Chat, pane: Pane, cx: &mut Context<Chat>, theme: &Theme) -> impl IntoElement {
    let messages = chat.messages(pane);
    let content = div()
        .id(match pane {
            Pane::Timeline => "chat-timeline",
            Pane::Thread => "chat-thread-messages",
        })
        .flex_1()
        .min_h(px(0.))
        .flex()
        .flex_col();
    if messages.is_empty()
        && let Some(nothing) = nothing_yet(chat, pane, theme)
    {
        return content.child(nothing);
    }
    let timeline = pane == Pane::Timeline;
    content
        .when_some(older(chat, cx, theme).filter(|_| timeline), |el, older| {
            el.child(older)
        })
        .when(!messages.is_empty(), |el| {
            // the way back floats over the list's foot, taking no row of its own
            el.child(
                div()
                    .relative()
                    .flex()
                    .flex_col()
                    .flex_1()
                    .min_h(px(0.))
                    .child(rows(chat, pane, messages.clone(), cx, theme))
                    .when(timeline, |el| {
                        el.when_some(jump_to_latest(chat, cx, theme), |el, jump| el.child(jump))
                    }),
            )
        })
        .when(
            timeline && chat.copy.is_some_and(|copy| copy.pane == pane),
            |el| el.child(selection_bar(chat, cx, theme)),
        )
        .when_some(super::menu::editing(chat, pane, cx), |el, editing| {
            el.child(editing)
        })
}

/// What an empty pane says: loading, refused, no replies yet, or the
/// room's beginning. None while the room itself is gone.
fn nothing_yet(chat: &Chat, pane: Pane, theme: &Theme) -> Option<AnyElement> {
    let room = chat.room.as_ref()?;
    let rows = match pane {
        Pane::Timeline => Some(&room.messages),
        Pane::Thread => room.thread.as_ref().map(|thread| &thread.replies),
    };
    if rows.is_some_and(|rows| matches!(rows, Loadable::Loading(_))) {
        return Some(quiet("Loading messages…", theme).p_4().into_any_element());
    }
    if let Some(refusal) = rows.and_then(Loadable::failed) {
        return Some(
            quiet(refusal.message.clone(), theme)
                .p_4()
                .into_any_element(),
        );
    }
    Some(match pane {
        Pane::Thread => no_replies(theme).into_any_element(),
        Pane::Timeline => {
            let (name, dm) = beginning(chat)?;
            intro(&name, dm.as_deref(), theme).into_any_element()
        }
    })
}

/// The open room's name and, for a dm, its peer: what its intro names.
fn beginning(chat: &Chat) -> Option<(String, Option<String>)> {
    let room = chat.room.as_ref()?;
    let name = chat
        .info(&room.id)
        .map_or_else(|| room.id.clone(), |info| info.channel.name.clone());
    Some((name, super::sidebar::dm_peer(chat).map(|peer| peer.0)))
}

/// The way to older history, while there is some.
fn older(chat: &Chat, cx: &mut Context<Chat>, theme: &Theme) -> Option<AnyElement> {
    let room = chat
        .room
        .as_ref()
        .filter(|room| room.has_older && !room.landed)?;
    let control = if room.older_loading {
        div()
            .id("chat-load-older-button")
            .text_color(theme.muted)
            .child("Loading older messages…")
            .into_any_element()
    } else {
        let older = cx.listener(|chat, _: &ClickEvent, _window, cx| {
            cx.notify();
            chat.load_older(cx)
        });
        super::button(
            "chat-load-older-button",
            "Load older messages",
            theme,
            older,
        )
        .into_any_element()
    };
    let row = div()
        .id("chat-load-older")
        .flex()
        .justify_center()
        .p_2()
        .child(control);
    Some(row.into_any_element())
}

/// The native list: the room's intro first once its history is all here,
/// the unread divider over the first unread message, and "No replies yet"
/// under a thread that is only its root.
fn rows(
    chat: &Chat,
    pane: Pane,
    messages: Vec<ChatMessage>,
    cx: &mut Context<Chat>,
    theme: &Theme,
) -> impl IntoElement {
    let room = chat.room.as_ref();
    let lead = pane == Pane::Timeline && room.is_some_and(|room| !room.has_older && !room.landed);
    let lead_text = lead.then(|| beginning(chat)).flatten();
    let bare = pane == Pane::Thread
        && messages.len() == 1
        && room
            .and_then(|room| room.thread.as_ref())
            .is_some_and(|thread| thread.replies.ready().is_some_and(Vec::is_empty));
    let keys = lead
        .then_some("intro".to_owned())
        .into_iter()
        .chain(messages.iter().map(|message| message.id.clone()))
        .chain(bare.then_some("no-replies".to_owned()))
        .collect::<Vec<_>>();
    let state = list_state(chat, pane, &keys);
    state.set_follow_mode(match pane {
        Pane::Timeline => FollowMode::Tail,
        Pane::Thread => FollowMode::Normal,
    });
    state.set_scroll_handler(cx.listener(move |chat, event, _window, cx| {
        chat.list_scrolled(pane, event, cx);
    }));
    let theme = *theme;
    let unread = unread_seq(
        &messages,
        (pane == Pane::Timeline).then_some(chat.reads.boundary),
    );
    // One Tab stop, a grid: ↑ ↓ walk the messages (the list scrolls the
    // next one into view before it claims), ← → a message's cells (the
    // message, then its controls), Enter presses the active cell — on the
    // message, its click. Active by message id: the newest until the
    // arrows move.
    let cursor = chat.cursor(pane);
    let at = cursor
        .id
        .as_ref()
        .and_then(|id| messages.iter().position(|message| &message.id == id))
        .unwrap_or(messages.len().saturating_sub(1));
    let active_id = messages.get(at).map(|message| message.id.clone());
    let cell = cursor.cell;
    let keys: Vec<(String, u64, u32)> = messages
        .iter()
        .map(|message| (message.id.clone(), message.seq, message.rev))
        .collect();
    let reveal = state.clone();
    let moved_to = keys.clone();
    let cell_row = active_id.clone();
    let grid = design::composite(
        match pane {
            Pane::Timeline => "chat-message-list",
            Pane::Thread => "chat-thread-list",
        },
        Role::Grid,
        match pane {
            Pane::Timeline => "Messages",
            Pane::Thread => "Replies",
        },
    )
    .active(at, messages.len())
    // the cells are counted as the active row is drawn (`Cursor::controls`),
    // so the bound is the recorded count at the key, not at the build
    .cells(cell, usize::MAX)
    .on_move(cx.processor(move |chat, index: usize, _, cx| {
        let cursor = chat.cursor_mut(pane);
        cursor.id = Some(moved_to[index].0.clone());
        cursor.cell = 0;
        reveal.scroll_to_reveal_item(index + usize::from(lead));
        cx.notify();
    }))
    .on_move_cell(cx.processor(move |chat, cell: usize, _, cx| {
        let cursor = chat.cursor_mut(pane);
        let last = cursor
            .id
            .clone()
            .or_else(|| cell_row.clone())
            .map_or(0, |id| cursor.controls_of(&id).len());
        cursor.cell = cell.min(last);
        cx.notify();
    }))
    .on_press(cx.processor(move |chat, index: usize, window, cx| {
        let (id, seq, rev) = keys[index].clone();
        let cursor = chat.cursor(pane);
        // the controls recorded are this message's, else the row was not
        // drawn since the cursor moved and no control is pressed
        let control = match cursor.cell {
            0 => None,
            cell => Some(cursor.controls_of(&id).get(cell - 1).cloned()),
        };
        chat.layout.press = chat.key_spot(pane);
        cx.notify();
        match control {
            None => chat.press_message(pane, seq),
            Some(Some(control)) => chat.act(pane, seq, rev, control, window, cx),
            Some(None) => {}
        }
    }))
    .build();
    let list = gpui_list(
        state,
        cx.processor(move |chat, index: usize, _window, cx| {
            if lead && index == 0 {
                let (name, dm) = lead_text.clone().expect("lead row");
                return intro(&name, dm.as_deref(), &theme).into_any_element();
            }
            let at = index - usize::from(lead);
            let Some(message) = messages.get(at).cloned() else {
                return match bare {
                    true => no_replies(&theme).into_any_element(),
                    false => div().into_any_element(),
                };
            };
            let day = new_day(&messages, at)
                .map(|day| day_marker(&message.id, day, &theme).into_any_element());
            let unread = (unread == Some(message.seq)).then(|| unread_marker(&theme));
            let active = (active_id.as_ref() == Some(&message.id)).then_some(cell);
            let id = message.id.clone();
            let set = (at + 1, messages.len());
            let (card, controls) = message::card(chat, message, pane, active, set, cx, &theme);
            if active.is_some() {
                let cursor = chat.cursor_mut(pane);
                cursor.controls = controls;
                cursor.controls_of = Some(id);
            }
            if day.is_none() && unread.is_none() {
                return card;
            }
            // full width, as a bare card is: a row shrunk to its words
            // took the hover and the action strip with it
            div()
                .w_full()
                .flex()
                .flex_col()
                .children(day)
                .children(unread)
                .child(card)
                .into_any_element()
        }),
    )
    .with_sizing_behavior(ListSizingBehavior::Auto)
    .flex_1()
    .min_h(px(0.))
    .w_full();
    grid.flex().flex_col().flex_1().min_h(px(0.)).child(list)
}

/// "Jump to latest", when the timeline is not at its live tail.
fn jump_to_latest(chat: &Chat, cx: &mut Context<Chat>, theme: &Theme) -> Option<AnyElement> {
    let room = chat.room.as_ref()?;
    if !room.landed && room.at_tail {
        return None;
    }
    let id = room.id.clone();
    let latest = cx.listener(move |chat, _: &ClickEvent, _, cx| {
        cx.notify();
        chat.open(id.clone(), cx)
    });
    // a strip across the list's foot that only centres the button: it has
    // no handlers, so a click beside the button still reaches the row under it
    let jump = div()
        .id("chat-jump-latest")
        .absolute()
        .left_0()
        .right_0()
        .bottom_3()
        .flex()
        .justify_center()
        .child(div().bg(theme.background).shadow_lg().child(super::button(
            "chat-jump-latest-button",
            "Jump to latest",
            theme,
            latest,
        )));
    Some(jump.into_any_element())
}

fn list_state(chat: &Chat, pane: Pane, keys: &[String]) -> ListState {
    let (slot, remembered, alignment) = match pane {
        Pane::Timeline => (
            &chat.timeline_list,
            &chat.timeline_rows,
            ListAlignment::Bottom,
        ),
        Pane::Thread => (&chat.thread_list, &chat.thread_rows, ListAlignment::Top),
    };
    let mut slot = slot.borrow_mut();
    let mut old = remembered.borrow_mut();
    if slot.is_none() {
        let state = ListState::new(keys.len(), alignment, px(UNMEASURED_ROW));
        *slot = Some(state.clone());
        *old = keys.to_vec();
        return state;
    }
    let state = slot.as_ref().expect("initialized list state").clone();
    let prefix = old.iter().zip(keys).take_while(|(a, b)| a == b).count();
    let suffix = old[prefix..]
        .iter()
        .rev()
        .zip(keys[prefix..].iter().rev())
        .take_while(|(a, b)| a == b)
        .count();
    let old_end = old.len() - suffix;
    let new_end = keys.len() - suffix;
    if prefix != old_end || prefix != new_end {
        state.splice(prefix..old_end, new_end - prefix);
    }
    *old = keys.to_vec();
    state
}

/// The day the messages under it were posted on, between two days.
fn day_marker(id: &str, day: String, theme: &Theme) -> impl IntoElement {
    let rule = || div().h(px(1.)).flex_1().bg(theme.border);
    div()
        .id(format!("chat-day-{id}"))
        .flex()
        .items_center()
        .gap_2()
        .px_3()
        .py_1()
        .text_size(design::text::CAPTION)
        .text_color(theme.muted)
        .child(rule())
        .child(day)
        .child(rule())
}

fn unread_marker(theme: &Theme) -> impl IntoElement {
    div()
        .id("chat-unread-marker")
        .flex()
        .items_center()
        .gap_2()
        .px_3()
        .py_1()
        .text_size(design::text::SECONDARY)
        .text_color(theme.accent_foreground)
        .child(div().h(px(1.)).flex_1().bg(theme.accent))
        .child("New messages")
}

/// The top of a room's history. The room's name is already its header's,
/// so this says only where the history starts.
fn intro(name: &str, dm: Option<&str>, theme: &Theme) -> impl IntoElement {
    let detail = match dm {
        Some(peer) => format!("This is the very beginning of your conversation with {peer}."),
        None => format!(
            "This is the very beginning of #{name}. Say hello, or pin what the room is for."
        ),
    };
    div()
        .id("chat-timeline-intro")
        .px_6()
        .pt_6()
        .pb_3()
        .text_size(design::text::SECONDARY)
        .text_color(theme.muted)
        .child(detail)
}

/// A thread with nothing under its root yet.
fn no_replies(theme: &Theme) -> impl IntoElement {
    div()
        .id("chat-thread-no-replies")
        .px_4()
        .py_3()
        .text_size(design::text::SECONDARY)
        .text_color(theme.muted)
        .child("No replies yet")
}
