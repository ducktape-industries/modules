//! One member: who the account is, the devices it acts with, the agents it
//! manages and what it signed lately. Expo `app/(tabs)/members/[number].tsx`
//! (board 91). Read-only, as the desk's detail is.
//!
//! Reads the roster, the account's valset standing, its recent activity
//! (`Members::activity`), and the session's reader, chain and signing key.
//! Writes the page stack only: Back, and a manager or an agent opened over
//! this page (`Members::open`). Open DM and Explorer leave through the
//! host's links.
use view::design::{self, Ladder, space};
use view::prelude::*;
use view::{Div, FontWeight, Stateful};

use super::parts::{
    ROW_TALL, caption, chevron, face, name_line, quiet, refused, title_and_caption,
};
use crate::activity::Signed;
use crate::{
    Device, Group, Members, Row, explorer_link, name_or_number, standing_badge, valset_badge, words,
};

/// A small button is this tall (`theme.ts:76`, `size.small`).
// SDK gap: the kit's small outline `Button`, swap when feat/phone-kit lands
const SMALL: Pixels = px(36.);

/// The member's page: Back, the name over `account N`, Explorer; then the
/// head, the bio, Open DM and Explorer, Devices, Manages, Recent activity
/// and an agent's note.
pub(super) fn page(
    view: &Members,
    number: u64,
    cx: &mut Context<Members>,
    theme: &Theme,
    ladder: &Ladder,
) -> AnyElement {
    let back = cx.listener(|view, _: &ClickEvent, _, cx| view.back(cx));
    let (Some(rows), Some(row)) = (view.rows.ready(), view.selected_row()) else {
        return missing(view, number, back, cx, theme, ladder);
    };
    let explorer = explorer_link(number);
    let open_explorer = {
        let explorer = explorer.clone();
        move |_: &ClickEvent, _: &mut Window, cx: &mut App| cx.host().open_link(&explorer)
    };
    let mut body = div()
        .id("members-detail")
        .flex_1()
        .min_h_0()
        .overflow_y_scroll()
        .flex()
        .flex_col()
        // the scroller's foot (`[number].tsx:78`)
        .pb(space::BLOCK)
        .child(head(view, row, rows, cx, theme, ladder))
        .children(bio(row, ladder))
        .child(actions(view, row, explorer, theme, ladder))
        .children(devices(view, row, theme, ladder));
    let managed = view.managed(number);
    if !managed.is_empty() {
        body = body.children(manages(&managed, cx, theme, ladder));
    }
    if !row.devices.is_empty() {
        body = body.children(activity(view, theme, ladder));
    }
    if row.group() == Group::Agents {
        let note = words::MANAGED_IN_SETTINGS;
        body = body.child(quiet("members-managed-in-settings", note, theme, ladder));
    }
    design::page(
        "members-member",
        row.name.clone(),
        Some(back),
        ladder,
        theme,
    )
    .caption(words::account(number))
    .trailing(design::icon_button(
        "members-explorer-open",
        design::icon("compass", ladder.icon),
        words::EXPLORER,
        ladder,
        theme,
        open_explorer,
    ))
    .child(body)
    .into_any_element()
}

/// The page while the roster has no such account to show: it is read
/// again, it was refused, or identity holds no account by this number.
fn missing(
    view: &Members,
    number: u64,
    back: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
    cx: &mut Context<Members>,
    theme: &Theme,
    ladder: &Ladder,
) -> AnyElement {
    let body = match &view.rows {
        Loadable::Failed(refusal) => refused(refusal, cx, theme, ladder),
        Loadable::Ready(_) | Loadable::Reloading(..) => design::empty_state(
            "members-no-account",
            format!("No account #{number}"),
            "Identity holds no account by this number.",
            theme,
        )
        .into_any_element(),
        Loadable::Idle | Loadable::Loading(_) => {
            quiet("members-loading", words::READING_ROSTER, theme, ladder)
        }
    };
    design::page(
        "members-member",
        format!("Account {number}"),
        Some(back),
        ladder,
        theme,
    )
    .child(body)
    .into_any_element()
}

/// The head: the face at its detail size beside the name in the big
/// title (`you` after mine), one line in the data face of what the account
/// is, and its badges.
fn head(
    view: &Members,
    row: &Row,
    rows: &[Row],
    cx: &mut Context<Members>,
    theme: &Theme,
    ladder: &Ladder,
) -> Div {
    let me = view.me == Some(row.number);
    let title = name_line(row, me, ladder.big_title, theme, ladder)
        .font_weight(FontWeight::MEDIUM)
        .id("members-detail-title")
        .role(Role::Heading)
        .aria_level(2);
    div()
        .flex()
        .gap(space::LG)
        .p(space::BLOCK)
        .pb(space::LG)
        .child(face(row, ladder.avatar_detail, theme))
        .child(
            div()
                .flex_1()
                .min_w_0()
                .flex()
                .flex_col()
                .gap(space::XXS)
                .child(title)
                .child(what(row, rows, cx, theme, ladder))
                .child(badges(row, theme, ladder)),
        )
}

/// What the account is, in the data face: `account 7 · Person · 1 device`,
/// `account 8 · Program`, `account 8 · Program · chat`, or `account 9 ·
/// Agent · managed by <manager> · 1 device` with the manager a link that
/// opens the manager over this page.
fn what(
    row: &Row,
    rows: &[Row],
    cx: &mut Context<Members>,
    theme: &Theme,
    ladder: &Ladder,
) -> Stateful<Div> {
    let account = words::account(row.number);
    let devices = words::devices(row.devices.len());
    let line = div()
        .id("members-caption")
        .flex()
        .flex_wrap()
        .items_baseline()
        .font_family(design::fonts::FAMILY_MONO)
        .text_type(ladder.caption)
        .text_color(theme.muted);
    match &row.kind {
        identity::Kind::User => line.child(format!("{account} · Person · {devices}")),
        identity::Kind::System(program) if *program == row.name => {
            line.child(format!("{account} · {}", words::PROGRAM))
        }
        identity::Kind::System(program) => {
            line.child(format!("{account} · {} · {program}", words::PROGRAM))
        }
        identity::Kind::Program { manager, .. } => {
            let manager = *manager;
            let who = name_or_number(rows, manager);
            let open = cx.listener(move |view, _: &ClickEvent, _, cx| view.open(manager, cx));
            line.child(format!("{account} · Agent · managed by "))
                .child(inline_link("members-manager", who, open, theme, ladder))
                .child(format!(" · {devices}"))
        }
    }
}

/// An agent's standing, and the valset standing of a key the account
/// holds (`Validator`, `Resident`), wrapping.
fn badges(row: &Row, theme: &Theme, ladder: &Ladder) -> Div {
    let standing = match row.kind {
        identity::Kind::Program { standing, .. } => Some(standing_badge(
            "members-standing",
            row,
            standing,
            ladder,
            theme,
        )),
        _ => None,
    };
    div()
        .flex()
        .flex_wrap()
        .gap(space::XS)
        .children(standing)
        .children(
            row.standing
                .as_deref()
                .map(|standing| valset_badge(standing, ladder, theme)),
        )
}

/// The bio, when the account wrote one: secondary, under the head.
fn bio(row: &Row, ladder: &Ladder) -> Option<Stateful<Div>> {
    let bio = row.bio.clone().filter(|bio| !bio.trim().is_empty())?;
    Some(
        div()
            .id("members-bio")
            .px(space::BLOCK)
            .pb(space::LG)
            .text_type(ladder.secondary)
            .child(bio),
    )
}

/// Open DM (a person or an agent, not the reader) and Explorer, as the
/// kit's small outline buttons.
fn actions(view: &Members, row: &Row, explorer: String, theme: &Theme, ladder: &Ladder) -> Div {
    let dm = view.dm_link(row);
    div()
        .flex()
        .gap(space::SM)
        .px(space::BLOCK)
        .pb(space::LG)
        .children(
            dm.map(|link| small_button("members-open-dm", words::OPEN_DM, link, theme, ladder)),
        )
        .child(small_button(
            "members-explorer",
            words::EXPLORER,
            explorer,
            theme,
            ladder,
        ))
}

/// Devices: the section head and a row per key the account acts with.
fn devices(view: &Members, row: &Row, theme: &Theme, ladder: &Ladder) -> Vec<AnyElement> {
    let count = row.devices.len();
    let head = design::section_head(
        "members-devices",
        words::DEVICES,
        Some(count),
        ladder,
        theme,
    );
    let mut section = vec![head.into_any_element()];
    if row.devices.is_empty() {
        section.push(quiet(
            "members-no-devices",
            words::NO_DEVICES,
            theme,
            ladder,
        ));
    }
    let rows = row.devices.iter().enumerate();
    section.extend(rows.map(|(index, device)| device_row(view, index, device, theme, ladder)));
    section
}

/// One key: a phone when it is the key this device signs with, else a
/// key; its label (`Device N` when it has none), its short hex, the day it
/// joined.
fn device_row(
    view: &Members,
    index: usize,
    device: &Device,
    theme: &Theme,
    ladder: &Ladder,
) -> AnyElement {
    let hex = device.hex();
    let glyph = match hex == view.signer {
        true => "smartphone",
        false => "key-round",
    };
    let label = device.title(index);
    let added = div()
        .flex_none()
        .font_family(design::fonts::FAMILY_MONO)
        .text_type(ladder.caption)
        .text_color(theme.faint)
        .child(words::added(device.added_at));
    design::row(format!("members-device-{index}"), ladder, theme)
        .min_h(ROW_TALL)
        .child(design::lead(
            design::icon(glyph, ladder.icon_row).text_color(theme.muted),
        ))
        .child(title_and_caption(
            div().truncate().child(label),
            Some(
                caption(design::short_hex(&hex), theme, ladder)
                    .font_family(design::fonts::FAMILY_MONO),
            ),
        ))
        .child(added)
        .into_any_element()
}

/// Manages: the agents the account manages; a press opens one over this
/// page.
fn manages(
    managed: &[&Row],
    cx: &mut Context<Members>,
    theme: &Theme,
    ladder: &Ladder,
) -> Vec<AnyElement> {
    let head = design::section_head(
        "members-manages",
        words::MANAGES,
        Some(managed.len()),
        ladder,
        theme,
    );
    let rows = managed
        .iter()
        .filter_map(|agent| managed_row(agent, cx, theme, ladder));
    std::iter::once(head.into_any_element())
        .chain(rows)
        .collect()
}

/// One agent: its face, its name over `account N · Agent`, its standing,
/// the chevron.
fn managed_row(
    agent: &Row,
    cx: &mut Context<Members>,
    theme: &Theme,
    ladder: &Ladder,
) -> Option<AnyElement> {
    let identity::Kind::Program { standing, .. } = agent.kind else {
        return None;
    };
    let number = agent.number;
    let about = words::agent(number);
    let life = agent.life();
    let row = design::row(format!("members-manages-{number}"), ladder, theme)
        .min_h(ROW_TALL)
        .role(Role::Button)
        // the agent's name, not the avatar's initial drawn before it
        .aria_label(agent.name.clone())
        .aria_description(format!("{about} · {life}"))
        .focusable()
        .on_click(cx.listener(move |view, _: &ClickEvent, _, cx| view.open(number, cx)))
        .child(design::lead(face(agent, ladder.avatar, theme)))
        .child(title_and_caption(
            div().truncate().child(agent.name.clone()),
            Some(caption(about, theme, ladder)),
        ))
        .child(standing_badge(
            "members-manages-standing",
            agent,
            standing,
            ladder,
            theme,
        ))
        .child(chevron(theme, ladder));
    Some(row.into_any_element())
}

/// Recent activity: what the account's keys signed lately, newest first,
/// or the quiet line for its read: on its way, refused (a network with no
/// activity program says so; there is nothing to retry), or nothing yet.
fn activity(view: &Members, theme: &Theme, ladder: &Ladder) -> Vec<AnyElement> {
    let head = design::section_head(
        "members-activity",
        words::RECENT_ACTIVITY,
        None,
        ladder,
        theme,
    );
    let body = match &view.activity {
        Loadable::Idle | Loadable::Loading(_) => vec![quiet(
            "members-activity-loading",
            words::READING_ACTIVITY,
            theme,
            ladder,
        )],
        Loadable::Failed(refusal) => vec![quiet(
            "members-activity-refused",
            refusal.message.clone(),
            theme,
            ladder,
        )],
        Loadable::Ready(recent) | Loadable::Reloading(recent, _) if recent.items.is_empty() => {
            vec![quiet(
                "members-no-activity",
                words::NO_ACTIVITY,
                theme,
                ladder,
            )]
        }
        Loadable::Ready(recent) | Loadable::Reloading(recent, _) => recent
            .items
            .iter()
            .enumerate()
            .map(|(index, signed)| signed_row(index, signed, recent.now, theme, ladder))
            .collect(),
    };
    std::iter::once(head.into_any_element())
        .chain(body)
        .collect()
}

/// One frame the account signed: its program, then its block (a link to
/// it in Explorer) and how long ago, in the data face.
fn signed_row(
    index: usize,
    signed: &Signed,
    now: u64,
    theme: &Theme,
    ladder: &Ladder,
) -> AnyElement {
    let place = design::grouped(signed.height);
    let block = format!("duck://explorer/block/{}", signed.height);
    let open = move |_: &ClickEvent, _: &mut Window, cx: &mut App| cx.host().open_link(&block);
    let when = div()
        .flex()
        .items_baseline()
        .font_family(design::fonts::FAMILY_MONO)
        .text_type(ladder.caption)
        .text_color(theme.muted)
        .child(inline_link(
            format!("members-block-{index}"),
            format!("block {place}"),
            open,
            theme,
            ladder,
        ))
        .child(format!(" · {}", design::ago(now, signed.time)));
    design::row(format!("members-signed-{index}"), ladder, theme)
        .min_h(ROW_TALL)
        .child(title_and_caption(
            div().truncate().child(signed.target.clone()),
            Some(when),
        ))
        .into_any_element()
}

/// The kit's small outline button (36 tall, 12 in, secondary medium),
/// opening `link` through the host; its press reaches 44 up and down.
// SDK gap: the kit's small outline `Button`, swap when feat/phone-kit lands
fn small_button(
    id: &'static str,
    label: &'static str,
    link: String,
    theme: &Theme,
    ladder: &Ladder,
) -> Div {
    let open = move |_: &ClickEvent, _: &mut Window, cx: &mut App| cx.host().open_link(&link);
    div()
        .relative()
        .flex_none()
        .h(SMALL)
        .flex()
        .items_center()
        .justify_center()
        .px(space::LG)
        .border_1()
        .border_color(theme.border_strong)
        .text_type(ladder.secondary)
        .font_weight(FontWeight::MEDIUM)
        .whitespace_nowrap()
        .child(label)
        .child(
            design::hit(id, ladder.hit, SMALL, ladder)
                .role(Role::Button)
                .aria_label(label)
                .focusable()
                .on_click(open),
        )
}

/// A link inside a line of text (the explorer kit's `Linked`): small, in
/// the data face, underlined, in the foreground. Its press reaches 44 up
/// and down and half of 44 past each side, as wide as the word is unknown
/// here.
// SDK gap: the explorer kit's `Linked`, swap when feat/phone-kit lands
fn inline_link(
    id: impl Into<ElementId>,
    label: impl Into<SharedString>,
    click: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
    theme: &Theme,
    ladder: &Ladder,
) -> Div {
    let label = label.into();
    let line = ladder.small.line.unwrap_or(ladder.small.size);
    div()
        .relative()
        .flex_none()
        .h(line)
        .font_family(design::fonts::FAMILY_MONO)
        .text_type(ladder.small)
        .text_color(theme.foreground)
        .text_decoration_1()
        .whitespace_nowrap()
        .child(label.clone())
        .child(
            design::hit(id, px(0.), line, ladder)
                .role(Role::Link)
                .aria_label(label)
                .focusable()
                .on_click(click),
        )
}
