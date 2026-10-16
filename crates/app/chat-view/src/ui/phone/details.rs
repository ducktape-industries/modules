//! Channel details (Expo `app/(tabs)/chat/[channel]/details.tsx`, board
//! 53): the channel's name and facts, its members, and for its owner the
//! way to add a member, rename it, archive or unarchive it; adding,
//! renaming and removing a member each a sheet over the page. Reads the
//! open channel, its members and the roster; writes the details' drafts
//! and open sheet, and sends membership, rename and archive ops.
use view::prelude::*;
use view::{Div, Stateful};

use super::parts::{self, ROW_TALL, face, quiet};
use crate::{Chat, Details, DetailsSheet, Room};
use chat::view::Names;
use chat::{ChannelInfo, Principal};
use view::FontWeight;
use view::design::Ladder;

/// How many people the Add sheet offers (mobile
/// `app/(tabs)/chat/[channel]/details.tsx:142`).
const OFFERED: usize = 8;

pub(super) fn screen(
    chat: &Chat,
    room: &Room,
    back: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
    cx: &mut Context<Chat>,
    theme: &Theme,
    ladder: &Ladder,
) -> AnyElement {
    let page = design::page(
        "chat-details-page",
        "Channel details",
        Some(back),
        ladder,
        theme,
    );
    let (Some(info), Some(details)) = (chat.info(&room.id), chat.details.as_ref()) else {
        return page
            .child(quiet(
                "chat-details-loading",
                "Loading the channel…",
                theme,
                ladder,
            ))
            .into_any_element();
    };
    let body = div()
        .id("chat-details-scroll")
        .flex_1()
        .min_h(px(0.))
        .overflow_y_scroll()
        .flex()
        .flex_col()
        .child(head(chat, info, theme, ladder))
        .children(members(chat, room, info, cx, theme, ladder))
        .children(owner_actions(chat, info, cx, theme, ladder));
    page.caption(format!("#{}", info.channel.name))
        .children(parts::room_notice(chat, cx, theme, ladder))
        .child(body)
        .children(sheet(chat, details, cx, theme, ladder))
        .into_any_element()
}

/// The name, then who may post, when it was made and by whom.
fn head(chat: &Chat, info: &ChannelInfo, theme: &Theme, ladder: &Ladder) -> Stateful<Div> {
    let channel = &info.channel;
    let policy = parts::policy(channel);
    let owner = chat.names.ready().map_or_else(
        || Names::empty().member(&channel.owner),
        |names| names.member(&channel.owner),
    );
    div()
        .id("chat-details-head")
        .flex()
        .flex_col()
        .gap(design::space::XXS)
        .p(design::space::BLOCK)
        .child(
            div()
                .text_type(ladder.heading)
                .font_weight(FontWeight::MEDIUM)
                .child(format!("#{}", channel.name)),
        )
        .child(
            div()
                .text_type(ladder.secondary)
                .text_color(theme.muted)
                .child(format!(
                    "{policy} · created {} · owner {owner}",
                    design::day(channel.created_at)
                )),
        )
}

/// Members and how many; a row each (the owner's press opens a member's
/// way out); Add a member… for the owner.
fn members(
    chat: &Chat,
    room: &Room,
    info: &ChannelInfo,
    cx: &mut Context<Chat>,
    theme: &Theme,
    ladder: &Ladder,
) -> Vec<AnyElement> {
    let roster = chat.roster();
    let count = (!roster.is_empty()).then_some(roster.len());
    let mut rows = vec![
        design::section_head("chat-details-members-head", "Members", count, ladder, theme)
            .into_any_element(),
    ];
    if room.members.ready().is_none() {
        rows.push(
            quiet(
                "chat-details-members-loading",
                "Loading members…",
                theme,
                ladder,
            )
            .into_any_element(),
        );
    } else if roster.is_empty() {
        rows.push(
            quiet(
                "chat-details-no-members",
                "No members added. An open channel needs none.",
                theme,
                ladder,
            )
            .into_any_element(),
        );
    }
    let owner = chat.owns_room();
    rows.extend(
        roster
            .into_iter()
            .map(|member| member_row(chat, info, member, owner, cx, theme, ladder)),
    );
    if owner {
        let add = cx.listener(|chat, _: &ClickEvent, _window, cx| {
            open_sheet(chat, DetailsSheet::Add);
            cx.notify();
        });
        rows.push(
            action_row("chat-details-add", "plus", ladder, theme)
                .on_click(add)
                .child(
                    div()
                        .flex_1()
                        .text_type(ladder.body)
                        .text_color(theme.muted)
                        .child("Add a member…"),
                )
                .into_any_element(),
        );
    }
    rows
}

/// One member, by principal and name: face, name, what they are here
/// (owner, you), Agent; the owner's press on anyone else opens their way
/// out.
fn member_row(
    chat: &Chat,
    info: &ChannelInfo,
    (principal, name): (Principal, String),
    owner: bool,
    cx: &mut Context<Chat>,
    theme: &Theme,
    ladder: &Ladder,
) -> AnyElement {
    let me = chat.me();
    let notes = [
        (principal == info.channel.owner).then_some("owner"),
        (Some(&principal) == me.as_ref()).then_some("you"),
    ]
    .into_iter()
    .flatten()
    .collect::<Vec<_>>()
    .join(" · ");
    let agent = chat
        .names
        .ready()
        .is_some_and(|names| crate::message::agent(names, &principal));
    let removable = match &principal {
        Principal::Account(account) if owner && Some(&principal) != me.as_ref() => Some(*account),
        _ => None,
    };
    let id = format!(
        "chat-details-member-{}",
        crate::names::mention_token(&principal)
    );
    design::row(id.clone(), ladder, theme)
        .when(!notes.is_empty(), |row| row.min_h(ROW_TALL))
        .child(design::lead(face(&name, agent, ladder.avatar, theme)))
        .child(
            div()
                .flex_1()
                .min_w(px(0.))
                .flex()
                .flex_col()
                .child(div().truncate().text_type(ladder.body).child(name.clone()))
                .when(!notes.is_empty(), |words| {
                    words.child(
                        div()
                            .truncate()
                            .text_type(ladder.caption)
                            .text_color(theme.muted)
                            .child(notes),
                    )
                }),
        )
        .when(agent, |row| {
            row.child(design::badge(
                format!("{id}-agent"),
                "Agent",
                ladder,
                theme.agent,
                theme.agent_soft,
            ))
        })
        .when_some(removable, |row, account| {
            row.role(Role::Button)
                .aria_label(name)
                .focusable()
                .on_click(cx.listener(move |chat, _: &ClickEvent, _window, cx| {
                    open_sheet(chat, DetailsSheet::Remove(account));
                    cx.notify();
                }))
        })
        .into_any_element()
}

/// The owner's actions: rename, and archive or unarchive.
fn owner_actions(
    chat: &Chat,
    info: &ChannelInfo,
    cx: &mut Context<Chat>,
    theme: &Theme,
    ladder: &Ladder,
) -> Vec<AnyElement> {
    if !chat.owns_room() {
        return Vec::new();
    }
    let rename = cx.listener(|chat, _: &ClickEvent, _window, cx| {
        open_sheet(chat, DetailsSheet::Rename);
        cx.notify();
    });
    let archived = info.channel.archived;
    let archive = cx.listener(move |chat, _: &ClickEvent, _window, cx| {
        cx.notify();
        chat.set_archived(!archived, cx);
    });
    let (icon, title, caption) = match archived {
        true => (
            "archive-restore",
            "Unarchive channel",
            "It takes messages again",
        ),
        false => (
            "archive",
            "Archive channel",
            "Keeps history, takes no new messages",
        ),
    };
    vec![
        design::section_head(
            "chat-details-owner-head",
            "Owner actions",
            None,
            ladder,
            theme,
        )
        .into_any_element(),
        action_row("chat-details-rename", "pencil", ladder, theme)
            .on_click(rename)
            .child(
                div()
                    .flex_1()
                    .text_type(ladder.body)
                    .child("Rename channel"),
            )
            .child(design::icon("chevron-right", ladder.icon_row).text_color(theme.faint))
            .into_any_element(),
        action_row("chat-details-archive", icon, ladder, theme)
            .min_h(ROW_TALL)
            .on_click(archive)
            .child(
                div()
                    .flex_1()
                    .flex()
                    .flex_col()
                    .child(div().text_type(ladder.body).child(title))
                    .child(
                        div()
                            .text_type(ladder.caption)
                            .text_color(theme.muted)
                            .child(caption),
                    ),
            )
            .into_any_element(),
    ]
}

/// A row that does something, its icon muted in the lead column.
fn action_row(
    id: &'static str,
    icon: &'static str,
    ladder: &Ladder,
    theme: &Theme,
) -> Stateful<Div> {
    design::row(id, ladder, theme)
        .role(Role::Button)
        .focusable()
        .child(design::lead(
            design::icon(icon, ladder.icon_row).text_color(theme.muted),
        ))
}

/// Opens a sheet over the page, its field fresh.
fn open_sheet(chat: &mut Chat, sheet: DetailsSheet) {
    let name = chat
        .room_info()
        .map(|info| info.channel.name.clone())
        .unwrap_or_default();
    if let Some(details) = &mut chat.details {
        details.member_draft.reset("");
        details.name_draft.reset(name);
        details.sheet = Some(sheet);
    }
}

/// Closes the open sheet.
fn shut(chat: &mut Chat) {
    if let Some(details) = &mut chat.details {
        details.sheet = None;
    }
}

/// The sheet open over the page, if any.
fn sheet(
    chat: &Chat,
    details: &Details,
    cx: &mut Context<Chat>,
    theme: &Theme,
    ladder: &Ladder,
) -> Option<AnyElement> {
    let which = details.sheet?;
    let empty = Names::empty();
    let names = chat.names.ready().unwrap_or(&empty);
    let (title, body) = match which {
        DetailsSheet::Add => (
            "Add a member".to_owned(),
            add_body(chat, details, names, cx, theme, ladder),
        ),
        DetailsSheet::Rename => (
            "Rename channel".to_owned(),
            rename_body(details, cx, theme, ladder),
        ),
        DetailsSheet::Remove(account) => {
            let remove = cx.listener(move |chat, _: &ClickEvent, _window, cx| {
                shut(chat);
                cx.notify();
                chat.set_member(Principal::Account(account), false, cx);
            });
            let row = design::sheet_row(
                "chat-details-remove",
                Some("user-minus"),
                "Remove from this channel",
                true,
                ladder,
                theme,
                remove,
            );
            (
                names.member(&Principal::Account(account)),
                vec![row.into_any_element()],
            )
        }
    };
    let close = cx.listener(|chat, _: &(), _window, cx| {
        shut(chat);
        cx.notify();
    });
    let head = design::sheet_head("chat-details-sheet-head", theme).child(
        div()
            .text_type(ladder.body)
            .font_weight(FontWeight::MEDIUM)
            .child(title.clone()),
    );
    Some(
        design::sheet("chat-details-sheet", title, ladder, theme)
            .on_dismiss(close)
            .child(div().flex().flex_col().child(head).children(body))
            .into_any_element(),
    )
}

/// Add a member: the account number and Add, then the people not seated.
fn add_body(
    chat: &Chat,
    details: &Details,
    names: &Names,
    cx: &mut Context<Chat>,
    theme: &Theme,
    ladder: &Ladder,
) -> Vec<AnyElement> {
    let add = cx.listener(|chat, _: &ClickEvent, _window, cx| {
        shut(chat);
        cx.notify();
        chat.add_member(cx);
    });
    let input = Input::new(
        "chat-details-member-input",
        &details.member_draft,
        "Account number",
    )
    .virtual_keyboard(VirtualKeyboard::Numeric)
    .autocorrect(false)
    .autocapitalize(Autocapitalize::None);
    // an account number, in the data face at the secondary size
    let input = design::field(input, ladder, theme)
        .font_family(design::fonts::FAMILY_MONO)
        .text_type(ladder.secondary);
    let field = parts::field("Account number", input, None, theme, ladder);
    let form = div()
        .flex()
        .flex_col()
        .gap(design::space::LG)
        .p(design::space::BLOCK)
        .child(field)
        .child(
            design::button("chat-details-add-member", "Add", ladder, theme, add)
                .kind(design::Kind::Primary),
        );
    let seated: Vec<Principal> = chat
        .roster()
        .into_iter()
        .map(|(principal, _)| principal)
        .collect();
    let people = names
        .people()
        .filter(|account| !seated.contains(&Principal::Account(*account)))
        .take(OFFERED)
        .map(|account| {
            let pick = cx.listener(move |chat, _: &ClickEvent, _window, cx| {
                shut(chat);
                cx.notify();
                chat.set_member(Principal::Account(account), true, cx);
            });
            let name = names.member(&Principal::Account(account));
            design::sheet_row(
                format!("chat-details-person-{account}"),
                None,
                name,
                false,
                ladder,
                theme,
                pick,
            )
            .child(
                div()
                    .text_type(ladder.caption)
                    .text_color(theme.muted)
                    .child(format!("account {account}")),
            )
            .into_any_element()
        });
    std::iter::once(form.into_any_element())
        .chain(people)
        .collect()
}

/// Rename the channel: the name and Rename.
fn rename_body(
    details: &Details,
    cx: &mut Context<Chat>,
    theme: &Theme,
    ladder: &Ladder,
) -> Vec<AnyElement> {
    let rename = cx.listener(|chat, _: &ClickEvent, _window, cx| {
        shut(chat);
        cx.notify();
        chat.rename(cx)
    });
    let input = Input::new("chat-details-name-input", &details.name_draft, "Name")
        .autocorrect(false)
        .autocapitalize(Autocapitalize::None);
    let input = design::field(input, ladder, theme);
    vec![
        div()
            .flex()
            .flex_col()
            .gap(design::space::LG)
            .p(design::space::BLOCK)
            .child(parts::field("Name", input, None, theme, ladder))
            .child(
                design::button(
                    "chat-details-rename-button",
                    "Rename",
                    ladder,
                    theme,
                    rename,
                )
                .kind(design::Kind::Primary),
            )
            .into_any_element(),
    ]
}
