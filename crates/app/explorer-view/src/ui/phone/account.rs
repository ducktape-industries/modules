//! Expo `app/(tabs)/explorer/account/[number].tsx`: one account over the
//! root. Its face and what it is, its figures, its devices and when each
//! last signed, the programs it used, what it signed. Reads the accounts,
//! what activity recorded for it (`used`, `acted`); writes the route (a
//! chip, a row).
use super::*;
use crate::decode::{scheme, short};

/// A chip of the programs an account used (Expo `src/design/theme.ts:78`
/// `size.chip`).
const CHIP_H: Pixels = px(32.);
/// A chip's count (Expo `src/kit/Choice.tsx:72`).
const CHIP_COUNT_TEXT: Pixels = px(11.);

pub(super) fn account(view: &Explorer, number: u64, cx: Cx, k: &Kit) -> AnyElement {
    let Some(account) = view.account(number) else {
        return over("explorer-account", format!("Account {number}"), view, cx, k)
            .child(missing(view, number, cx, k))
            .into_any_element();
    };
    let kind = view.kind_of(account);
    let copy = copy_link(view, &Route::Account(number), cx, k);
    let page = over("explorer-account", account.card.name.clone(), view, cx, k)
        .caption(format!("account {number} · {kind}"));
    let page = match view.dm_link(account) {
        Some(link) => page.trailing(open_dm(link, k)),
        None => page,
    };
    let page = match copy {
        Some(copy) => page.trailing(copy),
        None => page,
    };
    page.child(
        scroller("explorer-page")
            .child(head(account, &kind, k))
            .child(figures(view, account, k))
            .children(devices(view, account, k))
            .children(programs_used(view, number, cx, k))
            .children(activity(view, number, cx, k)),
    )
    .into_any_element()
}

/// Open DM (Expo `account/[number].tsx:59,69`): chat opens on the reader's
/// DM room with this account.
fn open_dm(link: String, k: &Kit) -> AnyElement {
    design::icon_button(
        "explorer-open-dm",
        design::icon("message-square", k.ladder.icon),
        "Open DM",
        &k.ladder,
        &k.theme,
        move |_: &ClickEvent, _: &mut Window, cx: &mut App| cx.host().open_link(&link),
    )
    .into_any_element()
}

/// An account not in hand: the accounts being read, refused, or none by
/// this number.
fn missing(view: &Explorer, number: u64, cx: Cx, k: &Kit) -> AnyElement {
    match &view.accounts {
        Loadable::Ready(_) | Loadable::Reloading(_, _) => empty(
            "explorer-no-account",
            "user-x",
            words::no_account(number),
            words::NO_ACCOUNT_HERE,
            k,
        ),
        Loadable::Failed(refusal) => refused(&refusal.message, cx, k),
        Loadable::Idle | Loadable::Loading(_) => {
            quiet("explorer-account-loading", words::READING_ACCOUNT, k)
        }
    }
}

/// Its face at the detail size, its name, `account n · what it is · its
/// devices`.
fn head(account: &identity::Account, kind: &str, k: &Kit) -> Div {
    let about = format!(
        "account {} · {kind} · {}",
        account.number,
        plural(account.keys().len() as u64, "device", "devices")
    );
    div()
        .flex()
        .items_center()
        .gap(space::LG)
        .p(space::BLOCK)
        .child(face(account, k.ladder.avatar_detail, k))
        .child(
            div()
                .flex_1()
                .min_w_0()
                .flex()
                .flex_col()
                .child(
                    div()
                        .id("explorer-title")
                        .truncate()
                        .text_type(k.ladder.heading)
                        .font_weight(FontWeight::MEDIUM)
                        .child(account.card.name.clone()),
                )
                .child(
                    mono(about)
                        .text_type(k.ladder.caption)
                        .text_color(k.theme.muted),
                ),
        )
}

/// The transactions activity recorded for it, and its devices.
fn figures(view: &Explorer, account: &identity::Account, k: &Kit) -> Stateful<Div> {
    let total = view.sent_by(account.number);
    facts(
        "explorer-account-facts",
        vec![
            Fact {
                key: "transactions",
                label: "Transactions",
                value: total.map_or_else(|| words::NO_VALUE.to_owned(), grouped),
                caption: Some("recorded".into()),
            },
            Fact {
                key: "devices",
                label: "Devices",
                value: grouped(account.keys().len() as u64),
                caption: None,
            },
        ],
        k,
    )
}

/// Each key it signs with, under their head: its label, its scheme and
/// short key, when activity last saw it sign.
fn devices(view: &Explorer, account: &identity::Account, k: &Kit) -> Vec<AnyElement> {
    let keys = account.keys();
    let now = view.chain.now();
    let signed = view.acted(account.number).unwrap_or_default();
    let head = section(
        "explorer-devices-heading",
        "Devices",
        Some(aside(grouped(keys.len() as u64))),
        k,
    );
    let rows = keys.iter().enumerate().map(|(index, key)| {
        let last = signed.iter().find(|frame| frame.signer == key.key);
        let label = key.label.clone().unwrap_or_else(|| words::device(index));
        let used = words::last_used(now, last.map(|frame| frame.time));
        list_row(ElementId::named_usize("explorer-device", index), k)
            .child(design::lead(
                design::icon("key-round", k.ladder.icon_row).text_color(k.theme.muted),
            ))
            .child(title_and_caption(
                div().truncate().child(label),
                mono(format!("{} {}", scheme(key.scheme), short(&key.key)))
                    .text_type(k.ladder.caption)
                    .text_color(k.theme.faint),
            ))
            .child(tail(used, k))
            .into_any_element()
    });
    std::iter::once(head.into_any_element())
        .chain(rows)
        .collect()
}

/// The programs it used, most first, a chip each with its count; a chip
/// opens the program's transactions (Expo `Chips`). None used, no section.
fn programs_used(view: &Explorer, number: u64, cx: Cx, k: &Kit) -> Option<Div> {
    let used = view.programs_used(number)?;
    if used.is_empty() {
        return None;
    }
    let chips = used
        .into_iter()
        .map(|(program, count)| chip(program.to_owned(), count, cx, k));
    Some(
        div()
            .child(section("explorer-used-heading", "Programs used", None, k))
            .child(
                div()
                    .id("explorer-used")
                    .flex()
                    .gap(space::SM)
                    .py(space::SM)
                    .px(space::BLOCK)
                    .overflow_x_scroll()
                    .children(chips),
            ),
    )
}

// SDK gap: Chips, swap when feat/phone-kit lands
/// One program it used, and how many transactions it sent there; the
/// press reaches past the chip to `ladder.hit`.
fn chip(program: String, count: u64, cx: Cx, k: &Kit) -> Div {
    let route = Route::Transactions(Some(program.clone()));
    let go =
        cx.listener(move |view: &mut Explorer, _: &ClickEvent, _, cx| view.go(route.clone(), cx));
    let count = words::tx_count(count);
    div()
        .relative()
        .flex_none()
        .h(CHIP_H)
        .flex()
        .items_center()
        .px(space::MD)
        .border_1()
        .border_color(k.theme.border_strong)
        .text_type(k.ladder.secondary)
        .whitespace_nowrap()
        .child(program.clone())
        .child(
            mono(format!("  {count}"))
                .text_size(CHIP_COUNT_TEXT)
                .text_color(k.theme.muted),
        )
        .child(
            design::hit(
                SharedString::from(format!("explorer-used-{program}")),
                k.ladder.hit,
                CHIP_H,
                &k.ladder,
            )
            .role(Role::Button)
            .aria_label(format!("{program}, {count}"))
            .focusable()
            .on_click(go),
        )
}

/// What it signed, newest first, under its head (Expo `TxLine
/// who={false}`).
fn activity(view: &Explorer, number: u64, cx: Cx, k: &Kit) -> Vec<AnyElement> {
    let recorded = view.sent_by(number).map(words::recorded);
    let head = section(
        "explorer-activity-heading",
        "Activity",
        recorded.map(aside),
        k,
    );
    let list = match (view.acted(number), &view.acted) {
        (Some([]), _) => quiet("explorer-no-activity", words::NO_ACTIVITY, k),
        (Some(acted), _) => column(
            "explorer-activity-list",
            acted
                .iter()
                .map(|frame| frame_line(view, frame, cx, k))
                .collect(),
        )
        .into_any_element(),
        (None, Loadable::Failed(refusal)) => refused(&refusal.message, cx, k),
        (None, _) => quiet("explorer-activity-loading", words::READING_ACTIVITY, k),
    };
    vec![head.into_any_element(), list]
}
