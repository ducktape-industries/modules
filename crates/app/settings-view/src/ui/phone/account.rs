//! Expo `app/account/index.tsx`: the Account screen. Its title row with the
//! network under it, the strip of panes (Account; Agents, for a person who
//! manages some; Invites), and the Account pane: who the seated key is and
//! its keys, or what stands in for them.
//!
//! Reads the seat (`view.account`), the session (the network's name, this
//! device's key), the open pane and the create-account form. Writes the
//! open pane (`select_section`), a new account (`submit_create_account`)
//! and a read again after a refusal (`read_account`). "Add a device" is
//! the phone shell's row, not this view's (SPEC §7.4).
use super::parts::{TALL, labelled, mono, muted_icon, note, problem_of, quiet, words};
use super::*;
use crate::queries::{Account, Key};
use identity::Kind;
use sha2::{Digest, Sha256};
use view::design::Orientation;

/// The page element's id; `design::page` names its title row after it.
const PAGE: &str = "settings/page";
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
    theme: &Theme,
    ladder: &Ladder,
) -> impl IntoElement {
    let section = shown(view);
    let pane = match section {
        Section::Account => account_pane(view, cx, theme, ladder),
        Section::Agents => agents::pane(view, cx, theme, ladder),
        Section::Invites => {
            quiet("settings/invite/unsupported", NO_INVITES, theme, ladder).into_any_element()
        }
    };
    // the view's first page: no Back of its own, the shell's capsule closes it
    let back: Option<fn(&ClickEvent, &mut Window, &mut App)> = None;
    let page = design::page(PAGE, "Account", back, ladder, theme);
    let page = match network(view) {
        Some(network) => page.caption(network.to_owned()),
        None => page,
    };
    page.child(strip(view, section, cx, theme, ladder)).child(
        div()
            .id("settings/scroll")
            .flex_1()
            .min_h_0()
            .overflow_y_scroll()
            .pb(design::space::BLOCK)
            .child(pane),
    )
}

/// The network's name: the session's `<label>#<salt>` up to the `#`.
fn network(view: &Settings) -> Option<&str> {
    view.session
        .chain_id
        .split('#')
        .next()
        .filter(|name| !name.is_empty())
}

/// The panes under the title row (the Expo kit's `Strip`,
/// `src/kit/Choice.tsx:114`): one Tab stop, ← → open the next.
fn strip(
    view: &Settings,
    shown: Section,
    cx: &mut Context<Settings>,
    theme: &Theme,
    ladder: &Ladder,
) -> Div {
    let sections = sections(view);
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
                .map(|section| strip_item(section, shown, cx, theme, ladder)),
        );
    div()
        .flex_none()
        .h(STRIP)
        .border_b_1()
        .border_color(theme.border)
        .child(tabs)
}

/// One pane's name in the strip: muted, or, while it is the one shown,
/// medium over the underline. It draws as tall as the strip above its
/// hairline; its press reaches the height a finger needs.
fn strip_item(
    section: Section,
    shown: Section,
    cx: &mut Context<Settings>,
    theme: &Theme,
    ladder: &Ladder,
) -> Div {
    let on = section == shown;
    let pick =
        cx.listener(move |v: &mut Settings, _: &ClickEvent, _, cx| v.select_section(section, cx));
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
    theme: &Theme,
    ladder: &Ladder,
) -> AnyElement {
    match &view.account {
        Loadable::Ready(Some(seat)) | Loadable::Reloading(Some(seat), _) => {
            seat_pane(view, seat, cx, theme, ladder).into_any_element()
        }
        Loadable::Ready(None) | Loadable::Reloading(None, _) => {
            no_account(view, theme, ladder).into_any_element()
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
        Loadable::Idle | Loadable::Loading(_) => quiet(
            "settings/account/loading",
            "Reading your account…",
            theme,
            ladder,
        )
        .into_any_element(),
    }
}

/// A seat: an account and its keys; a bare key and the form that gives it
/// an account; a stopped agent's key and why it does not act.
fn seat_pane(
    view: &Settings,
    seat: &Seat,
    cx: &mut Context<Settings>,
    theme: &Theme,
    ladder: &Ladder,
) -> Stateful<Div> {
    let mine = abi::unhex(&view.session.signer).unwrap_or_default();
    let pane = column("settings/account/data").gap_0();
    match seat {
        Seat::Account(account) => pane
            .child(who(&account.name, about(account), theme, ladder))
            .child(keys_head(account.keys.len(), theme, ladder))
            .children(
                account
                    .keys
                    .iter()
                    .enumerate()
                    .map(|(i, key)| key_row(i, &key_label(key, i), key, &mine, theme, ladder)),
            ),
        Seat::Bare(key) => pane
            .child(who(
                "Unregistered key",
                "no account yet".into(),
                theme,
                ladder,
            ))
            .child(keys_head(1, theme, ladder))
            .child(key_row(0, THIS_DEVICE, key, &mine, theme, ladder))
            .child(create_account(view, cx, theme, ladder)),
        Seat::Stopped { key, name, note } => pane
            .child(who(name, "a key of an agent".into(), theme, ladder))
            .child(keys_head(1, theme, ladder))
            .child(key_row(0, THIS_DEVICE, key, &mine, theme, ladder))
            .child(quiet("settings/account/note", note.clone(), theme, ladder)),
    }
}

/// No seat: the session names an account identity does not hold, or no
/// key is seated at all.
fn no_account(view: &Settings, theme: &Theme, ladder: &Ladder) -> Stateful<Div> {
    match view.session.account {
        Some(number) if !view.session.signer.is_empty() => quiet(
            "settings/account/none",
            format!("Identity holds no account {number}."),
            theme,
            ladder,
        ),
        _ => design::empty_state(
            "settings/account/empty",
            "No account",
            "No host key is selected. Sign in with a key to create an account.",
            theme,
        ),
    }
}

/// The line under an account's name: `account 7 · Person · 1 key`.
fn about(account: &Account) -> String {
    let keys = design::plural(account.keys.len() as u64, "key", "keys");
    format!(
        "account {} · {} · {keys}",
        account.number,
        what(&account.kind)
    )
}

/// What identity says an account is, as the line under its name says it
/// (Expo `src/account/words.ts:33`).
fn what(kind: &Kind) -> String {
    match kind {
        Kind::User => "Person".into(),
        Kind::Program { .. } => "Agent".into(),
        Kind::System(program) => format!("Program · {program}"),
    }
}

/// A key's name: its label, else its place on the account (Expo
/// `src/account/words.ts:40`).
fn key_label(key: &Key, index: usize) -> String {
    match &key.label {
        Some(label) => label.clone(),
        None => format!("Device {}", index + 1),
    }
}

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

/// Who the seated key is: an avatar, the name, one mono line under it.
fn who(name: &str, about: String, theme: &Theme, ladder: &Ladder) -> Stateful<Div> {
    div()
        .id("settings/who")
        .flex()
        .items_center()
        .gap(design::space::LG)
        .p(design::space::BLOCK)
        .child(design::avatar(name, ladder.avatar_detail, theme))
        .child(
            div()
                .flex_1()
                .min_w_0()
                .flex()
                .flex_col()
                .child(
                    div()
                        .id("settings/who/name")
                        .truncate()
                        .text_type(ladder.big_title)
                        .font_weight(FontWeight::MEDIUM)
                        .child(name.to_owned()),
                )
                .child(mono(about, theme, ladder)),
        )
}

fn keys_head(count: usize, theme: &Theme, ladder: &Ladder) -> Stateful<Div> {
    design::section_head("settings/keys", "Keys", Some(count), ladder, theme)
}

/// One key: a phone for this device's, a key for the rest; its name over
/// its short hex (and this device's fingerprint); Validator at the right.
fn key_row(
    i: usize,
    label: &str,
    key: &Key,
    mine: &[u8],
    theme: &Theme,
    ladder: &Ladder,
) -> Stateful<Div> {
    let this = key.key == mine;
    let mut caption = design::short_hex(&abi::hex(&key.key));
    if this {
        caption.push_str(&format!(" · fingerprint {}", fingerprint(&key.key)));
    }
    let glyph = if this { "smartphone" } else { "key-round" };
    design::row(format!("settings/key/{i}"), ladder, theme)
        .min_h(TALL)
        .child(design::lead(muted_icon(glyph, ladder, theme)))
        .child(words(
            label,
            theme.foreground,
            mono(caption, theme, ladder),
            ladder,
        ))
        .when(key.validator, |row| {
            row.child(design::badge(
                format!("settings/key/{i}/validator"),
                "Validator",
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
    theme: &Theme,
    ladder: &Ladder,
) -> Stateful<Div> {
    let form = &view.create_account;
    let pressed =
        cx.listener(|v: &mut Settings, _: &ClickEvent, _, cx| v.submit_create_account(cx));
    let create = design::button(
        "settings/account/create/submit",
        if form.busy {
            "Creating…"
        } else {
            "Create account"
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
                .child("Create an account"),
        )
        .child(note(
            "Your key isn't linked to an account yet. An account gives you a name others see.",
            theme.muted,
            ladder,
        ))
        .child(labelled(
            "Account name",
            design::field(account_name(form, cx), ladder, theme),
            problem_of(form, "Enter an account name."),
            theme,
            ladder,
        ))
        .child(div().flex().child(create))
}

/// The new account's name; Return creates it.
fn account_name(form: &Form, cx: &mut Context<Settings>) -> Input {
    Input::new(
        "settings/account/create/name",
        &form.text,
        "Account name".to_owned(),
    )
    .placeholder("Name the new account")
    .disabled(form.busy)
    .autocapitalize(Autocapitalize::Words)
    .when(!form.busy, |input| {
        input.on_submit(cx.listener(|v: &mut Settings, _: &(), _, cx| v.submit_create_account(cx)))
    })
}

#[cfg(test)]
mod tests {
    #[test]
    fn a_fingerprint_is_the_digests_first_four_bytes() {
        // SHA-256 of nothing is e3b0c442…
        assert_eq!(super::fingerprint(&[]), "E3B0 C442");
    }
}
