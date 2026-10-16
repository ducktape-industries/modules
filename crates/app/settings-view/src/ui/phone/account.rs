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
use view::design::{Kind as ButtonKind, Ladder, Nav, Orientation};
use view::prelude::*;
use view::{Div, FontWeight, Stateful};

use super::agents;
use super::parts::{
    column, labelled, mono, muted_icon, name_input, name_line, named, note, problem_of, quiet,
    title_and_caption,
};
use crate::Settings;
use crate::queries::{Account, Key, Seat};
use crate::state::Section;
use crate::words;

/// The page element's id; `design::page` names its title row after it.
const PAGE: &str = "settings/page";
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
    // the view's first page: no Back of its own, the shell's capsule closes it
    let page = design::page(PAGE, "Account", Nav::Root, ladder, theme);
    // SDK gap: the network caption, swap when the SDK builds it (kit.md §2)
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

/// The panes under the title row (the Expo kit's `Strip`,
/// `src/kit/Choice.tsx:114`): one Tab stop, ← → open the next.
fn strip(
    view: &Settings,
    shown: Section,
    cx: &mut Context<Settings>,
    ladder: &Ladder,
    theme: &Theme,
) -> Stateful<Div> {
    let sections = view.sections();
    let at = sections
        .iter()
        .position(|section| *section == shown)
        .unwrap_or_default();
    let picked = sections.clone();
    let tabs = sections.into_iter().map(|section| {
        let on = section == shown;
        let pick = cx
            .listener(move |v: &mut Settings, _: &ClickEvent, _, cx| v.select_section(section, cx));
        let id = format!("settings/strip/{}", section.slug());
        design::item(
            design::tab(id, section.label(), on, ladder, theme, pick),
            Role::Tab,
            on,
        )
    });
    design::composite("settings/strip", Role::TabList, "Account")
        .orientation(Orientation::Horizontal)
        .active(at, picked.len())
        .wrap()
        .on_move(cx.processor(move |v: &mut Settings, index: usize, _, cx| {
            v.select_section(picked[index], cx)
        }))
        .build()
        .flex_none()
        .flex()
        .h(ladder.strip)
        .px(design::space::SM)
        .border_b_1()
        .border_color(theme.border)
        .children(tabs)
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
            // SDK gap: the refused read as a full-width Notice, swap when feat/phone-kit lands
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
        // SDK gap: the centred empty state with its icon, swap when feat/phone-kit lands
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
    design::section_head("settings/keys", words::KEYS, ladder, theme).child(count.to_string())
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
        .min_h(ladder.row_tall)
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
    )
    // Return creates it (Expo `app/account/index.tsx:189`)
    .on_submit(cx.listener(|v: &mut Settings, _: &(), _, cx| v.submit_create_account(cx)));
    let pressed =
        cx.listener(|v: &mut Settings, _: &ClickEvent, _, cx| v.submit_create_account(cx));
    // SDK gap: the busy button, swap when feat/phone-kit lands (its spinner needs a wire and host change)
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
