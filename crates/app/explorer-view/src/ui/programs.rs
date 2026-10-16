//! The Programs tab: what the programs role runs, lists and will change, who
//! maintains each name, and the presses the reader's grants or voices
//! allow (`acting.rs`). A form opens under the list it changes, outside
//! it: a list takes the arrows and Enter, a form's fields need them. One
//! program, or one view-only entry, opens as a page ([`program`]): what its
//! code declares first, then the code, who changes it and what is coming.
use super::*;
use crate::Network;
use crate::acting::{self, Edit, Form, Path};
use crate::decode::{preview, size};
use crate::state::blob_ref;
use ::programs::{Grant, NOTICE, Pending, Scope};
use view::design::Ladder;

pub(super) fn programs(view: &Explorer, cx: Cx, theme: &Theme, ladder: &Ladder) -> AnyElement {
    let network = match &view.network {
        Loadable::Ready(network) | Loadable::Reloading(network, _) => network,
        Loadable::Failed(refusal) => return failed(&refusal.message, cx, theme, ladder),
        Loadable::Idle | Loadable::Loading(_) => {
            return quiet(
                "explorer-programs-loading",
                "Reading the programs role…",
                theme,
            );
        }
    };
    if network.programs.is_empty() && network.views.is_empty() && network.changes.is_empty() {
        return empty_state(
            "explorer-empty",
            "No programs",
            "The programs role of this network runs nothing yet.",
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
    let running = running(view, network, cx, theme, ladder);
    let views = listed(view, network, cx, theme, ladder);
    let scheduled: Vec<_> = network
        .changes
        .iter()
        .enumerate()
        .map(|(index, change)| scheduled(view, index, change, cx, theme, ladder))
        .collect();
    let nothing_scheduled = scheduled.is_empty().then(|| {
        quiet(
            "explorer-no-changes",
            "Nothing is scheduled against the programs role.",
            theme,
        )
    });
    let running_form = form(
        view,
        |edit| !edits_a_view(edit) && *edit != Edit::Grant,
        cx,
        theme,
        ladder,
    );
    let views_form = form(view, edits_a_view, cx, theme, ladder);
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
        .when_some(views, |list, views| {
            list.child(heading(
                "explorer-views-header",
                "Views",
                Some(caption(
                    plural(network.views.len() as u64, "view", "views"),
                    theme,
                )),
                theme,
            ))
            .child(views)
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
        .child(access(view, network, cx, theme, ladder))
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

/// Each running program, a row that opens its page, with who maintains it
/// and the reader's presses.
fn running(
    view: &Explorer,
    network: &Network,
    cx: Cx,
    theme: &Theme,
    ladder: &Ladder,
) -> Stateful<Div> {
    let list = rows("explorer-programs-list", "Programs", view);
    network
        .programs
        .iter()
        .fold(list, |list, entry| {
            let route = Route::Program(entry.program.clone());
            let row = row(
                SharedString::from(format!("explorer-program-{}", entry.program)).into(),
                entry.program.clone(),
                route.clone(),
                cx,
                theme,
            )
            .child(named(view, network, &entry.program, theme, ladder))
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

/// The view-only entries, each a row that opens its page; `None` where the
/// programs role lists none.
fn listed(
    view: &Explorer,
    network: &Network,
    cx: Cx,
    theme: &Theme,
    ladder: &Ladder,
) -> Option<Stateful<Div>> {
    if network.views.is_empty() {
        return None;
    }
    let list = rows("explorer-views-list", "Views", view);
    let list = network.views.iter().fold(list, |list, listed| {
        let route = Route::Program(listed.name.clone());
        let row = row(
            SharedString::from(format!("explorer-view-{}", listed.name)).into(),
            listed.name.clone(),
            route.clone(),
            cx,
            theme,
        )
        .child(named(view, network, &listed.name, theme, ladder))
        .child(muted("view only", theme))
        .child(mono(short(listed.view.digest())).text_color(theme.muted))
        .children(presses(view, network, &listed.name, true, cx, theme));
        list.row(route, row, theme)
    });
    Some(list.build(cx))
}

/// A name and who changes it ([`changers`]); a hook marked so.
fn named(view: &Explorer, network: &Network, name: &str, theme: &Theme, ladder: &Ladder) -> Div {
    let hook = network.hooks.iter().any(|hook| hook == name);
    div()
        .flex_1()
        .min_w(px(0.))
        .flex()
        .items_center()
        .gap_2()
        .overflow_hidden()
        .child(mono(name.to_owned()))
        .children(hook.then(|| chip(format!("explorer-hook-{name}"), "hook", theme, ladder)))
        .children(changers(view, network, name, theme, ladder))
}

/// Who changes `name`: the validators' vote for a role's program, else its
/// maintainers (an agent marked so) and, when any account holds every
/// program, those too. Empty: no one but the vote, which holds every name.
fn changers(
    view: &Explorer,
    network: &Network,
    name: &str,
    theme: &Theme,
    ladder: &Ladder,
) -> Vec<AnyElement> {
    let bound = network
        .roles
        .as_ref()
        .is_some_and(|roles| ::programs::role_bound(roles, name));
    if bound {
        return vec![muted("validators' vote", theme)];
    }
    let every = network.grants.iter().any(|grant| grant.scope == Scope::All);
    network
        .grants
        .iter()
        .filter(|grant| grant.scope == Scope::Program(name.to_owned()))
        .map(|grant| holder(view, grant.account, theme, ladder))
        .chain(every.then(|| muted("every-program maintainers", theme)))
        .collect()
}

/// The role `name` fills for the kernel, in the kernel's word for it: the
/// roles are the chain's own table, so a role it gains is named here as is.
fn role_of<'a>(roles: &'a abi::Roles, name: &str) -> Option<&'a str> {
    roles
        .iter()
        .find(|(_, program)| program.as_str() == name)
        .map(|(role, _)| role)
}

/// What a capability lets a view do, in a line a person reads: the phone's
/// "uses camera" for a view. Every capability is worded here, so a new one
/// is worded before it ships.
fn allows(capability: Capability) -> &'static str {
    match capability {
        Capability::Chain => "reads the chain: blocks, transactions and the node's status",
        Capability::Program => "asks programs questions and follows what they change",
        Capability::Op => "sends operations to the chain in your name",
        Capability::Invite => "mints invites with the node's key",
        Capability::Link => "opens links: another view, or the browser",
        Capability::Blob => "reads blobs the node holds",
        Capability::Host => "knows the session: who you are, which chain, when it is on screen",
        Capability::Clock => "wakes on a timer",
        Capability::Clipboard => "reads and writes the clipboard",
        Capability::Notify => "posts notifications",
        Capability::Store => "keeps its own data on this device",
        Capability::Web => "shows web pages, and takes the web links the app opens",
        Capability::Live => {
            "sends and hears live streams in its program's rooms, as the program grants"
        }
    }
}

/// One program, or one view-only entry: what its code declares first (the
/// capabilities its view asks the host for, as a phone's app page says
/// "uses camera"), then the code and its size, the params, who maintains
/// it, the role and hook it fills, what is scheduled for it, and the way to
/// its transactions. The reader's presses sit beside the title, their form
/// under it.
pub(super) fn program(
    view: &Explorer,
    name: &str,
    cx: Cx,
    theme: &Theme,
    ladder: &Ladder,
) -> AnyElement {
    let network = match &view.network {
        Loadable::Ready(network) | Loadable::Reloading(network, _) => network,
        Loadable::Failed(refusal) => return failed(&refusal.message, cx, theme, ladder),
        Loadable::Idle | Loadable::Loading(_) => {
            return quiet(
                "explorer-program-loading",
                "Reading the programs role…",
                theme,
            );
        }
    };
    let entry = view.runs(name);
    let Some(code) = view.code_of(name) else {
        return empty_state(
            "explorer-no-program",
            format!("Nothing called {name}"),
            "The programs role runs and lists nothing by this name.",
            theme,
        )
        .into_any_element();
    };
    let listed = entry.is_none();
    let hook = network.hooks.iter().any(|hook| hook == name);
    let role = network
        .roles
        .as_ref()
        .and_then(|roles| role_of(roles, name));
    let blob = view.manifest.ready().and_then(|blob| blob.as_ref());
    let manifest = blob
        .and_then(|blob| blob.view.as_ref())
        .and_then(|view| view.manifest.as_ref());
    let title = div()
        .flex()
        .items_center()
        .gap_3()
        .pr_5()
        .child(titled(
            if listed { "view only" } else { "program" },
            name.to_owned(),
            theme,
        ))
        .children(hook.then(|| chip("explorer-program-hook".into(), "hook", theme, ladder)))
        .children(role.map(|role| chip("explorer-program-role".into(), role, theme, ladder)))
        .child(div().flex_1())
        .children(copy_button(
            view,
            &Route::Program(name.to_owned()),
            cx,
            theme,
        ))
        .children(presses(view, network, name, listed, cx, theme));
    let page_form = form(
        view,
        |edit| match edit {
            Edit::Upgrade { name: edited, .. } | Edit::Remove { name: edited, .. } => {
                edited == name
            }
            Edit::Add | Edit::Grant => false,
        },
        cx,
        theme,
        ladder,
    );
    let count = manifest.map(|manifest| {
        caption(
            plural(
                manifest.capabilities.len() as u64,
                "capability",
                "capabilities",
            ),
            theme,
        )
    });
    let id = blob_ref(&code);
    let copy = {
        let id = id.clone();
        press("explorer-copy-code", "Copy", cx, theme, move |view, cx| {
            view.copy(id.clone(), "the code id", cx)
        })
    };
    let code_field = div()
        .flex_1()
        .flex()
        .items_center()
        .gap_3()
        .min_w(px(0.))
        .child(
            mono(id)
                .id("explorer-code-id")
                .text_size(design::text::SECONDARY)
                .overflow_hidden(),
        )
        .child(copy);
    let sizes = blob.map(|blob| match &blob.view {
        Some(carried) if !listed => format!(
            "{} of code, {} of it the view",
            size(blob.size),
            size(carried.size)
        ),
        _ => size(blob.size),
    });
    let params = entry.map(|entry| match entry.params.is_empty() {
        true => muted("none", theme),
        false => mono(preview(
            entry.params.len() as u64,
            &entry.params[..entry.params.len().min(32)],
        ))
        .into_any_element(),
    });
    let mut who = changers(view, network, name, theme, ladder);
    if who.is_empty() {
        who.push(muted("no account: the validators' vote only", theme));
    }
    let about = div()
        .flex()
        .flex_wrap()
        .items_center()
        .gap_2()
        .children(who);
    let declares = manifest.map(|manifest| {
        let mut words = manifest.name.clone();
        if !manifest.description.is_empty() {
            words = format!("{words} · {}", manifest.description);
        }
        words
    });
    let talks_to = manifest
        .filter(|manifest| !manifest.targets.is_empty())
        .map(|manifest| manifest.targets.join(", "));
    let blocks = view.chain.blocks.len() as u64;
    let sent = view.chain.txs.iter().filter(|tx| tx.target == name).count() as u64;
    // no window yet: the way to its transactions, without a count of nothing
    let transactions = (!listed).then(|| {
        line("explorer-program-transactions".into(), theme).child(link(
            "explorer-program-txs".into(),
            match blocks {
                0 => "Transactions →".to_owned(),
                _ => format!(
                    "{} in the last {} →",
                    plural(sent, "transaction", "transactions"),
                    plural(blocks, "block", "blocks")
                ),
            },
            Route::Transactions(Some(name.to_owned())),
            cx,
            theme,
        ))
    });
    let scheduled: Vec<AnyElement> = network
        .changes
        .iter()
        .enumerate()
        .filter(|(_, pending)| pending.change.program() == name)
        .map(|(index, pending)| scheduled(view, index, pending, cx, theme, ladder))
        .collect();
    let nothing_scheduled = scheduled.is_empty().then(|| {
        quiet(
            "explorer-program-no-changes",
            "Nothing is scheduled for it.",
            theme,
        )
    });
    div()
        .id("explorer-program")
        .flex()
        .flex_col()
        .child(title)
        .children(page_form)
        .child(heading(
            "explorer-capabilities-header",
            "Capabilities",
            count,
            theme,
        ))
        .children(capabilities(view, listed, cx, theme, ladder))
        .child(heading("explorer-about-header", "About", None, theme))
        .child(field("Code", code_field, theme))
        .children(sizes.map(|sizes| field("Size", muted(&sizes, theme), theme)))
        .children(params.map(|params| field("Params", params, theme)))
        .child(field("Maintained by", about, theme))
        .children(declares.map(|words| field("Declares", muted(&words, theme), theme)))
        .children(manifest.map(|manifest| {
            field(
                "Min width",
                muted(&format!("{} px", manifest.min_width), theme),
                theme,
            )
        }))
        .children(talks_to.map(|targets| field("Talks to", mono(targets), theme)))
        .children(manifest.map(|manifest| {
            field(
                "Wire",
                mono(manifest.wire_id.clone()).text_color(theme.muted),
                theme,
            )
        }))
        .child(heading(
            "explorer-program-scheduled-header",
            "Scheduled",
            None,
            theme,
        ))
        .children(scheduled)
        .children(nothing_scheduled)
        .children(transactions)
        .into_any_element()
}

/// The capabilities the code's view declares, a line each, or why none are
/// shown: unread yet, refused (with a retry), code the node does not hold,
/// a program that draws nothing, a manifest this app cannot read.
fn capabilities(
    view: &Explorer,
    listed: bool,
    cx: Cx,
    theme: &Theme,
    ladder: &Ladder,
) -> Vec<AnyElement> {
    let blob = match &view.manifest {
        Loadable::Idle | Loadable::Loading(_) => {
            return vec![quiet(
                "explorer-manifest-loading",
                "Reading the code…",
                theme,
            )];
        }
        Loadable::Failed(refusal) => return vec![failed(&refusal.message, cx, theme, ladder)],
        Loadable::Ready(blob) | Loadable::Reloading(blob, _) => blob,
    };
    let Some(blob) = blob else {
        return vec![quiet(
            "explorer-no-code",
            "The node does not hold this code yet.",
            theme,
        )];
    };
    let manifest = match &blob.view {
        None if listed => {
            return vec![quiet(
                "explorer-unread-view",
                "This app cannot read this view's manifest.",
                theme,
            )];
        }
        None => {
            return vec![quiet(
                "explorer-no-view",
                "This program draws nothing: no view, so nothing is asked of the host.",
                theme,
            )];
        }
        Some(CarriedView { manifest: None, .. }) => {
            return vec![quiet(
                "explorer-unread-view",
                "Its view carries a manifest this app cannot read: built for another wire, or asking a capability this app does not have.",
                theme,
            )];
        }
        Some(CarriedView {
            manifest: Some(manifest),
            ..
        }) => manifest,
    };
    if manifest.capabilities.is_empty() {
        return vec![quiet(
            "explorer-no-capabilities",
            "Its view asks nothing of the host.",
            theme,
        )];
    }
    manifest
        .capabilities
        .iter()
        .map(|capability| {
            line(
                SharedString::from(format!("explorer-capability-{}", capability.as_str())).into(),
                theme,
            )
            .child(
                mono(capability.as_str().to_owned())
                    .w(LABEL_W)
                    .flex_shrink_0(),
            )
            .child(div().child(allows(*capability)))
            .into_any_element()
        })
        .collect()
}

fn muted(text: &str, theme: &Theme) -> AnyElement {
    div()
        .text_size(design::text::SECONDARY)
        .text_color(theme.muted)
        .child(text.to_owned())
        .into_any_element()
}

fn chip(id: String, label: &str, theme: &Theme, ladder: &Ladder) -> AnyElement {
    design::badge(
        SharedString::from(id),
        label.to_owned(),
        ladder,
        theme.muted,
        theme.surface,
    )
    .into_any_element()
}

/// An account by its name (`account N` while unread), an agent marked so,
/// the reader marked "(you)".
fn holder(view: &Explorer, number: u64, theme: &Theme, ladder: &Ladder) -> AnyElement {
    let account = view.account(number);
    let name = account.map_or_else(|| format!("account {number}"), |a| a.card.name.clone());
    let you = view.acting.account == Some(number);
    let agent = account.is_some_and(|a| {
        matches!(
            a.kind(),
            identity::Kind::Program {
                category: identity::Category::Agent,
                ..
            }
        )
    });
    div()
        .flex()
        .items_center()
        .gap_1()
        .text_size(design::text::SECONDARY)
        .child(if you { format!("{name} (you)") } else { name })
        .children(agent.then(|| chip(format!("explorer-agent-{number}"), "agent", theme, ladder)))
        .into_any_element()
}

/// Upgrade and Remove on the reader's path to `name`: straight to the
/// programs by grant, proposed by vote, or none. A role's program is never
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
        .is_none_or(|roles| ::programs::role_bound(roles, name));
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

/// One change the programs role will fold in: what it does, to what, when, who
/// made it; how long the vote has to cancel one a grant made; Cancel by
/// grant or Propose cancel by vote.
fn scheduled(
    view: &Explorer,
    index: usize,
    pending: &Pending,
    cx: Cx,
    theme: &Theme,
    ladder: &Ladder,
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
            ladder,
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
fn access(view: &Explorer, network: &Network, cx: Cx, theme: &Theme, ladder: &Ladder) -> Div {
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
        .map(|(index, grant)| granted(view, network, index, grant, cx, theme, ladder))
        .collect();
    let none = rows.is_empty().then(|| {
        quiet(
            "explorer-no-grants",
            "No account maintains a program: only the validators' vote changes the programs role.",
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
        .children(form(view, |edit| *edit == Edit::Grant, cx, theme, ladder))
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
    ladder: &Ladder,
) -> AnyElement {
    let unseated = match &grant.scope {
        Scope::Program(id) if !network.names(id) => Some(muted(
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
        let op = ::programs::Op::Revoke(grant.clone());
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
                .child(holder(view, grant.account, theme, ladder))
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
    ladder: &Ladder,
) -> Option<AnyElement> {
    let form = view.acting.form.as_ref().filter(|form| here(&form.edit))?;
    let network = view.network.ready()?;
    let tip = view.status.ready().map_or(0, |status| status.height);
    let title = match (&form.edit, form.path) {
        (Edit::Upgrade { name, .. }, Path::Grant) => format!("Upgrade {name}"),
        (Edit::Upgrade { name, .. }, Path::Vote) => format!("Propose upgrade · {name}"),
        (Edit::Remove { name, .. }, Path::Grant) => format!("Remove {name}"),
        (Edit::Remove { name, .. }, Path::Vote) => format!("Propose remove · {name}"),
        (Edit::Add, _) => "Add a program".into(),
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
                "program id",
                160.,
            ));
            let kind = |view_kind: bool, label: &str| {
                let pick = cx.listener(move |view: &mut Explorer, _: &ClickEvent, _, cx| {
                    view.add_view(view_kind, cx)
                });
                design::button(
                    SharedString::from(format!("explorer-form-{label}")),
                    label.to_owned(),
                    ladder,
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
                "Program",
                "a program, or empty for every program",
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
                        design::button("explorer-form-submit", submit, ladder, theme, send)
                            .kind(design::Kind::Primary)
                            .enabled(!view.acting.busy),
                    )
                    .child(
                        design::button("explorer-form-close", "Close", ladder, theme, close)
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
