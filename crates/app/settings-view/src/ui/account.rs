//! The Account and Agents panes: who the seated key is and its keys, the
//! form that gives a bare key an account, and a person's agents with what
//! their manager does to them.
use super::*;
use crate::queries::{Account, Agent, Key};
use identity::Standing;

/// The identity header's avatar.
const AVATAR_XL: Pixels = px(40.);

pub(super) fn account(view: &Settings, cx: &mut Context<Settings>, theme: &Theme) -> AnyElement {
    match &view.account {
        Loadable::Ready(Some(Seat::Bare(key))) | Loadable::Reloading(Some(Seat::Bare(key)), _) => {
            column("settings/account/data")
                .gap_0()
                .child(who("Unregistered key", "no account yet", theme))
                .child(group("settings/keys", "Keys", Some(1), theme))
                .child(key_row(0, "Host key", key, theme))
                .child(create_account(view, cx, theme))
                .into_any_element()
        }
        // a key held by an agent that does not act goes by the agent
        Loadable::Ready(Some(Seat::Stopped { key, name, note }))
        | Loadable::Reloading(Some(Seat::Stopped { key, name, note }), _) => {
            column("settings/account/data")
                .gap_0()
                .child(who(name, "a key of an agent", theme))
                .child(group("settings/keys", "Keys", Some(1), theme))
                .child(key_row(0, "Host key", key, theme))
                .child(secondary("settings/account/note", note, theme).pt(design::space::MD))
                .into_any_element()
        }
        Loadable::Ready(Some(Seat::Account(account)))
        | Loadable::Reloading(Some(Seat::Account(account)), _) => {
            let kind = identity::view::kind(&account.kind, |_| None);
            let about = format!("account {} · {kind}", account.number);
            let keys = account
                .keys
                .iter()
                .enumerate()
                .map(|(i, key)| key_row(i, key.label.as_deref().unwrap_or("Key"), key, theme));
            column("settings/account/data")
                .gap_0()
                .child(who(&account.name, &about, theme))
                .child(group(
                    "settings/keys",
                    "Keys",
                    Some(account.keys.len()),
                    theme,
                ))
                .children(keys)
                .child(
                    secondary(
                        "settings/keys/help",
                        "Add a device from the account menu.",
                        theme,
                    )
                    .pt(design::space::MD),
                )
                .into_any_element()
        }
        Loadable::Ready(None) | Loadable::Reloading(None, _) => design::empty_state(
            "settings/account/empty",
            "No account",
            "No host key is selected. Sign in with a key to create an account.",
            theme,
        )
        .p_0()
        .into_any_element(),
        Loadable::Failed(refusal) => {
            let retry = cx.listener(|v: &mut Settings, _: &ClickEvent, _, cx| v.read_account(cx));
            design::refused("settings/account", refusal.message.clone(), theme, retry)
                .into_any_element()
        }
        Loadable::Idle | Loadable::Loading(_) => {
            secondary("settings/account/loading", "Reading your account…", theme).into_any_element()
        }
    }
}

/// Who the seated key is: an avatar, the name, one mono line under it.
fn who(name: &str, about: &str, theme: &Theme) -> Stateful<Div> {
    div()
        .id("settings/who")
        .flex()
        .items_center()
        .gap(design::space::LG)
        .pb(design::space::LG)
        .border_b_1()
        .border_color(theme.border)
        .child(
            design::avatar(name, AVATAR_XL, theme)
                .border_1()
                .border_color(theme.border),
        )
        .child(
            div()
                .flex()
                .flex_col()
                .gap(design::space::HAIR)
                .child(
                    div()
                        .id("settings/who/name")
                        .text_size(design::text::TITLE)
                        .font_weight(FontWeight::SEMIBOLD)
                        .child(name.to_owned()),
                )
                .child(
                    design::mono(about.to_owned())
                        .text_size(design::text::CAPTION)
                        .text_color(theme.muted),
                ),
        )
}

/// One key: its label over its short hex, a Validator tag on the right.
fn key_row(i: usize, label: &str, key: &Key, theme: &Theme) -> Stateful<Div> {
    let hex = design::short_hex(&abi::hex(&key.key));
    div()
        .id(format!("settings/key/{i}"))
        .flex()
        .items_center()
        .gap(design::space::LG)
        .py(design::space::MD)
        .border_b_1()
        .border_color(theme.border)
        .child(
            div()
                .flex_1()
                .flex()
                .flex_col()
                .gap(design::space::HAIR)
                .child(
                    div()
                        .font_weight(FontWeight::MEDIUM)
                        .child(label.to_owned()),
                )
                .child(
                    design::mono(hex)
                        .text_size(design::text::CAPTION)
                        .text_color(theme.muted),
                ),
        )
        .when(key.validator, |row| {
            row.child(design::badge(
                format!("settings/key/{i}/validator"),
                "Validator",
                theme.success,
                theme.success_soft,
            ))
        })
}

fn create_account(view: &Settings, cx: &mut Context<Settings>, theme: &Theme) -> AnyElement {
    let form = &view.create_account;
    let typed = cx.listener(|v: &mut Settings, text: &String, _, cx| {
        v.create_account.text = text.clone();
        cx.notify();
    });
    let pressed =
        cx.listener(|v: &mut Settings, _: &ClickEvent, _, cx| v.submit_create_account(cx));
    let mut name = field(
        "settings/account/create/name",
        "Name the new account",
        "Account name",
        form,
        theme,
        typed,
    );
    if !form.busy {
        name = name
            .on_submit(cx.listener(|v: &mut Settings, _: &(), _, cx| v.submit_create_account(cx)));
    }
    let control = div()
        .flex()
        .gap(design::space::SM)
        .child(name)
        .child(submit(
            "settings/account/create/submit",
            "Create account",
            "Creating…",
            form.busy,
            true,
            theme,
            pressed,
        ));
    column("settings/account/create")
        .gap_0()
        .child(design::setting_row(
            "settings/account/create/row",
            "Create an account",
            "Your key isn't linked to an account yet. An account gives you a name others see.",
            control,
            theme,
        ))
        .children(problem(
            "account/create",
            form,
            "Enter an account name.",
            theme,
        ))
        .into_any_element()
}

/// The Agents pane: every agent this person manages, one row each, then
/// the forms that create one and add a key to one.
pub(super) fn agents_pane(
    view: &Settings,
    cx: &mut Context<Settings>,
    theme: &Theme,
) -> AnyElement {
    match view.account.ready() {
        Some(Some(Seat::Account(account))) if account.manages => agents(view, account, cx, theme),
        // the menu lists Agents only for such an account
        _ => div().into_any_element(),
    }
}

fn agents(
    view: &Settings,
    account: &Account,
    cx: &mut Context<Settings>,
    theme: &Theme,
) -> AnyElement {
    let create_typed = cx.listener(|v: &mut Settings, text: &String, _, cx| {
        v.create_agent.text = text.clone();
        cx.notify();
    });
    let key_typed = cx.listener(|v: &mut Settings, text: &String, _, cx| {
        v.agent_key.text = text.clone();
        cx.notify();
    });
    let create = cx.listener(|v: &mut Settings, _: &ClickEvent, _, cx| v.submit_create_agent(cx));
    let add = cx.listener(|v: &mut Settings, _: &ClickEvent, _, cx| v.submit_agent_key(cx));
    let rows: Vec<AnyElement> = account
        .agents
        .iter()
        .map(|agent| self::agent(view, agent, cx, theme))
        .collect();
    let create_control = div()
        .flex()
        .gap(design::space::SM)
        .child(field(
            "settings/agents/create/name",
            "Name the new agent",
            "Agent name",
            &view.create_agent,
            theme,
            create_typed,
        ))
        .child(submit(
            "settings/agents/create/submit",
            "Create agent",
            "Creating…",
            view.create_agent.busy,
            false,
            theme,
            create,
        ));
    let key_control = div()
        .flex()
        .gap(design::space::SM)
        .child(field(
            "settings/agents/key/request",
            "Paste an agent's key request",
            "Agent key request",
            &view.agent_key,
            theme,
            key_typed,
        ))
        .child(submit(
            "settings/agents/key/submit",
            "Add key",
            "Adding…",
            view.agent_key.busy,
            false,
            theme,
            add,
        ));
    column("settings/agents")
        .gap_0()
        .child(
            secondary(
                "settings/agents/help",
                "Agents act as accounts you manage. You answer for what they do.",
                theme,
            )
            .pb(design::space::XS),
        )
        .children(rows)
        .children(problem("agents/standing", &view.agent_standing, "", theme))
        .child(group("settings/agents/add", "Add", None, theme))
        .child(design::setting_row(
            "settings/agents/create",
            "Create an agent",
            "A new account you manage. Give it a key next.",
            create_control,
            theme,
        ))
        .children(problem(
            "agents/create",
            &view.create_agent,
            "Enter an agent name.",
            theme,
        ))
        .child(design::setting_row(
            "settings/agents/key",
            "Add a key to an agent",
            "Paste the key request the agent's device shows.",
            key_control,
            theme,
        ))
        .children(problem("agents/key", &view.agent_key, "", theme))
        .into_any_element()
}

/// One agent's row: its avatar, name (a field while renamed) and standing,
/// `account n · k keys`, and what its manager does to it. Revoked, it only
/// reads as such.
fn agent(view: &Settings, agent: &Agent, cx: &mut Context<Settings>, theme: &Theme) -> AnyElement {
    let number = agent.number;
    let standing = agent.standing();
    let revoked = standing == Standing::Revoked;
    let keys = design::plural(agent.keys as u64, "key", "keys");
    let (label, foreground, background) = match standing {
        Standing::Active => ("active", theme.success, theme.success_soft),
        Standing::Suspended => ("suspended", theme.warning, theme.warning_soft),
        Standing::Revoked => ("revoked", theme.muted, theme.surface_raised),
    };
    let renaming = view.rename_agent.get(&number);
    let editing = renaming.is_some() && !revoked;
    let name: AnyElement = match renaming.filter(|_| !revoked) {
        Some(form) => {
            let typed = cx.listener(move |v: &mut Settings, text: &String, _, cx| {
                v.rename_agent.entry(number).or_default().text = text.clone();
                cx.notify();
            });
            let submitted = cx
                .listener(move |v: &mut Settings, _: &(), _, cx| v.submit_rename_agent(number, cx));
            let input = field(
                &format!("settings/agents/{number}/name"),
                &format!("Rename {}", agent.name),
                "New name",
                form,
                theme,
                typed,
            );
            match form.busy {
                true => input.into_any_element(),
                false => input.on_submit(submitted).into_any_element(),
            }
        }
        None => div()
            .id(format!("settings/agents/{number}/label"))
            .font_weight(FontWeight::MEDIUM)
            .child(agent.name.clone())
            .into_any_element(),
    };
    // as wide as the rename field at least; narrower, the actions wrap below
    let body = div()
        .flex_1()
        .min_w(super::FIELD_W)
        .flex()
        .flex_col()
        .gap(design::space::HAIR)
        .child(
            div()
                .flex()
                .items_center()
                .gap(design::space::SM)
                .child(name)
                // the field takes the row while renaming; the standing waits
                .when(!editing, |line| {
                    line.child(design::badge(
                        format!("settings/agents/{number}/standing"),
                        label,
                        foreground,
                        background,
                    ))
                }),
        )
        .child(
            design::mono(format!("account {number} · {keys}"))
                .id(format!("settings/agents/{number}/about"))
                .text_size(design::text::CAPTION)
                .text_color(theme.muted),
        );
    let row = div()
        .id(format!("settings/agents/{number}"))
        .flex()
        .flex_wrap()
        .items_center()
        .gap(design::space::LG)
        .py(design::space::MD)
        .child(
            design::avatar(&agent.name, design::size::AVATAR, theme)
                .bg(theme.agent_soft)
                .text_color(theme.agent)
                .font_weight(FontWeight::SEMIBOLD),
        )
        .child(body)
        .when(!revoked, |row| {
            row.child(actions(view, number, standing, renaming, cx, theme))
        });
    let mut card = div()
        .id(format!("settings/agents/{number}/card"))
        .flex()
        .flex_col()
        .border_b_1()
        .border_color(theme.border)
        .when(revoked, |card| card.opacity(0.6))
        .child(row);
    if let Some(form) = renaming {
        card = card.children(problem(
            &format!("agents/{number}/rename"),
            form,
            "Enter the agent's new name.",
            theme,
        ));
    }
    card.into_any_element()
}

/// An agent's presses: Rename (Save while its field is open, with
/// Cancel), Suspend or Resume, Revoke. Suspend and Revoke are confirmed
/// by the host before the key signs them: one press here.
fn actions(
    view: &Settings,
    number: u64,
    standing: Standing,
    renaming: Option<&Form>,
    cx: &mut Context<Settings>,
    theme: &Theme,
) -> impl IntoElement {
    let (toggle, toggle_label, toggle_op) = match standing {
        Standing::Suspended => ("resume", "Resume", identity::Op::Resume { account: number }),
        Standing::Active | Standing::Revoked => (
            "suspend",
            "Suspend",
            identity::Op::Suspend { account: number },
        ),
    };
    let toggled = cx.listener(move |v: &mut Settings, _: &ClickEvent, _, cx| {
        v.set_standing(toggle_op.clone(), cx)
    });
    let revoke = cx.listener(move |v: &mut Settings, _: &ClickEvent, _, cx| {
        v.set_standing(identity::Op::Revoke { account: number }, cx)
    });
    let renamed = cx
        .listener(move |v: &mut Settings, _: &ClickEvent, _, cx| v.submit_rename_agent(number, cx));
    let cancel = cx
        .listener(move |v: &mut Settings, _: &ClickEvent, _, cx| v.cancel_rename_agent(number, cx));
    let busy = view.agent_standing.busy;
    div()
        .id(format!("settings/agents/{number}/actions"))
        .flex()
        .flex_none()
        .items_center()
        .gap(design::space::SM)
        .child(submit(
            &format!("settings/agents/{number}/rename"),
            if renaming.is_some() { "Save" } else { "Rename" },
            "Renaming…",
            renaming.is_some_and(|form| form.busy),
            false,
            theme,
            renamed,
        ))
        .when(renaming.is_some_and(|form| !form.busy), |row| {
            row.child(
                button(
                    format!("settings/agents/{number}/rename-cancel"),
                    "Cancel",
                    theme,
                )
                .border_color(theme.background)
                .text_color(theme.muted)
                .on_click(cancel),
            )
        })
        .child(submit(
            &format!("settings/agents/{number}/{toggle}"),
            toggle_label,
            "Working…",
            busy,
            false,
            theme,
            toggled,
        ))
        .child(
            submit(
                &format!("settings/agents/{number}/revoke"),
                "Revoke",
                "Working…",
                busy,
                false,
                theme,
                revoke,
            )
            .text_color(theme.danger)
            .border_color(theme.background),
        )
}
