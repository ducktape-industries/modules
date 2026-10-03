//! The channel creation dialog.

use ducktape_view_guest::design;
use ducktape_view_guest::prelude::*;
use ducktape_view_guest::{
    AnyElement, ClickEvent, Context, ParentElement, Styled, Theme, div, px, wire,
};

use crate::{ChannelCreate, Chat};

/// How wide the dialog's card grows.
const CARD_MAX_WIDTH: f32 = 480.;

pub fn channel_create(chat: &Chat, cx: &mut Context<Chat>, theme: &Theme) -> Option<AnyElement> {
    let create = chat.create.as_ref()?;
    let busy = create.busy;
    let can_submit = !busy && chat.session.connected && chat.holds_account();
    let cancel = cx.listener(|chat, _: &ClickEvent, _window, cx| {
        chat.create = None;
        cx.notify();
    });
    let submit = cx.listener(|chat, _: &ClickEvent, _window, cx| {
        cx.notify();
        chat.create_channel(cx)
    });
    let card = div()
        .id("chat-create-card")
        .max_w(px(CARD_MAX_WIDTH))
        .flex()
        .flex_col()
        .gap_2()
        .p_5()
        .border_1()
        .border_color(theme.border)
        .bg(theme.background)
        .shadow_lg()
        .child(
            div()
                .text_size(design::text::TITLE)
                .child("Create a channel"),
        )
        .child(
            div()
                .text_size(design::text::SECONDARY)
                .text_color(theme.muted)
                .child("Channel name"),
        )
        .child(name_field(create, can_submit, cx, theme))
        .children(notes(chat, create, theme))
        // one row, as the other views' dialogs: the option, then Cancel
        // beside the primary action
        .child(
            div()
                .flex()
                .flex_wrap()
                .gap_2()
                .child(members_only(create, cx, theme))
                .child(div().flex_1())
                .child(design::button("chat-create-cancel", "Cancel", theme, cancel).enabled(!busy))
                .child(
                    design::button("chat-create-submit", "Create channel", theme, submit)
                        .kind(design::Kind::Primary)
                        .enabled(can_submit),
                ),
        );
    Some(card.into_any_element())
}

/// The name, typed; Enter creates where the dialog may.
fn name_field(
    create: &ChannelCreate,
    can_submit: bool,
    cx: &mut Context<Chat>,
    theme: &Theme,
) -> Input {
    let typed = cx.listener(|chat, change: &wire::TextChange, _window, cx| {
        if let Some(create) = &mut chat.create {
            create.name.apply(change);
        }
        cx.notify();
    });
    let name = Input::new("chat-create-name", "Name the new channel")
        .h(design::size::CONTROL)
        .px_2()
        .py_1()
        .border_1()
        .border_color(theme.border_strong)
        .bg(theme.surface)
        .value(&create.name)
        .placeholder("Channel name")
        .disabled(create.busy)
        .on_change(typed);
    match can_submit {
        true => name.on_submit(cx.listener(|chat, _: &(), _window, cx| {
            cx.notify();
            chat.create_channel(cx)
        })),
        false => name,
    }
}

/// Members only, a toggle: on, posting takes a seat.
fn members_only(create: &ChannelCreate, cx: &mut Context<Chat>, theme: &Theme) -> AnyElement {
    let toggle = cx.listener(|chat, _: &ClickEvent, _window, cx| {
        if let Some(create) = &mut chat.create {
            create.members_only = !create.members_only;
        }
        cx.notify();
    });
    design::button("chat-create-members", "Members only", theme, toggle)
        .selected(create.members_only)
        .enabled(!create.busy)
        .into_any_element()
}

/// Why the dialog failed, and why it cannot create at all.
fn notes(chat: &Chat, create: &ChannelCreate, theme: &Theme) -> Vec<AnyElement> {
    let note = |text: String, color| {
        div()
            .text_size(design::text::SECONDARY)
            .text_color(color)
            .child(text)
            .into_any_element()
    };
    let mut notes = Vec::new();
    if !create.error.is_empty() {
        notes.push(note(create.error.clone(), theme.danger));
    }
    if !chat.holds_account() {
        let why = "Create an account to create a channel".to_owned();
        notes.push(note(why, theme.muted));
    }
    notes
}
