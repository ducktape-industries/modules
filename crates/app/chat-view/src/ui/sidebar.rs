//! The channel and direct-message pane, authored as native GPUI elements.

use ducktape_view_guest::AnyElement;
use ducktape_view_guest::design;
use ducktape_view_guest::prelude::*;
use ducktape_view_guest::{
    ClickEvent, Context, ElementId, ParentElement, Role, StyleRefinement, Styled, Theme,
    WeakEntity, div, px, wire,
};

use chat::Principal;

use crate::names::dm_peer_of;
use crate::{ChannelCreate, Chat, RoomRow, Rooms, RoomsShown};

pub fn render(chat: &Chat, cx: &mut Context<Chat>, theme: &Theme) -> impl IntoElement {
    div()
        .id("chat-sidebar")
        .flex()
        .flex_col()
        .w(px(chat.layout.sidebar))
        .h_full()
        .bg(theme.sidebar)
        .text_color(theme.sidebar_foreground)
        .child(
            div()
                .id("chat-sidebar-search-row")
                .flex()
                .items_center()
                .gap_1()
                .p_2()
                .child(search(chat, cx, theme)),
        )
        .child(rooms(chat, cx))
}

/// The rooms list, kept across frames in a box that takes the column's
/// share as the list did (`flex_1`): the list renders only when what it
/// shows moved, or its arrows did.
fn rooms(chat: &Chat, cx: &mut Context<Chat>) -> impl IntoElement {
    let rooms = chat.rooms().clone();
    let shown = chat.rooms_shown();
    rooms.update(cx, |rooms, cx| rooms.show(shown, cx));
    rooms.cached(StyleRefinement::default().flex_1())
}

/// The message search field, and its clear button once it holds a search.
fn search(chat: &Chat, cx: &mut Context<Chat>, theme: &Theme) -> impl IntoElement {
    let typed = cx.listener(|chat, change: &wire::TextChange, _window, cx| {
        chat.search.draft.apply(change);
        cx.notify();
    });
    let submit = cx.listener(|chat, _: &(), _window, cx| {
        cx.notify();
        chat.search_submit(cx)
    });
    let input = Input::new("chat-sidebar-search", "Search messages")
        .h(design::size::CONTROL)
        .flex_1()
        .px_2()
        .py_1()
        .border_1()
        .border_color(theme.sidebar_border)
        .bg(theme.sidebar_raised)
        .text_color(theme.sidebar_foreground)
        .value(&chat.search.draft)
        .placeholder("Search messages…")
        .on_change(typed)
        .on_submit(submit);
    let searching = !chat.search.query.is_empty() || !chat.search.draft.is_blank();
    let clear = cx.listener(|chat, _: &ClickEvent, _window, cx| {
        chat.search_clear();
        cx.notify();
    });
    div()
        .flex()
        .items_center()
        .gap_1()
        .flex_1()
        .child(input)
        .when(searching, |el| {
            // the rail's ink at rest, as the field beside it
            el.child(
                design::icon_button(
                    "chat-sidebar-clear-search",
                    "✕",
                    "Clear search",
                    theme,
                    clear,
                )
                .text_color(theme.sidebar_foreground),
            )
        })
}

/// The rooms as the sidebar draws them: channels and direct messages, a
/// program's own rooms (`forge:…` review threads) left to it to show.
impl Chat {
    pub(crate) fn rooms_shown(&self) -> RoomsShown {
        let open = self.room.as_ref().map(|room| room.id.as_str());
        let mine = self.my_account();
        let names = self.names.ready();
        let (mut channels, mut dms) = (Vec::new(), Vec::new());
        for info in self.channels.ready().into_iter().flatten() {
            let id = info.channel.id.as_str();
            if chat::namespace::program(id).is_some() {
                continue;
            }
            let selected = open == Some(id);
            let row = |name: String, peer| RoomRow {
                id: id.to_owned(),
                name,
                peer,
                unread: self.unread(info) && !selected,
                selected,
                members_only: info.channel.members_only(),
                archived: info.channel.archived,
            };
            if chat::dm_peers(id).is_some() {
                let Some(peer) = mine.and_then(|mine| dm_peer_of(mine, id)) else {
                    continue;
                };
                let name = names.map_or_else(
                    || format!("account {peer}"),
                    |n| n.member(&Principal::Account(peer)),
                );
                let agent =
                    names.is_some_and(|n| crate::message::agent(n, &Principal::Account(peer)));
                dms.push(row(name, Some((peer, agent))));
            } else {
                channels.push(row(info.channel.name.clone(), None));
            }
        }
        RoomsShown {
            loading: self.channels.is_loading() && channels.is_empty() && dms.is_empty(),
            failed: self
                .channels
                .failed()
                .map(|refusal| refusal.message.clone()),
            create_open: self.create.is_some(),
            channels,
            dms,
        }
    }
}

impl Rooms {
    pub(crate) fn new(chat: WeakEntity<Chat>) -> Self {
        Self {
            chat,
            shown: RoomsShown::default(),
            cursor: None,
        }
    }

    /// What the root shows: a render when it moved.
    pub(crate) fn show(&mut self, shown: RoomsShown, cx: &mut Context<Self>) {
        if self.shown != shown {
            self.shown = shown;
            cx.notify();
        }
    }
}

impl Render for Rooms {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = *cx.global::<Theme>();
        let theme = &theme;
        let shown = &self.shown;
        let mut list = div()
            .id("chat-sidebar-rooms")
            .size_full()
            .overflow_y_scroll()
            .flex()
            .flex_col()
            .gap_1()
            .p_2()
            .child(section_header(
                "chat-sidebar-channels-header",
                "Channels",
                new_channel(shown.create_open, cx, theme),
                theme,
            ));
        if shown.loading {
            list = list.child(quiet("chat-sidebar-loading", "Loading rooms…", theme));
        }
        if let Some(refusal) = &shown.failed {
            list = list.child(quiet("chat-sidebar-failed", refusal.clone(), theme));
        }
        // one Tab stop under the header: ↑ ↓ walk the channels and the direct
        // messages, Enter opens the active room. Active: where the arrows are,
        // else the open room, else the first
        let rows: Vec<&RoomRow> = shown.channels.iter().chain(&shown.dms).collect();
        let ids: Vec<String> = rows.iter().map(|row| row.id.clone()).collect();
        let open = rows
            .iter()
            .find(|row| row.selected)
            .map(|row| row.id.as_str());
        let at = self
            .cursor
            .as_deref()
            .or(open)
            .and_then(|id| ids.iter().position(|room| room == id))
            .unwrap_or(0);
        let picked = ids.clone();
        let mut box_ = design::composite("chat-sidebar-rooms-list", Role::ListBox, "Rooms")
            .active(at, ids.len())
            .on_move(cx.processor(move |rooms, index: usize, _, cx| {
                rooms.cursor = Some(ids[index].clone());
                cx.notify();
            }))
            .on_press(cx.processor(move |rooms, index: usize, _, cx| {
                rooms.choose(picked[index].clone(), cx)
            }))
            .build()
            .flex()
            .flex_col()
            .gap_1();
        let mut index = 0;
        for row in &shown.channels {
            box_ = box_.child(channel_button(row, index == at, cx, theme));
            index += 1;
        }
        if !shown.dms.is_empty() {
            box_ = box_.child(section_header(
                "chat-sidebar-dm-header",
                "Direct messages",
                div(),
                theme,
            ));
            for row in &shown.dms {
                box_ = box_.child(dm_button(row, index == at, cx, theme));
                index += 1;
            }
        }
        list.child(box_)
    }
}

impl Rooms {
    /// Opens `id`: the root's.
    fn choose(&mut self, id: String, cx: &mut Context<Self>) {
        let _ = self.chat.update(cx, |chat, cx| {
            cx.notify();
            chat.choose(id, cx)
        });
    }
}

/// "+ New channel", or "✕ Close" while the create dialog is open.
fn new_channel(open: bool, cx: &mut Context<Rooms>, theme: &Theme) -> impl IntoElement {
    let toggle = cx.listener(|rooms, _: &ClickEvent, _window, cx| {
        let _ = rooms.chat.update(cx, |chat, cx| {
            chat.create = match chat.create.take() {
                Some(_) => None,
                None => Some(ChannelCreate::default()),
            };
            cx.notify();
        });
    });
    div()
        .id("chat-sidebar-new-channel")
        // the smallest box a pointer presses, each way (the door's AX-017)
        .min_w(px(24.))
        .min_h(px(24.))
        .flex()
        .items_center()
        .px_1()
        .py_0p5()
        .hover(|s| s.bg(theme.sidebar_raised))
        .role(ducktape_view_guest::Role::Button)
        .focusable()
        .on_click(toggle)
        .child(if open { "✕ Close" } else { "+ New channel" })
}

fn section_header(
    id: impl Into<ElementId>,
    label: &str,
    control: impl IntoElement,
    theme: &Theme,
) -> impl IntoElement {
    div()
        .id(id)
        .flex()
        .items_center()
        .gap_1()
        .px_1()
        .py_1()
        .text_size(design::text::CAPTION)
        .text_color(theme.sidebar_muted)
        .child(div().flex_1().child(label.to_owned()))
        .child(control)
}

fn quiet(id: impl Into<ElementId>, text: impl Into<String>, theme: &Theme) -> impl IntoElement {
    div()
        .id(id)
        .px_1()
        .py_1()
        .text_size(design::text::CAPTION)
        .text_color(theme.sidebar_muted)
        .child(text.into())
}

/// A click on a row opens its room.
fn opens(
    id: &str,
    cx: &mut Context<Rooms>,
) -> impl Fn(&ClickEvent, &mut Window, &mut App) + 'static {
    let id = id.to_owned();
    cx.listener(move |rooms, _: &ClickEvent, _, cx| rooms.choose(id.clone(), cx))
}

fn channel_button(
    row: &RoomRow,
    active: bool,
    cx: &mut Context<Rooms>,
    theme: &Theme,
) -> AnyElement {
    let selected = row.selected;
    let unread = row.unread;
    let mut el = div()
        .id(format!("chat-sidebar-channel-{}", row.id))
        .flex()
        .w_full()
        .items_center()
        .gap_1()
        .min_h(design::size::CONTROL)
        .px_1()
        .bg(if selected {
            theme.sidebar_raised
        } else {
            theme.sidebar
        })
        .hover(|s| s.bg(theme.sidebar_raised))
        .when(active && !selected, |el| el.bg(theme.sidebar_raised))
        .aria_selected(selected)
        .on_click(opens(&row.id, cx))
        .child(div().text_color(theme.sidebar_muted).child("#"))
        .child(
            div()
                .flex_1()
                .truncate()
                .text_color(if unread {
                    theme.sidebar_foreground
                } else {
                    theme.sidebar_muted
                })
                .child(row.name.clone()),
        );
    if row.members_only {
        el = el.child(
            div()
                .text_size(design::text::CAPTION)
                .text_color(theme.sidebar_muted)
                .child("Members only"),
        );
    }
    if row.archived {
        el = el.child(
            div()
                .text_size(design::text::CAPTION)
                .text_color(theme.sidebar_muted)
                .child("Archived"),
        );
    }
    if unread {
        el = el.child(
            div()
                .id(format!("chat-sidebar-channel-{}-unread", row.id))
                .size_2()
                .rounded_full()
                .bg(theme.accent),
        );
    }
    design::item(el, Role::ListBoxOption, active).into_any_element()
}

fn dm_button(
    row: &RoomRow,
    active: bool,
    cx: &mut Context<Rooms>,
    theme: &Theme,
) -> impl IntoElement {
    let (peer, agent) = row.peer.expect("a direct room has its peer");
    let (name, selected, unread) = (&row.name, row.selected, row.unread);
    let mut el = div()
        .id(format!("chat-sidebar-dm-{peer}"))
        .flex()
        .items_center()
        .gap_1()
        .min_h(design::size::CONTROL)
        .px_1()
        .bg(if selected {
            theme.sidebar_raised
        } else {
            theme.sidebar
        })
        .hover(|s| s.bg(theme.sidebar_raised))
        .when(active && !selected, |el| el.bg(theme.sidebar_raised))
        // the peer's name, not the avatar's initial drawn before it
        .aria_label(name.clone())
        .when(agent, |el| el.aria_description("Agent"))
        .aria_selected(selected)
        .on_click(opens(&row.id, cx))
        .child(avatar(
            format!("chat-sidebar-dm-{peer}-avatar"),
            name,
            agent,
            theme.sidebar_raised,
            theme,
        ))
        .child(
            div()
                .flex_1()
                .truncate()
                .text_color(if unread {
                    theme.sidebar_foreground
                } else {
                    theme.sidebar_muted
                })
                .child(name.clone()),
        );
    if agent {
        el = el.child(super::badge(
            format!("chat-sidebar-dm-{peer}-agent"),
            "Agent",
            theme.agent,
            theme.agent_soft,
        ));
    }
    if unread {
        el = el.child(
            div()
                .id(format!("chat-sidebar-dm-{peer}-unread"))
                .size_2()
                .rounded_full()
                .bg(theme.accent),
        );
    }
    design::item(el, Role::ListBoxOption, active)
}

/// A person's round initials: a direct room's face, in the sidebar and
/// over the room.
pub fn avatar(
    id: impl Into<ElementId>,
    name: &str,
    agent: bool,
    fill: ducktape_view_guest::Hsla,
    theme: &Theme,
) -> impl IntoElement {
    design::avatar(name, design::size::AVATAR_LG, theme)
        .id(id)
        .bg(if agent { theme.agent_soft } else { fill })
        .text_color(theme.foreground)
        .text_size(design::text::CAPTION)
}

pub fn dm_peer(chat: &Chat) -> Option<(String, bool)> {
    let room = chat.room.as_ref()?;
    let names = chat.names.ready()?;
    let peer = dm_peer_of(chat.my_account()?, &room.id)?;
    Some((
        names.member(&Principal::Account(peer)),
        crate::message::agent(names, &Principal::Account(peer)),
    ))
}
