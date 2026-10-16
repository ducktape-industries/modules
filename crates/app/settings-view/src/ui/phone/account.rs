//! Expo `app/account/index.tsx`: the Account screen. Its title row with the
//! network under it, the strip of panes (Account; Agents, for a person who
//! manages some; Invites), and the Account pane: who the seated key is and
//! its keys, or what stands in for them.
//!
//! Reads the seat (`view.account`), the session (the network's name, this
//! device's key), the panes there are and the one shown, and the
//! create-account form. Writes the open pane (`select_section`), a new
//! account (`submit_create_account`) and a read again after a refusal
//! (`read_account`). "Add a device" is the phone shell's row, not this
//! view's (SPEC §7.4).
use sha2::{Digest, Sha256};
use view::design::{Kind as ButtonKind, Ladder, Orientation};
use view::prelude::*;
use view::{Div, FontWeight, Stateful};

use super::agents;
use super::parts::{
    ROW_TALL, column, labelled, mono, muted_icon, name_input, name_line, named, note, problem_of,
    quiet, title_and_caption,
};
use crate::Settings;
use crate::queries::{Account, Key, Seat};
use crate::state::Section;
use crate::words;

/// The page element's id; `design::page` names its title row after it.
const PAGE: &str = "settings/page";
// SDK gap: the strip, swap when feat/phone-kit lands
/// The strip of panes, its hairline included (Expo `src/design/theme.ts:81`).
const STRIP: Pixels = px(44.);
/// The strip's hairline (Expo `src/design/theme.ts:83`).
const HAIRLINE: Pixels = px(1.);
/// The line under the open pane's name (Expo `src/kit/Choice.tsx:138`).
const UNDERLINE: Pixels = px(2.);
/// This device's key, where no account names it (Expo
/// `app/account/index.tsx:97`).
const THIS_DEVICE: &str = "Phone";
/// A phone runs no node, and only the machine running one mints its
/// invites: the Invites pane is this line (the Expo app's, from the app's
/// `invite_unsupported` refusal).
const NO_INVITES: &str = "Only the machine running this node can mint its invites.";

/// The screen: the title row, the strip, the open pane under it.
pub(super) fn screen(
    view: &Settings,
    cx: &mut Context<Settings>,
    ladder: &Ladder,
    theme: &Theme,
) -> impl IntoElement {
    let section = view.shown();
    let pane = match section {
        Section::Account => account_pane(view, cx, ladder, theme),
        Section::Agents => agents::pane(view, cx, ladder, theme),
        Section::Invites => {
            quiet("settings/invite/unsupported", NO_INVITES, ladder, theme).into_any_element()
        }
    };
    // SDK gap: a root page, swap when feat/phone-kit lands
    // the view's first page has no Back of its own: the shell's capsule closes it
    let back: Option<fn(&ClickEvent, &mut Window, &mut App)> = None;
    let page = design::page(PAGE, "Account", back, ladder, theme);
    let page = match words::network(&view.session) {
        Some(network) => page.caption(network.to_owned()),
        None => page,
    };
    page.child(strip(view, section, cx, ladder, theme)).child(
        div()
            .id("settings/scroll")
            .flex_1()
            .min_h_0()
            .overflow_y_scroll()
            .pb(design::space::BLOCK)
            .child(pane),
    )
}

// SDK gap: the strip, swap when feat/phone-kit lands
/// The panes under the title row (the Expo kit's `Strip`,
/// `src/kit/Choice.tsx:114`): one Tab stop, ← → open the next.
fn strip(
    view: &Settings,
    shown: Section,
    cx: &mut Context<Settings>,
    ladder: &Ladder,
    theme: &Theme,
) -> Div {
    let sections = view.sections();
    let at = sections
        .iter()
        .position(|section| *section == shown)
        .unwrap_or_default();
    let picked = sections.clone();
    let tabs = design::composite("settings/strip", Role::TabList, "Account")
        .orientation(Orientation::Horizontal)
        .active(at, sections.len())
        .wrap()
        .on_move(cx.processor(move |v: &mut Settings, index: usize, _, cx| {
            v.select_section(picked[index], cx)
        }))
        .build()
        .flex()
        .px(design::space::SM)
        .children(
            sections
                .into_iter()
                .map(|section| strip_item(section, shown, cx, ladder, theme)),
        );
    div()
        .flex_none()
        .h(STRIP)
        .border_b_1()
        .border_color(theme.border)
        .child(tabs)
}

// SDK gap: the strip, swap when feat/phone-kit lands
/// One pane's name in the strip: muted, or, while it is the one shown,
/// medium over the underline. It draws as tall as the strip above its
/// hairline.
fn strip_item(
    section: Section,
    shown: Section,
    cx: &mut Context<Settings>,
    ladder: &Ladder,
    theme: &Theme,
) -> Div {
    let on = section == shown;
    let pick =
        cx.listener(move |v: &mut Settings, _: &ClickEvent, _, cx| v.select_section(section, cx));
    // the item is already wider than a finger (its name and 10 each side),
    // so its press reaches out only in height, to the 44 a finger needs
    let press = design::hit(
        format!("settings/strip/{}", section.slug()),
        ladder.hit,
        STRIP - HAIRLINE,
        ladder,
    )
    .aria_label(section.label())
    .aria_selected(on)
    .on_click(pick);
    let underline = div()
        .absolute()
        .left_0()
        .right_0()
        .bottom_0()
        .h(UNDERLINE)
        .bg(theme.foreground);
    let label = div()
        .relative()
        .h_full()
        .flex()
        .items_center()
        .text_type(ladder.small)
        .text_color(if on { theme.foreground } else { theme.muted })
        .when(on, |label| label.font_weight(FontWeight::MEDIUM))
        .child(section.label())
        .when(on, |label| label.child(underline));
    div()
        .relative()
        .h(STRIP - HAIRLINE)
        .px(design::space::MD)
        .child(label)
        .child(design::item(press, Role::Tab, on))
}

/// The Account pane: who the seated key is and its keys, or what stands in
/// for them while they are read, refused or missing.
fn account_pane(
    view: &Settings,
    cx: &mut Context<Settings>,
    ladder: &Ladder,
    theme: &Theme,
) -> AnyElement {
    match &view.account {
        Loadable::Ready(Some(seat)) | Loadable::Reloading(Some(seat), _) => {
            seat_pane(view, seat, cx, ladder, theme).into_any_element()
        }
        Loadable::Ready(None) | Loadable::Reloading(None, _) => {
            no_account(view, ladder, theme).into_any_element()
        }
        Loadable::Failed(refusal) => {
            let retry = cx.listener(|v: &mut Settings, _: &ClickEvent, _, cx| v.read_account(cx));
            design::refused(
                "settings/account",
                refusal.message.clone(),
                ladder,
                theme,
                retry,
            )
            .into_any_element()
        }
        Loadable::Idle | Loadable::Loading(_) => {
            quiet("settings/account/loading", words::READING, ladder, theme).into_any_element()
        }
    }
}

/// A seat: an account and its keys; a bare key and the form that gives it
/// an account; a stopped agent's key and why it does not act.
fn seat_pane(
    view: &Settings,
    seat: &Seat,
    cx: &mut Context<Settings>,
    ladder: &Ladder,
    theme: &Theme,
) -> Stateful<Div> {
    // a signer that is not hex is no key: no row is this device's
    let mine = abi::unhex(&view.session.signer).unwrap_or_default();
    let pane = column("settings/account/data");
    match seat {
        Seat::Account(account) => pane
            .child(who(&account.name, about(account), ladder, theme))
            .child(keys_head(account.keys.len(), ladder, theme))
            .children(account.keys.iter().enumerate().map(|(i, key)| {
                let label = words::key_label(key.label.as_deref(), i);
                key_row(i, &label, key, &mine, ladder, theme)
            })),
        Seat::Bare(key) => pane
            .child(who(
                words::UNREGISTERED,
                words::NO_ACCOUNT_YET.into(),
                ladder,
                theme,
            ))
            .child(keys_head(1, ladder, theme))
            .child(key_row(0, THIS_DEVICE, key, &mine, ladder, theme))
            .child(create_account(view, cx, ladder, theme)),
        Seat::Stopped { key, name, note } => pane
            .child(who(name, words::AGENT_KEY.into(), ladder, theme))
            .child(keys_head(1, ladder, theme))
            .child(key_row(0, THIS_DEVICE, key, &mine, ladder, theme))
            .child(quiet("settings/account/note", note.clone(), ladder, theme)),
    }
}

/// No seat: the session names an account identity does not hold, or no
/// key is seated at all.
fn no_account(view: &Settings, ladder: &Ladder, theme: &Theme) -> Stateful<Div> {
    match view.session.account {
        Some(number) if !view.session.signer.is_empty() => quiet(
            "settings/account/none",
            format!("Identity holds no account {number}."),
            ladder,
            theme,
        ),
        _ => design::empty_state(
            "settings/account/empty",
            words::NO_ACCOUNT,
            words::NO_KEY,
            theme,
        ),
    }
}

/// The line under an account's name: `account 7 · Person · 1 key`.
fn about(account: &Account) -> String {
    format!(
        "{} · {} · {}",
        words::account(account.number),
        words::what(&account.kind),
        words::keys_count(account.keys.len())
    )
}

// SDK gap: `format::fingerprint`, swap when feat/phone-kit lands
/// A key's fingerprint, to compare by eye with the device that shows it:
/// the first four bytes of its SHA-256, `4354 C7BD` (the app's
/// `noded-client` `join::fingerprint`).
fn fingerprint(key: &[u8]) -> String {
    let hex: String = Sha256::digest(key)[..4]
        .iter()
        .map(|byte| format!("{byte:02X}"))
        .collect();
    format!("{} {}", &hex[..4], &hex[4..])
}

/// Who the seated key is: its face, its name, one mono line under it.
fn who(name: &str, about: String, ladder: &Ladder, theme: &Theme) -> Stateful<Div> {
    div()
        .id("settings/who")
        .p(design::space::BLOCK)
        .child(named(
            design::avatar(name, ladder.avatar_detail, theme),
            name_line(name, ladder.big_title).id("settings/who/name"),
            about,
            ladder,
            theme,
        ))
}

fn keys_head(count: usize, ladder: &Ladder, theme: &Theme) -> Stateful<Div> {
    design::section_head("settings/keys", words::KEYS, Some(count), ladder, theme)
}

/// One key: a phone for this device's, a key for the rest; its name over
/// its short hex (and this device's fingerprint); Validator at the right.
fn key_row(
    i: usize,
    label: &str,
    key: &Key,
    mine: &[u8],
    ladder: &Ladder,
    theme: &Theme,
) -> Stateful<Div> {
    let this = key.key == mine;
    let mut caption = design::short_hex(&abi::hex(&key.key));
    if this {
        caption.push_str(&format!(" · fingerprint {}", fingerprint(&key.key)));
    }
    let glyph = if this { "smartphone" } else { "key-round" };
    design::row(format!("settings/key/{i}"), ladder, theme)
        .min_h(ROW_TALL)
        .child(design::lead(muted_icon(glyph, ladder, theme)))
        .child(title_and_caption(
            label,
            theme.foreground,
            mono(caption, ladder, theme),
            ladder,
        ))
        .when(key.validator, |row| {
            row.child(design::badge(
                format!("settings/key/{i}/validator"),
                words::VALIDATOR,
                ladder,
                theme.success,
                theme.success_soft,
            ))
        })
}

/// A bare key's account: what it is for, its name, then Create.
fn create_account(
    view: &Settings,
    cx: &mut Context<Settings>,
    ladder: &Ladder,
    theme: &Theme,
) -> Stateful<Div> {
    let form = &view.create_account;
    let problem = problem_of(form, words::CREATE_ACCOUNT.empty);
    let name = name_input(
        "settings/account/create/name",
        form,
        words::ACCOUNT_NAME,
        words::CREATE_ACCOUNT.prompt,
        problem.as_deref(),
        cx.listener(|v: &mut Settings, _: &(), _, cx| v.submit_create_account(cx)),
    );
    let pressed =
        cx.listener(|v: &mut Settings, _: &ClickEvent, _, cx| v.submit_create_account(cx));
    let create = design::button(
        "settings/account/create/submit",
        if form.busy {
            words::CREATE_ACCOUNT.busy
        } else {
            words::CREATE_ACCOUNT.verb
        },
        ladder,
        theme,
        pressed,
    )
    .kind(ButtonKind::Primary)
    .enabled(!form.busy);
    column("settings/account/create")
        .gap(design::space::SM)
        .p(design::space::BLOCK)
        .child(
            div()
                .text_type(ladder.body)
                .font_weight(FontWeight::MEDIUM)
                .child(words::CREATE_ACCOUNT_TITLE),
        )
        .child(note(words::CREATE_ACCOUNT_WHY, theme.muted, ladder))
        .child(labelled(
            words::ACCOUNT_NAME,
            design::field(name, ladder, theme),
            problem,
            ladder,
            theme,
        ))
        .child(div().flex().child(create))
}

#[cfg(test)]
mod tests {
    #[test]
    fn a_fingerprint_is_the_digests_first_four_bytes() {
        // SHA-256 of nothing is e3b0c442…
        assert_eq!(super::fingerprint(&[]), "E3B0 C442");
    }
}
