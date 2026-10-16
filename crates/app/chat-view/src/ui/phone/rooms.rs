//! The rooms, the chat's first page (Expo `app/(tabs)/chat/index.tsx`,
//! boards 10-12): "Chat" over the network and the reader, Search and New
//! channel in its title row; then the channels, the direct rooms already
//! open and the people not yet written to, each section with its count,
//! each unread room with its dot. Reads the channel list, the roster and
//! the read cursors; a row opens its room, the title row's buttons open
//! the Search page and the new-channel form.
use view::prelude::*;
use view::{Div, Stateful};

use super::parts::{self, face, quiet};
use crate::Chat;
use crate::room_list::DmEntry;
use crate::words;
use chat::{ChannelInfo, Principal};
use view::FontWeight;
use view::design::Ladder;

/// The unread dot (mobile `src/kit/Badge.tsx:31`).
const DOT: Pixels = px(8.);
// SDK gap: the kit EmptyState, swap when feat/phone-kit lands
/// The kit `EmptyState`'s icon, its padding and how wide its words run
/// (mobile `src/design/theme.ts:87`, `src/kit/States.tsx:26` and `:31`).
const EMPTY_ICON: Pixels = px(28.);
const EMPTY_PAD_Y: Pixels = px(40.);
const EMPTY_PAD_X: Pixels = px(32.);
const EMPTY_WORDS: Pixels = px(280.);

pub(super) fn screen(
    chat: &Chat,
    cx: &mut Context<Chat>,
    theme: &Theme,
    ladder: &Ladder,
) -> AnyElement {
    let search = cx.listener(|chat, _: &ClickEvent, window, cx| {
        chat.open_search();
        window.focus("search");
        cx.notify();
    });
    let create = cx.listener(|chat, _: &ClickEvent, _window, cx| {
        chat.begin_create();
        cx.notify();
    });
    let page = design::page(
        "chat-rooms-page",
        words::CHAT,
        design::Nav::root(chat.reader_name()),
        ladder,
        theme,
    )
    .trailing(design::icon_button(
        "chat-rooms-search",
        design::icon("search", ladder.icon),
        "Search",
        ladder,
        theme,
        search,
    ))
    .trailing(design::icon_button(
        "chat-rooms-new-channel",
        design::icon("plus", ladder.icon),
        words::NEW_CHANNEL,
        ladder,
        theme,
        create,
    ));
    if !chat.session.connected {
        return page
            .child(quiet(
                "chat-rooms-offline",
                "Not connected to a node.",
                theme,
                ladder,
            ))
            .into_any_element();
    }
    let page = page.caption(caption(chat));
    match chat.channels.failed() {
        Some(refusal) => {
            let retry = cx.listener(|chat, _: &ClickEvent, _window, cx| {
                cx.notify();
                chat.load_channels(cx);
            });
            page.child(parts::refused(
                "chat-rooms",
                refusal.message.clone(),
                theme,
                ladder,
                retry,
            ))
        }
        None => page.child(rooms(chat, cx, theme, ladder)),
    }
    .into_any_element()
}

/// The title's caption: the network, then the reader's name.
fn caption(chat: &Chat) -> String {
    let me = chat
        .my_account()
        .zip(chat.names.ready())
        .map(|(me, names)| names.member(&Principal::Account(me)));
    [Some(chat.network().to_owned()), me]
        .into_iter()
        .flatten()
        .filter(|word| !word.is_empty())
        .collect::<Vec<_>>()
        .join(" · ")
}

/// The three sections: Channels always (its loading line or empty state
/// under it), then Direct messages and People where they have rows.
fn rooms(chat: &Chat, cx: &mut Context<Chat>, theme: &Theme, ladder: &Ladder) -> Stateful<Div> {
    let list = chat.room_list();
    let opened = list.opened();
    let (dms, people) = list.dms.split_at(opened);
    let count = |rows: usize| (rows > 0).then(|| rows.to_string());
    let loading = matches!(chat.channels, Loadable::Idle | Loadable::Loading(_));
    div()
        .id("chat-rooms")
        .flex_1()
        .min_h(px(0.))
        .overflow_y_scroll()
        .flex()
        .flex_col()
        .child(
            design::section_head("chat-rooms-channels-head", words::CHANNELS, ladder, theme)
                .children(count(list.channels.len())),
        )
        .children(
            list.channels
                .iter()
                .map(|info| channel_row(chat, info, cx, theme, ladder)),
        )
        .when(loading, |rooms| {
            rooms.child(quiet(
                "chat-rooms-loading",
                words::LOADING_ROOMS,
                theme,
                ladder,
            ))
        })
        .when(!loading && list.channels.is_empty(), |rooms| {
            rooms.child(no_rooms(chat, cx, theme, ladder))
        })
        .when(!dms.is_empty(), |rooms| {
            rooms.child(
                design::section_head("chat-rooms-dms-head", words::DIRECT_MESSAGES, ladder, theme)
                    .children(count(dms.len())),
            )
        })
        .children(
            dms.iter()
                .map(|dm| person_row(chat, dm, "dm", cx, theme, ladder)),
        )
        .when(!people.is_empty(), |rooms| {
            rooms.child(
                design::section_head("chat-rooms-people-head", words::PEOPLE, ladder, theme)
                    .children(count(people.len())),
            )
        })
        .children(
            people
                .iter()
                .map(|person| person_row(chat, person, "person", cx, theme, ladder)),
        )
}

/// A channel's row: its hash (an archived room's box) muted in the lead
/// column, the name (muted when archived), Members only and Archived, and
/// the dot while it holds what the reader has not read.
fn channel_row(
    chat: &Chat,
    info: &ChannelInfo,
    cx: &mut Context<Chat>,
    theme: &Theme,
    ladder: &Ladder,
) -> AnyElement {
    let channel = &info.channel;
    let id = format!("chat-rooms-channel-{}", channel.id);
    let archived = channel.archived;
    let glyph = design::icon(if archived { "archive" } else { "hash" }, ladder.icon_row)
        .text_color(theme.muted);
    let chip = |suffix: &str, label: &'static str| {
        design::badge(
            format!("{id}-{suffix}"),
            label,
            ladder,
            theme.muted,
            theme.surface_raised,
        )
    };
    let tail = div()
        .flex()
        .flex_none()
        .items_center()
        .gap(design::space::SM)
        .when(channel.members_only(), |tail| {
            tail.child(chip("members-only", words::MEMBERS_ONLY))
        })
        .when(archived, |tail| {
            tail.child(chip("archived", words::ARCHIVED))
        })
        .when(chat.unread(info), |tail| tail.child(dot(&id, theme)));
    row(
        &id,
        channel.name.clone(),
        channel.id.clone(),
        cx,
        theme,
        ladder,
    )
    .child(design::lead(glyph))
    .child(title(channel.name.clone(), archived, theme, ladder))
    .child(tail)
    .into_any_element()
}

/// A person's row: their face and name; an open direct room's also wears
/// Agent for an agent and the unread dot. `kind` names the section in the
/// row's id (`dm`, `person`).
fn person_row(
    chat: &Chat,
    entry: &DmEntry,
    kind: &str,
    cx: &mut Context<Chat>,
    theme: &Theme,
    ladder: &Ladder,
) -> AnyElement {
    let peer = Principal::Account(entry.peer);
    let names = chat.names.ready();
    let name = names.map_or_else(|| words::account(entry.peer), |n| n.member(&peer));
    let agent = names.is_some_and(|names| crate::message::agent(names, &peer));
    let id = format!("chat-rooms-{kind}-{}", entry.peer);
    let unread = entry.info.is_some_and(|info| chat.unread(info));
    let tail = div()
        .flex()
        .flex_none()
        .items_center()
        .gap(design::space::SM)
        .when(agent && entry.info.is_some(), |tail| {
            tail.child(design::badge(
                format!("{id}-agent"),
                words::AGENT,
                ladder,
                theme.agent,
                theme.agent_soft,
            ))
        })
        .when(unread, |tail| tail.child(dot(&id, theme)));
    row(&id, name.clone(), entry.id.clone(), cx, theme, ladder)
        .when(agent, |row| row.aria_description(words::AGENT))
        .child(design::lead(face(&name, agent, ladder.avatar, theme)))
        .child(title(name, false, theme, ladder))
        .child(tail)
        .into_any_element()
}

/// The kit `Row` as a room's: the whole row opens `room`.
fn row(
    id: &str,
    name: String,
    room: String,
    cx: &mut Context<Chat>,
    theme: &Theme,
    ladder: &Ladder,
) -> Stateful<Div> {
    let open = cx.listener(move |chat, _: &ClickEvent, _window, cx| {
        cx.notify();
        chat.choose(room.clone(), cx)
    });
    design::row(id.to_owned(), ladder, theme)
        .role(Role::Button)
        .aria_label(name)
        .focusable()
        .on_click(open)
}

/// A row's title (the kit `Row`'s): body text on one line, muted for an
/// archived room.
fn title(name: String, quiet: bool, theme: &Theme, ladder: &Ladder) -> Div {
    div()
        .flex_1()
        .min_w(px(0.))
        .truncate()
        .text_type(ladder.body)
        .text_color(if quiet { theme.muted } else { theme.foreground })
        .child(name)
}

// SDK gap: the kit Dot, kept local until the kit takes it (kit.md §2)
/// The unread dot (the kit `Dot`): the accent, never a count.
fn dot(row: &str, theme: &Theme) -> Stateful<Div> {
    div()
        .id(format!("{row}-unread"))
        .size(DOT)
        .flex_none()
        .rounded_full()
        .bg(theme.accent)
}

// SDK gap: the kit EmptyState, swap when feat/phone-kit lands
/// No channel on the network yet (the kit `EmptyState`): what is missing,
/// what would fill it, and New channel.
fn no_rooms(chat: &Chat, cx: &mut Context<Chat>, theme: &Theme, ladder: &Ladder) -> Stateful<Div> {
    let create = cx.listener(|chat, _: &ClickEvent, _window, cx| {
        chat.begin_create();
        cx.notify();
    });
    let network = match chat.network() {
        "" => "this network",
        network => network,
    };
    div()
        .id("chat-rooms-empty")
        .flex()
        .flex_col()
        .items_center()
        .gap(design::space::SM)
        .py(EMPTY_PAD_Y)
        .px(EMPTY_PAD_X)
        .child(design::icon("message-square", EMPTY_ICON).text_color(theme.faint))
        .child(
            div()
                .text_center()
                .text_type(ladder.body)
                .font_weight(FontWeight::MEDIUM)
                .child("No rooms yet"),
        )
        .child(
            div()
                .max_w(EMPTY_WORDS)
                .text_center()
                .text_type(ladder.secondary)
                .text_color(theme.muted)
                .child(format!(
                    "Nobody has created a channel on {network}. Open one, or message a person from the list below."
                )),
        )
        .child(
            parts::small_button(
                "chat-rooms-empty-new-channel",
                None,
                words::NEW_CHANNEL,
                theme,
                ladder,
                create,
            )
            .mt(design::space::SM),
        )
}
