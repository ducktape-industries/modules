//! The Accounts tab and one account.
use super::*;
use crate::decode::scheme;
use identity::Account;

/// What `account` is, its manager named from the list: "Person", "Agent ·
/// managed by Dev · suspended", "Program · forge" (identity's own wording,
/// [`identity::view::kind`], over `Kind::badge` and `Kind::note`).
fn kind(view: &Explorer, account: &Account) -> String {
    identity::view::kind(&account.kind(), |manager| {
        view.account(manager)
            .map(|manager| manager.card.name.clone())
    })
}

pub(super) fn accounts(view: &Explorer, cx: Cx, theme: &Theme) -> AnyElement {
    let listed = match &view.accounts {
        Loadable::Ready(listed) | Loadable::Reloading(listed, _) => listed,
        Loadable::Failed(refusal) => return failed(&refusal.message, cx, theme),
        Loadable::Idle | Loadable::Loading(_) => {
            return quiet("explorer-accounts-loading", "Reading accounts…", theme);
        }
    };
    let list = rows("explorer-accounts-list", "Accounts", view);
    let routes = listed
        .list
        .iter()
        .map(|account| Route::Account(account.number))
        .collect();
    let list = list.shown(routes, |view, range, cx, theme| {
        let Some(listed) = view.accounts.ready() else {
            return Vec::new();
        };
        range
            .filter_map(|index| listed.list.get(index))
            .map(|account| account_row(view, account, cx, theme))
            .collect()
    });
    let caption_text = format!("{} · tx recorded", grouped(listed.list.len() as u64));
    div()
        .id("explorer-accounts")
        .flex_1()
        .min_h(px(0.))
        .flex()
        .flex_col()
        .child(
            heading(
                "explorer-accounts-heading",
                "Accounts",
                Some(caption(caption_text, theme)),
                theme,
            )
            .mr(design::size::SCROLLBAR),
        )
        .child(list.build(cx))
        .into_any_element()
}

fn account_row(view: &Explorer, account: &Account, cx: Cx, theme: &Theme) -> Stateful<Div> {
    let name = &account.card.name;
    let sent = view.used_by(account.number).map(|used| frames(&used));
    row(
        SharedString::from(format!("explorer-account-{}", account.number)).into(),
        name.clone(),
        Route::Account(account.number),
        cx,
        theme,
    )
    .child(design::avatar(name, design::size::AVATAR, theme))
    .child(div().flex_1().truncate().child(name.clone()))
    .child(div().text_color(theme.muted).child(kind(view, account)))
    .child(mono(format!("#{}", account.number)).text_color(theme.faint))
    .child(div().w(DEVICES_W).text_color(theme.muted).child(plural(
        account.keys().len() as u64,
        "device",
        "devices",
    )))
    .child(
        mono(sent.map_or_else(|| "–".to_owned(), |sent| plural(sent, "tx", "tx")))
            .w(COUNT_W)
            .flex()
            .justify_end()
            .text_color(theme.muted),
    )
}

fn frames(used: &[&activity::Used]) -> u64 {
    used.iter().map(|used| used.frames).sum()
}

/// What the account did, newest first, read on as it is scrolled.
fn acted(view: &Explorer, number: u64, cx: Cx, theme: &Theme) -> AnyElement {
    let Some(listing) = view.acted(number) else {
        return quiet("explorer-activity-loading", "Reading its activity…", theme);
    };
    let listing = listing.rows.clone();
    let (held, state) = listing.read(|paged| {
        let state = match paged.failed() {
            Some(refusal) => Err(refusal.clone()),
            None => Ok(paged.is_loading()),
        };
        (paged.rows().len(), state)
    });
    match state {
        Err(refusal) => failed(&refusal.message, cx, theme),
        Ok(true) => quiet("explorer-activity-loading", "Reading its activity…", theme),
        Ok(false) if held == 0 => quiet("explorer-no-activity", "No activity recorded.", theme),
        Ok(false) => {
            let list = rows("explorer-activity-list", "Activity", view);
            index_rows(list, listing, None, true, false)
                .build(cx)
                .into_any_element()
        }
    }
}

pub(super) fn account(view: &Explorer, number: u64, cx: Cx, theme: &Theme) -> AnyElement {
    let Some(account) = view.account(number) else {
        return match &view.accounts {
            Loadable::Ready(_) | Loadable::Reloading(_, _) => empty_state(
                "explorer-no-account",
                format!("No account #{number}"),
                "Identity holds no account by this number.",
                theme,
            )
            .into_any_element(),
            Loadable::Failed(refusal) => failed(&refusal.message, cx, theme),
            Loadable::Idle | Loadable::Loading(_) => {
                quiet("explorer-account-loading", "Reading the account…", theme)
            }
        };
    };
    let total = view.used_by(number).map(|used| frames(&used));
    let recorded = total.map(|total| {
        let total = plural(total, "transaction", "transactions");
        caption(format!("{total} recorded"), theme)
    });
    // the activity list scrolls for the page, as the Transactions tab's
    // does: the page is no scroller, the column beside it its own
    let activity = div()
        .id("explorer-activity")
        .flex_1()
        .min_h(px(0.))
        .flex()
        .flex_col()
        .border_r_1()
        .border_color(theme.border)
        .child(heading(
            "explorer-activity-heading",
            "Activity",
            recorded,
            theme,
        ))
        .child(acted(view, number, cx, theme));
    div()
        .id("explorer-account")
        .flex()
        .flex_col()
        .flex_1()
        .min_h(px(0.))
        .child(title(view, account, cx, theme))
        .child(
            div()
                .flex()
                .flex_1()
                .min_h(px(0.))
                .border_t_1()
                .border_color(theme.border)
                .child(activity)
                .child(side(view, account, theme)),
        )
        .into_any_element()
}

/// The account's name, what it is, and its link.
fn title(view: &Explorer, account: &Account, cx: Cx, theme: &Theme) -> impl IntoElement {
    let name = &account.card.name;
    let about = format!(
        "account {}   {}   {}",
        account.number,
        kind(view, account),
        plural(account.keys().len() as u64, "device", "devices")
    );
    div()
        .flex()
        .items_center()
        .gap_4()
        .px_5()
        .py_4()
        .child(design::avatar(name, PAGE_AVATAR, theme))
        .child(
            div()
                .flex()
                .flex_col()
                .gap_1()
                .child(
                    div()
                        .id("explorer-title")
                        .text_size(design::text::TITLE)
                        .role(Role::Heading)
                        .aria_level(1)
                        .child(name.clone()),
                )
                .child(
                    mono(about)
                        .text_size(design::text::CAPTION)
                        .text_color(theme.muted),
                ),
        )
        .child(div().flex_1())
        .children(copy_button(
            view,
            &Route::Account(account.number),
            cx,
            theme,
        ))
}

fn side(view: &Explorer, account: &Account, theme: &Theme) -> impl IntoElement {
    let now = view.chain.now();
    let last_used = view.last_used(account.number).unwrap_or_default();
    let devices = account.keys().iter().enumerate().map(|(index, key)| {
        let last = last_used.get(index).copied().flatten();
        let label = key
            .label
            .clone()
            .unwrap_or_else(|| format!("Device {}", index + 1));
        div()
            .flex()
            .items_center()
            .gap_2()
            .h(ROW_H)
            .px_5()
            .border_b_1()
            .border_color(theme.border)
            .child(div().child(label))
            .child(
                mono(format!("{} {}", scheme(key.scheme), short(&key.key)))
                    .flex_1()
                    .text_size(design::text::CAPTION)
                    .text_color(theme.faint),
            )
            .child(
                mono(match last {
                    Some(time) => format!("last used {} ago", ago(now, time)),
                    None => "not used lately".into(),
                })
                .text_size(design::text::CAPTION)
                .text_color(theme.muted),
            )
    });
    let mut used: Vec<(&str, u64)> = view
        .used_by(account.number)
        .unwrap_or_default()
        .into_iter()
        .map(|used| (used.program.as_str(), used.frames))
        .collect();
    used.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(b.0)));
    // a heading over nothing says nothing: it comes with its rows
    let used_heading =
        (!used.is_empty()).then(|| heading("explorer-used-heading", "Programs used", None, theme));
    let programs = used.into_iter().map(|(program, count)| {
        div()
            .flex()
            .items_center()
            .h(ROW_H)
            .px_5()
            .border_b_1()
            .border_color(theme.border)
            .child(mono(program.to_owned()).flex_1())
            .child(mono(format!("{count} tx")).text_color(theme.muted))
    });
    div()
        .id("explorer-account-side")
        .w(SIDE_W)
        .flex_shrink_0()
        .overflow_y_scroll()
        .child(heading(
            "explorer-devices-heading",
            "Devices",
            Some(caption(grouped(account.keys().len() as u64), theme)),
            theme,
        ))
        .children(devices)
        .children(used_heading)
        .children(programs)
}
