//! The Programs tab: what the registry runs, lists and will change, who
//! maintains each name, and the presses the reader's grants or voices
//! allow (`acting.rs`). A form opens under the list it changes, outside
//! it: a list takes the arrows and Enter, a form's fields need them.
use super::*;
use crate::Network;
use crate::acting::{self, Edit, Form, Path};
use module_registry::{self as registry, Grant, NOTICE, Pending, Scope};

pub(super) fn programs(view: &Explorer, cx: Cx, theme: &Theme) -> AnyElement {
    let network = match &view.network {
        Loadable::Ready(network) | Loadable::Reloading(network, _) => network,
        Loadable::Failed(refusal) => return failed(&refusal.message, cx, theme),
        Loadable::Idle | Loadable::Loading(_) => {
            return quiet("explorer-programs-loading", "Reading the registry…", theme);
        }
    };
    if network.programs.is_empty() && network.views.is_empty() && network.changes.is_empty() {
        return empty_state(
            "explorer-empty",
            "No programs",
            "The registry of this network runs nothing yet.",
            theme,
        )
        .into_any_element();
    }
    let add = view.adds().then(|| {
        press("explorer-add", "Add", cx, theme, |view, cx| {
            view.open_form(Edit::Add, Path::Grant, cx)
        })
    });
    let count = caption(
        plural(network.programs.len() as u64, "program", "programs"),
        theme,
    );
    let running = running(view, network, cx, theme);
    let listed = listed(view, network, cx, theme);
    let scheduled: Vec<_> = network
        .changes
        .iter()
        .enumerate()
        .map(|(index, change)| scheduled(view, index, change, cx, theme))
        .collect();
    let nothing_scheduled = scheduled.is_empty().then(|| {
        quiet(
            "explorer-no-changes",
            "Nothing is scheduled against the registry.",
            theme,
        )
    });
    let running_form = form(
        view,
        |edit| !edits_a_view(edit) && *edit != Edit::Grant,
        cx,
        theme,
    );
    let views_form = form(view, edits_a_view, cx, theme);
    div()
        .id("explorer-list")
        .child(heading(
            "explorer-running-header",
            "Running",
            Some(beside([Some(count), add])),
            theme,
        ))
        .child(running)
        .children(running_form)
        .when(!listed.is_empty(), |list| {
            list.child(heading(
                "explorer-views-header",
                "Views",
                Some(caption(plural(listed.len() as u64, "view", "views"), theme)),
                theme,
            ))
            .children(listed)
            .children(views_form)
        })
        .child(heading(
            "explorer-scheduled-header",
            "Scheduled",
            None,
            theme,
        ))
        .children(scheduled)
        .children(nothing_scheduled)
        .child(access(view, network, cx, theme))
        .into_any_element()
}

fn edits_a_view(edit: &Edit) -> bool {
    matches!(
        edit,
        Edit::Upgrade { view: true, .. } | Edit::Remove { view: true, .. }
    )
}

/// What sits on a heading's right, in a row.
fn beside(parts: impl IntoIterator<Item = Option<AnyElement>>) -> AnyElement {
    div()
        .flex()
        .items_center()
        .gap_3()
        .children(parts.into_iter().flatten())
        .into_any_element()
}

/// Each running program, a row that opens its transactions, with who
/// maintains it and the reader's presses.
fn running(view: &Explorer, network: &Network, cx: Cx, theme: &Theme) -> Stateful<Div> {
    let list = rows("explorer-programs-list", "Programs", view);
    network
        .programs
        .iter()
        .fold(list, |list, entry| {
            let route = Route::Transactions(Some(entry.program.clone()));
            let row = row(
                SharedString::from(format!("explorer-program-{}", entry.program)).into(),
                entry.program.clone(),
                route.clone(),
                cx,
                theme,
            )
            .child(named(view, network, &entry.program, theme))
            .child(
                div()
                    .text_size(design::text::SECONDARY)
                    .text_color(theme.muted)
                    .child(plural(
                        entry.params.len() as u64,
                        "param byte",
                        "param bytes",
                    )),
            )
            .child(mono(short(entry.code.digest())).text_color(theme.muted))
            .children(presses(view, network, &entry.program, false, cx, theme));
            list.row(route, row, theme)
        })
        .build(cx)
}

/// The view-only entries. One sends nothing, so it has no transactions to
/// open.
fn listed(view: &Explorer, network: &Network, cx: Cx, theme: &Theme) -> Vec<AnyElement> {
    network
        .views
        .iter()
        .map(|listed| {
            line(
                SharedString::from(format!("explorer-view-{}", listed.name)).into(),
                theme,
            )
            .child(named(view, network, &listed.name, theme))
            .child(
                div()
                    .text_size(design::text::SECONDARY)
                    .text_color(theme.muted)
                    .child("view only"),
            )
            .child(mono(short(listed.view.digest())).text_color(theme.muted))
            .children(presses(view, network, &listed.name, true, cx, theme))
            .into_any_element()
        })
        .collect()
}

/// A name and who changes it: the validators' vote for a role's program,
/// else its maintainers (an agent marked so) and, when any account holds
/// every module, those too; a hook marked so.
fn named(view: &Explorer, network: &Network, name: &str, theme: &Theme) -> Div {
    let bound = network
        .roles
        .as_ref()
        .is_some_and(|roles| registry::role_bound(roles, name));
    let hook = network.hooks.iter().any(|hook| hook == name);
    let maintainers: Vec<u64> = network
        .grants
        .iter()
        .filter(|grant| grant.scope == Scope::Module(name.to_owned()))
        .map(|grant| grant.account)
        .collect();
    let every = network.grants.iter().any(|grant| grant.scope == Scope::All);
    let who: Vec<AnyElement> = match bound {
        true => vec![muted("validators' vote", theme)],
        false => maintainers
            .iter()
            .map(|number| holder(view, *number, theme))
            .chain(every.then(|| muted("every-module maintainers", theme)))
            .collect(),
    };
    div()
        .flex_1()
        .min_w(px(0.))
        .flex()
        .items_center()
        .gap_2()
        .overflow_hidden()
        .child(mono(name.to_owned()))
        .children(hook.then(|| chip(format!("explorer-hook-{name}"), "hook", theme)))
        .children(who)
}

fn muted(text: &str, theme: &Theme) -> AnyElement {
    div()
        .text_size(design::text::SECONDARY)
        .text_color(theme.muted)
        .child(text.to_owned())
        .into_any_element()
}

fn chip(id: String, label: &str, theme: &Theme) -> AnyElement {
    design::badge(
        SharedString::from(id),
        label.to_owned(),
        theme.muted,
        theme.surface,
    )
    .into_any_element()
}

/// An account by its name (`account N` while unread), an agent marked so,
/// the reader marked "(you)".
fn holder(view: &Explorer, number: u64, theme: &Theme) -> AnyElement {
    let account = view.account(number);
    let name = account.map_or_else(|| format!("account {number}"), |a| a.card.name.clone());
    let you = view.acting.account == Some(number);
    let agent = account.is_some_and(|a| matches!(a.kind(), identity::Kind::Managed { .. }));
    div()
        .flex()
        .items_center()
        .gap_1()
        .text_size(design::text::SECONDARY)
        .child(if you { format!("{name} (you)") } else { name })
        .children(agent.then(|| chip(format!("explorer-agent-{number}"), "agent", theme)))
        .into_any_element()
}

/// Upgrade and Remove on the reader's path to `name`: straight to the
/// registry by grant, proposed by vote, or none. A role's program is never
/// removed; a hook only by vote, which a maintainer with voices proposes.
fn presses(
    view: &Explorer,
    network: &Network,
    name: &str,
    listed: bool,
    cx: Cx,
    theme: &Theme,
) -> Vec<AnyElement> {
    let Some(path) = view.path(name) else {
        return Vec::new();
    };
    let bound = network
        .roles
        .as_ref()
        .is_none_or(|roles| registry::role_bound(roles, name));
    let hook = network.hooks.iter().any(|hook| hook == name);
    let edit = |remove: bool, path: Path| {
        let name = name.to_owned();
        move |view: &mut Explorer, cx: &mut Context<Explorer>| {
            let edit = match remove {
                false => Edit::Upgrade {
                    name: name.clone(),
                    view: listed,
                },
                true => Edit::Remove {
                    name: name.clone(),
                    view: listed,
                },
            };
            view.open_form(edit, path, cx)
        }
    };
    let upgrade = match path {
        Path::Grant => "Upgrade",
        Path::Vote => "Propose upgrade",
    };
    let mut out = vec![press(
        format!("explorer-upgrade-{name}"),
        upgrade,
        cx,
        theme,
        edit(false, path),
    )];
    let removes = match path {
        Path::Grant if hook => view.acting.votes().then_some(Path::Vote),
        path => Some(path),
    };
    if let Some(path) = removes.filter(|_| !bound) {
        let remove = match path {
            Path::Grant => "Remove",
            Path::Vote => "Propose remove",
        };
        out.push(press(
            format!("explorer-remove-{name}"),
            remove,
            cx,
            theme,
            edit(true, path),
        ));
    }
    out
}

/// One change the registry will fold in: what it does, to what, when, who
/// made it; how long the vote has to cancel one a grant made; Cancel by
/// grant or Propose cancel by vote.
fn scheduled(
    view: &Explorer,
    index: usize,
    pending: &Pending,
    cx: Cx,
    theme: &Theme,
) -> AnyElement {
    let change = &pending.change;
    let (foreground, background) = match change.code() {
        None => (theme.danger, theme.danger_soft),
        Some(_) => (theme.accent_foreground, theme.accent_soft),
    };
    let by = match pending.by {
        Some(number) => format!("by {}", name_of(view, number)),
        None => "by vote".into(),
    };
    let window = pending.by.and(view.status.ready()).map(|status| {
        let blocks = pending.height.saturating_sub(status.height);
        format!(
            "validators can cancel for {} min",
            minutes(blocks, status.block_time_ms)
        )
    });
    let program = change.program();
    let path = match pending.by {
        Some(_) if view.maintains(program) => Some(Path::Grant),
        _ => view.acting.votes().then_some(Path::Vote),
    };
    let cancel = path.map(|path| {
        let op = acting::cancel(pending);
        let label = match path {
            Path::Grant => "Cancel",
            Path::Vote => "Propose cancel",
        };
        press(
            format!("explorer-cancel-{index}"),
            label,
            cx,
            theme,
            move |view, cx| view.press(path, op.clone(), cx),
        )
    });
    line(ElementId::named_usize("explorer-change", index), theme)
        .child(design::badge(
            ElementId::named_usize("explorer-change-verb", index),
            change.verb(),
            foreground,
            background,
        ))
        .child(
            div()
                .flex_1()
                .flex()
                .items_center()
                .gap_2()
                .child(mono(program.to_string()))
                .child(muted(&by, theme))
                .children(window.map(|window| muted(&window, theme))),
        )
        .child(
            div()
                .text_size(design::text::SECONDARY)
                .text_color(theme.muted)
                .child(format!("at {}", pending.height)),
        )
        .children(
            change
                .code()
                .map(|code| mono(short(code.digest())).text_color(theme.muted)),
        )
        .children(cancel)
        .into_any_element()
}

/// How long `blocks` take at the node's block time, in whole minutes.
fn minutes(blocks: u64, block_time_ms: u64) -> u64 {
    (blocks * block_time_ms).div_ceil(60_000)
}

fn name_of(view: &Explorer, number: u64) -> String {
    view.account(number)
        .map_or_else(|| format!("account {number}"), |a| a.card.name.clone())
}

/// Every grant: what it covers, who holds it. The reader's own steps down
/// by grant; a reader with voices proposes a grant or a revoke.
fn access(view: &Explorer, network: &Network, cx: Cx, theme: &Theme) -> Div {
    let about = view.status.ready().map_or_else(String::new, |status| {
        format!(" (about {} min)", minutes(NOTICE, status.block_time_ms))
    });
    let notice = caption(
        format!("changes by a maintainer wait {NOTICE} blocks{about}"),
        theme,
    );
    let propose = view.acting.votes().then(|| {
        press(
            "explorer-propose-grant",
            "Propose grant",
            cx,
            theme,
            |view, cx| view.open_form(Edit::Grant, Path::Vote, cx),
        )
    });
    let rows: Vec<AnyElement> = network
        .grants
        .iter()
        .enumerate()
        .map(|(index, grant)| granted(view, network, index, grant, cx, theme))
        .collect();
    let none = rows.is_empty().then(|| {
        quiet(
            "explorer-no-grants",
            "No account maintains a module: only the validators' vote changes the registry.",
            theme,
        )
    });
    div()
        .child(heading(
            "explorer-access-header",
            "Access",
            Some(beside([Some(notice), propose])),
            theme,
        ))
        .children(form(view, |edit| *edit == Edit::Grant, cx, theme))
        .children(rows)
        .children(none)
}

fn granted(
    view: &Explorer,
    network: &Network,
    index: usize,
    grant: &Grant,
    cx: Cx,
    theme: &Theme,
) -> AnyElement {
    let unseated = match &grant.scope {
        Scope::Module(id) if !network.names(id) => Some(muted(
            &format!(
                "runs nothing yet: {} may add it",
                name_of(view, grant.account)
            ),
            theme,
        )),
        _ => None,
    };
    let own = view.acting.account == Some(grant.account);
    let path = match own {
        true => Some(Path::Grant),
        false => view.acting.votes().then_some(Path::Vote),
    };
    let revoke = path.map(|path| {
        let op = registry::Op::Revoke(grant.clone());
        let label = match path {
            Path::Grant => "Step down",
            Path::Vote => "Propose revoke",
        };
        press(
            format!("explorer-revoke-{index}"),
            label,
            cx,
            theme,
            move |view, cx| view.press(path, op.clone(), cx),
        )
    });
    line(ElementId::named_usize("explorer-grant", index), theme)
        .child(mono(grant.scope.to_string()).w(px(140.)).flex_shrink_0())
        .child(
            div()
                .flex_1()
                .flex()
                .items_center()
                .gap_2()
                .child(holder(view, grant.account, theme))
                .children(unseated),
        )
        .child(muted("Maintainer", theme))
        .children(revoke)
        .into_any_element()
}

/// The open form, when `here` says it belongs under this list.
fn form(
    view: &Explorer,
    here: impl Fn(&Edit) -> bool,
    cx: Cx,
    theme: &Theme,
) -> Option<AnyElement> {
    let form = view.acting.form.as_ref().filter(|form| here(&form.edit))?;
    let network = view.network.ready()?;
    let tip = view.status.ready().map_or(0, |status| status.height);
    let title = match (&form.edit, form.path) {
        (Edit::Upgrade { name, .. }, Path::Grant) => format!("Upgrade {name}"),
        (Edit::Upgrade { name, .. }, Path::Vote) => format!("Propose upgrade · {name}"),
        (Edit::Remove { name, .. }, Path::Grant) => format!("Remove {name}"),
        (Edit::Remove { name, .. }, Path::Vote) => format!("Propose remove · {name}"),
        (Edit::Add, _) => "Add a module".into(),
        (Edit::Grant, _) => "Propose a grant".into(),
    };
    let field = |id: &str, field: &TextField, name: &str, hint: &str, width: f32| {
        Input::new(id.to_owned(), field, name.to_owned())
            .h(design::size::CONTROL)
            .w(px(width))
            .px_2()
            .border_1()
            .border_color(theme.border_strong)
            .bg(theme.background)
            .text_size(design::text::SECONDARY)
            .placeholder(hint.to_owned())
            .into_any_element()
    };
    let mut fields: Vec<AnyElement> = Vec::new();
    let mut notes: Vec<String> = Vec::new();
    match &form.edit {
        Edit::Add => {
            fields.push(field(
                "explorer-form-name",
                &form.name,
                "Name",
                "module id",
                160.,
            ));
            let kind = |view_kind: bool, label: &str| {
                let pick = cx.listener(move |view: &mut Explorer, _: &ClickEvent, _, cx| {
                    view.add_view(view_kind, cx)
                });
                design::button(
                    SharedString::from(format!("explorer-form-{label}")),
                    label.to_owned(),
                    theme,
                    pick,
                )
                .selected(form.view == view_kind)
                .into_any_element()
            };
            fields.push(kind(false, "program"));
            fields.push(kind(true, "view"));
        }
        Edit::Grant => {
            fields.push(field(
                "explorer-form-account",
                &form.account,
                "Account",
                "account number",
                140.,
            ));
            fields.push(field(
                "explorer-form-name",
                &form.name,
                "Module",
                "a module, or empty for every module",
                240.,
            ));
            notes.push(who(view, form));
            notes.push("This proposes a vote: the validators carry it in Nodes.".into());
        }
        Edit::Upgrade { .. } | Edit::Remove { .. } => {}
    }
    if matches!(form.edit, Edit::Upgrade { .. } | Edit::Add) {
        fields.push(field(
            "explorer-form-code",
            &form.code,
            "Code id",
            "sha256:… of the published code",
            280.,
        ));
    }
    if form.edit == Edit::Add && !form.view {
        fields.push(field(
            "explorer-form-params",
            &form.params,
            "Params",
            "params hex, or empty",
            160.,
        ));
    }
    if form.edit != Edit::Grant {
        fields.push(field(
            "explorer-form-height",
            &form.height,
            "At block",
            "block height",
            140.,
        ));
        notes.push(match form.path {
            Path::Grant => format!(
                "Block {} or later: the validators get {NOTICE} blocks to look, and can cancel it by vote until it lands.",
                grouped(acting::earliest(tip))
            ),
            Path::Vote => "Lands at this block once the validators carry the vote.".into(),
        });
    }
    if let Edit::Upgrade { name, view: false } = &form.edit
        && let Some(entry) = network.programs.iter().find(|entry| &entry.program == name)
    {
        notes.push(format!(
            "Params kept: {}, as {name} runs now.",
            plural(entry.params.len() as u64, "byte", "bytes")
        ));
    }
    let submit = match form.path {
        Path::Grant => "Schedule",
        Path::Vote => "Propose",
    };
    let send = cx.listener(|view: &mut Explorer, _: &ClickEvent, _, cx| view.submit_form(cx));
    let close = cx.listener(|view: &mut Explorer, _: &ClickEvent, _, cx| view.close_form(cx));
    Some(
        div()
            .id("explorer-form")
            .flex()
            .flex_col()
            .gap_2()
            .px_5()
            .py_3()
            .border_b_1()
            .border_color(theme.border)
            .bg(theme.surface)
            .child(design::heading("explorer-form-title", title, 3, theme))
            .child(
                div()
                    .flex()
                    .flex_wrap()
                    .items_center()
                    .gap_2()
                    .children(fields),
            )
            .children(notes.into_iter().map(|note| {
                div()
                    .text_size(design::text::SECONDARY)
                    .text_color(theme.muted)
                    .child(note)
            }))
            .children(form.problem.as_ref().map(|problem| {
                div()
                    .id("explorer-form-problem")
                    .text_size(design::text::SECONDARY)
                    .text_color(theme.danger)
                    .child(problem.clone())
            }))
            .child(
                div()
                    .flex()
                    .gap_2()
                    .child(
                        design::button("explorer-form-submit", submit, theme, send)
                            .kind(design::Kind::Primary)
                            .enabled(!view.acting.busy),
                    )
                    .child(
                        design::button("explorer-form-close", "Close", theme, close)
                            .kind(design::Kind::Outline),
                    ),
            )
            .into_any_element(),
    )
}

/// The account a grant form names, as typed: its name and what it is.
fn who(view: &Explorer, form: &Form) -> String {
    let typed = form.account.text();
    let typed = typed.trim().trim_start_matches('#');
    match typed.parse::<u64>().ok() {
        None => "Type the account's number.".into(),
        Some(number) => match view.account(number) {
            Some(account) => format!(
                "#{number} · {} · {}",
                account.card.name,
                identity::view::kind(&account.kind(), |manager| {
                    view.account(manager).map(|m| m.card.name.clone())
                })
            ),
            None => format!("No account {number} here."),
        },
    }
}

/// A list row that opens nothing.
fn line(id: ElementId, theme: &Theme) -> Stateful<Div> {
    div()
        .id(id)
        .flex()
        .items_center()
        .gap_4()
        .h(ROW_H)
        .px_5()
        .border_b_1()
        .border_color(theme.border)
}
