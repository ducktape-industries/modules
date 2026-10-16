//! Expo `src/network/explorer/programs.tsx` (mobile `explorer-detail`
//! b2de83e): the Programs page under the strip. What the programs role
//! will change (Scheduled), runs (Running) and lists (Views), and who
//! changes each name (Access); a program's sheet, and More. Reads the
//! programs role, the accounts, the status and the reader's grants and
//! voices (`acting`); writes a Cancel, a Step down or a revoke (`press`),
//! which sheet is open (`sheet`) and the form a sheet opens (`open_form`).
use super::*;
use crate::Network;
use crate::acting::{self, Edit, Path};
use crate::state::{Sheet, blob_ref};
use crate::ui::programs::{minutes, name_of};
use ::programs::{Change, NOTICE, Pending, Scope};

pub(super) fn programs(view: &Explorer, cx: Cx, k: &Kit) -> AnyElement {
    let network = match &view.network {
        Loadable::Ready(network) | Loadable::Reloading(network, _) => network,
        Loadable::Failed(refusal) => return refused(&refusal.message, cx, k),
        Loadable::Idle | Loadable::Loading(_) => {
            return quiet("explorer-programs-loading", "Reading the programs role…", k);
        }
    };
    let empty =
        network.programs.is_empty() && network.views.is_empty() && network.changes.is_empty();
    let listed = match empty {
        true => vec![quiet(
            "explorer-empty",
            "The programs role of this network runs nothing yet.",
            k,
        )],
        false => [
            scheduled(view, network, cx, k),
            running(view, network, cx, k),
            views(view, network, cx, k),
        ]
        .into_iter()
        .flatten()
        .collect(),
    };
    scroller("explorer-page")
        .children(listed)
        .children(access(view, network, cx, k))
        .into_any_element()
}

/// More on the root's title row while this page shows (Expo
/// `ProgramsMenuHit`): there while the reader may add a program by grant
/// or propose a grant by vote.
pub(super) fn more(view: &Explorer, page: design::Page, cx: Cx, k: &Kit) -> design::Page {
    if !(view.adds() || view.acting.votes()) {
        return page;
    }
    let open = cx.listener(|view: &mut Explorer, _: &ClickEvent, _, cx| {
        view.sheet = Some(Sheet::More);
        cx.notify();
    });
    page.trailing(design::icon_button(
        "explorer-more",
        design::icon("ellipsis", k.ladder.icon),
        "More",
        &k.ladder,
        &k.theme,
        open,
    ))
}

// ---------- the words ----------

/// A code id as the screens show it (Expo `words.ts` `codeWords`):
/// `sha256:1a2b3c4d…9f0e`.
fn code_words(code: &abi::BlobId) -> String {
    let id = blob_ref(code);
    match id.split_once(':') {
        Some((kind, digest)) => format!("{kind}:{}", design::short_hex(digest)),
        None => design::short_hex(&id),
    }
}

/// An account by its name, the reader's marked "(you)" (Expo
/// `holderName`).
fn holder_name(view: &Explorer, number: u64) -> String {
    let name = name_of(view, number);
    match view.acting.account == Some(number) {
        true => format!("{name} (you)"),
        false => name,
    }
}

/// Who changes `name` (Expo `changedBy`): the validators' vote for a
/// role's program, else its maintainers and the every-program ones.
fn changed_by(view: &Explorer, network: &Network, name: &str) -> String {
    let Some(maintainers) = network.maintainers(name) else {
        return "Validators' vote".into();
    };
    let mut named: Vec<String> = maintainers
        .named
        .iter()
        .map(|&number| holder_name(view, number))
        .collect();
    if maintainers.every {
        named.push("every-program maintainers".into());
    }
    match named.is_empty() {
        true => "Validators' vote".into(),
        false => format!("Maintainer {}", named.join(", ")),
    }
}

/// A scheduled change (Expo `words.ts` `scheduledWords`): a set against
/// what runs now, anything else by its verb.
fn scheduled_words(pending: &Pending, network: &Network) -> String {
    let at = grouped(pending.height);
    let Change::Set(entry) = &pending.change else {
        return format!(
            "{} {} at block {at}",
            pending.change.verb(),
            pending.change.program()
        );
    };
    let now = network
        .programs
        .iter()
        .find(|running| running.program == entry.program);
    let params = match now {
        None => "a new program",
        Some(now) if now.params == entry.params => "params unchanged",
        Some(_) => "params changed",
    };
    format!(
        "Set {} to code {} at block {at} · {params}",
        entry.program,
        code_words(&entry.code)
    )
}

/// Whose a scheduled change is (Expo `byWords`): the vote's, or a
/// maintainer's grant and the blocks the validators have left to look.
fn by_words(view: &Explorer, pending: &Pending) -> String {
    let Some(by) = pending.by else {
        return "by vote".into();
    };
    let tip = view.status.ready().map_or(0, |status| status.height);
    format!(
        "by {}'s grant · {} to look",
        name_of(view, by),
        plural(pending.height.saturating_sub(tip), "block", "blocks")
    )
}

// ---------- the sections ----------

/// What the programs role will fold in, under its head with their count.
fn scheduled(view: &Explorer, network: &Network, cx: Cx, k: &Kit) -> Vec<AnyElement> {
    let count = (!network.changes.is_empty()).then(|| aside(network.changes.len().to_string()));
    let head = section("explorer-scheduled-header", "Scheduled", count, k);
    let none = network.changes.is_empty().then(|| {
        quiet(
            "explorer-no-changes",
            "Nothing is scheduled against the programs role.",
            k,
        )
    });
    let rows = network.changes.iter().enumerate().map(|(index, pending)| {
        scheduled_row(view, network, index, pending, k)
            .children(cancel(view, index, pending, cx, k))
            .into_any_element()
    });
    std::iter::once(head.into_any_element())
        .chain(none)
        .chain(rows)
        .collect()
}

/// One change the programs role will fold in: what it does, and whose it
/// is. The program page draws the same row.
pub(super) fn scheduled_row(
    view: &Explorer,
    network: &Network,
    index: usize,
    pending: &Pending,
    k: &Kit,
) -> Stateful<Div> {
    list_row(ElementId::named_usize("explorer-change", index), k)
        .child(design::lead(
            design::icon("clock", k.ladder.icon_row).text_color(k.theme.muted),
        ))
        .child(title_and_caption(
            div().truncate().child(scheduled_words(pending, network)),
            caption(by_words(view, pending), k),
        ))
}

/// Cancel by the grant that made it, or Propose cancel by vote; nothing
/// where the reader has neither. Off while a write is out.
fn cancel(
    view: &Explorer,
    index: usize,
    pending: &Pending,
    cx: Cx,
    k: &Kit,
) -> Option<Stateful<Div>> {
    let path = match pending.by {
        Some(_) if view.maintains(pending.change.program()) => Path::Grant,
        _ => view.acting.votes().then_some(Path::Vote)?,
    };
    let label = match path {
        Path::Grant => "Cancel",
        Path::Vote => "Propose cancel",
    };
    let op = acting::cancel(pending);
    let press = (!view.acting.busy).then(|| -> Press {
        Box::new(
            cx.listener(move |view: &mut Explorer, _: &ClickEvent, _, cx| {
                view.press(path, op.clone(), cx)
            }),
        )
    });
    let id = format!("explorer-cancel-{index}");
    Some(small(id, label, design::Kind::Quiet, press, k))
}

/// Each running program, under its head with their count (Expo's Running).
fn running(view: &Explorer, network: &Network, cx: Cx, k: &Kit) -> Vec<AnyElement> {
    let head = section(
        "explorer-running-header",
        "Running",
        Some(aside(network.programs.len().to_string())),
        k,
    );
    let rows = network.programs.iter().map(|entry| {
        let caption = format!(
            "{} · {}",
            code_words(&entry.code),
            changed_by(view, network, &entry.program)
        );
        entry_row(network, &entry.program, caption, cx, k)
    });
    vec![
        head.into_any_element(),
        column("explorer-programs-list", rows.collect()).into_any_element(),
    ]
}

/// Each view-only entry, under its head; none, no section (Expo's Views).
fn views(view: &Explorer, network: &Network, cx: Cx, k: &Kit) -> Vec<AnyElement> {
    if network.views.is_empty() {
        return Vec::new();
    }
    let head = section(
        "explorer-views-header",
        "Views",
        Some(aside(plural(network.views.len() as u64, "view", "views"))),
        k,
    );
    let rows = network.views.iter().map(|listed| {
        let caption = format!(
            "view only · {} · {}",
            code_words(&listed.view),
            changed_by(view, network, &listed.name)
        );
        entry_row(network, &listed.name, caption, cx, k)
    });
    vec![
        head.into_any_element(),
        column("explorer-views-list", rows.collect()).into_any_element(),
    ]
}

/// A program's or a view's row: its square face, its name in the data
/// face, the caption, a hook badge where it is one; the press opens its
/// sheet.
fn entry_row(
    network: &Network,
    name: &str,
    caption_words: String,
    cx: Cx,
    k: &Kit,
) -> Stateful<Div> {
    let open = {
        let name = name.to_owned();
        cx.listener(move |view: &mut Explorer, _: &ClickEvent, _, cx| {
            view.sheet = Some(Sheet::Program(name.clone()));
            cx.notify();
        })
    };
    let hook = network.hooks.iter().any(|hook| hook == name).then(|| {
        design::badge(
            SharedString::from(format!("explorer-hook-{name}")),
            "hook",
            &k.ladder,
            k.theme.muted,
            k.theme.surface_raised,
        )
    });
    list_row(SharedString::from(format!("explorer-program-{name}")), k)
        .role(Role::Button)
        .aria_label(name.to_owned())
        .focusable()
        .on_click(open)
        .child(design::lead(module_face(name, k.ladder.avatar, k)))
        .child(title_and_caption(
            mono(name.to_owned())
                .truncate()
                .text_type(k.ladder.secondary),
            caption(caption_words, k),
        ))
        .children(hook)
        .child(chevron(k))
}

/// Every grant (Expo's Access), under how long a maintainer's change waits.
fn access(view: &Explorer, network: &Network, cx: Cx, k: &Kit) -> Vec<AnyElement> {
    let block_time = view
        .status
        .ready()
        .map_or(1000, |status| status.block_time_ms);
    let wait = format!(
        "changes by a maintainer wait {NOTICE} blocks (about {} min)",
        minutes(NOTICE, block_time)
    );
    let head = section("explorer-access-header", "Access", Some(aside(wait)), k);
    let none = network.grants.is_empty().then(|| {
        quiet(
            "explorer-no-grants",
            "No account maintains a program: only the validators' vote changes the programs role.",
            k,
        )
    });
    let rows = network
        .grants
        .iter()
        .enumerate()
        .map(|(index, grant)| grant_row(view, network, index, grant, cx, k));
    std::iter::once(head.into_any_element())
        .chain(none)
        .chain(rows)
        .collect()
}

/// One grant: what it covers, who holds it (an agent marked so, a program
/// not run yet said so); the holder steps down, a reader with voices
/// proposes the revoke.
fn grant_row(
    view: &Explorer,
    network: &Network,
    index: usize,
    grant: &::programs::Grant,
    cx: Cx,
    k: &Kit,
) -> AnyElement {
    let agent = view
        .account(grant.account)
        .is_some_and(|account| matches!(account.kind(), identity::Kind::Program { .. }));
    let unseated = matches!(&grant.scope, Scope::Program(id) if !network.names(id));
    let mut about = format!("Maintainer {}", holder_name(view, grant.account));
    if agent {
        about.push_str(" · agent");
    }
    if unseated {
        about.push_str(&format!(
            " · runs nothing yet: {} may add it",
            name_of(view, grant.account)
        ));
    }
    list_row(ElementId::named_usize("explorer-grant", index), k)
        .child(title_and_caption(
            mono(grant.scope.to_string())
                .truncate()
                .text_type(k.ladder.secondary),
            caption(about, k),
        ))
        .children(revoke(view, index, grant, cx, k))
        .into_any_element()
}

/// Step down from one's own grant, or Propose revoke by vote; nothing where
/// the reader has neither. Off while a write is out.
fn revoke(
    view: &Explorer,
    index: usize,
    grant: &::programs::Grant,
    cx: Cx,
    k: &Kit,
) -> Option<Stateful<Div>> {
    let path = match view.acting.account == Some(grant.account) {
        true => Path::Grant,
        false => view.acting.votes().then_some(Path::Vote)?,
    };
    let label = match path {
        Path::Grant => "Step down",
        Path::Vote => "Propose revoke",
    };
    let op = ::programs::Op::Revoke(grant.clone());
    let press = (!view.acting.busy).then(|| -> Press {
        Box::new(
            cx.listener(move |view: &mut Explorer, _: &ClickEvent, _, cx| {
                view.press(path, op.clone(), cx)
            }),
        )
    });
    let id = format!("explorer-revoke-{index}");
    Some(small(id, label, design::Kind::Quiet, press, k))
}

// ---------- the sheets ----------

/// A sheet row's hint at its right (Expo `SheetRow hint`).
fn hint(text: impl Into<SharedString>, k: &Kit) -> Div {
    div()
        .flex_none()
        .text_type(k.ladder.caption)
        .text_color(k.theme.muted)
        .child(text.into())
}

/// The press of a sheet row that leaves the sheet for `route`.
fn leaves_for(route: Route, cx: Cx) -> impl Fn(&ClickEvent, &mut Window, &mut App) + 'static {
    cx.listener(move |view: &mut Explorer, _: &ClickEvent, _, cx| view.go(route.clone(), cx))
}

/// The press of a sheet row that leaves the sheet for a form.
fn opens_form(
    edit: Edit,
    path: Path,
    cx: Cx,
) -> impl Fn(&ClickEvent, &mut Window, &mut App) + 'static {
    cx.listener(move |view: &mut Explorer, _: &ClickEvent, _, cx| {
        view.sheet = None;
        view.open_form(edit.clone(), path, cx)
    })
}

/// What closes a sheet: its scrim, its handle, Back.
fn closes(cx: Cx) -> impl Fn(&(), &mut Window, &mut App) + 'static {
    cx.listener(|view: &mut Explorer, _: &(), _, cx| {
        view.sheet = None;
        cx.notify();
    })
}

/// More (Expo's programs `Sheet` of `SheetRow`s): Add a program by grant,
/// Propose a grant by vote.
pub(super) fn more_sheet(view: &Explorer, cx: Cx, k: &Kit) -> AnyElement {
    let add = view.adds().then(|| {
        let press = opens_form(Edit::Add, Path::Grant, cx);
        design::sheet_row(
            "explorer-add",
            None,
            "Add a program",
            false,
            &k.ladder,
            &k.theme,
            press,
        )
        .child(hint("by your grant", k))
    });
    let grant = view.acting.votes().then(|| {
        let press = opens_form(Edit::Grant, Path::Vote, cx);
        design::sheet_row(
            "explorer-propose-grant",
            None,
            "Propose a grant",
            false,
            &k.ladder,
            &k.theme,
            press,
        )
        .child(hint("by vote", k))
    });
    design::sheet("explorer-more-sheet", "More", &k.ladder, &k.theme)
        .on_dismiss(closes(cx))
        .child(
            div()
                .id("explorer-more-rows")
                .flex()
                .flex_col()
                .children(add)
                .children(grant),
        )
        .into_any_element()
}

/// One program's or view-only entry's sheet (Expo `ProgramSheet`): its
/// face, name and kind; About, its transactions, and the reader's Upgrade
/// and Remove.
pub(super) fn program_sheet(view: &Explorer, name: &str, cx: Cx, k: &Kit) -> Option<AnyElement> {
    let network = view.network.ready()?;
    let listed = view.runs(name).is_none();
    let hook = network.hooks.iter().any(|hook| hook == name);
    let about = design::sheet_row(
        "explorer-program-about",
        None,
        "About",
        false,
        &k.ladder,
        &k.theme,
        leaves_for(Route::Program(name.to_owned()), cx),
    )
    .child(hint("capabilities, code, scheduled", k));
    let transactions = (!listed).then(|| {
        let press = leaves_for(Route::Transactions(Some(name.to_owned())), cx);
        design::sheet_row(
            "explorer-program-txs",
            None,
            "Transactions",
            false,
            &k.ladder,
            &k.theme,
            press,
        )
        .child(hint(format!("to {name}"), k))
    });
    let edits: Vec<Stateful<Div>> = view
        .edits(network, name, listed)
        .into_iter()
        .map(|(edit, path, label)| edit_row(name, edit, path, label, cx, k))
        .collect();
    Some(
        design::sheet(
            "explorer-program-sheet",
            name.to_owned(),
            &k.ladder,
            &k.theme,
        )
        .on_dismiss(closes(cx))
        .child(
            div()
                .id("explorer-program-rows")
                .flex()
                .flex_col()
                .child(sheet_head(name, listed, hook, k))
                .child(about)
                .children(transactions)
                .children(edits),
        )
        .into_any_element(),
    )
}

/// What a program's sheet is about: its square face, its name in the data
/// face, program or view only, and hook.
fn sheet_head(name: &str, listed: bool, hook: bool, k: &Kit) -> Stateful<Div> {
    let kind = match listed {
        true => "view only",
        false => "program",
    };
    let kind = match hook {
        true => format!("{kind} · hook"),
        false => kind.to_owned(),
    };
    design::sheet_head("explorer-program-head", &k.theme).child(
        div()
            .flex()
            .items_center()
            .gap(space::LG)
            .child(module_face(name, k.ladder.avatar_sheet, k))
            .child(
                div()
                    .flex_1()
                    .flex()
                    .flex_col()
                    .child(
                        mono(name.to_owned())
                            .text_type(k.ladder.body)
                            .font_weight(FontWeight::MEDIUM),
                    )
                    .child(caption(kind, k)),
            ),
    )
}

/// Upgrade or Remove on the reader's path, opening its form; Remove in the
/// danger tone.
fn edit_row(
    name: &str,
    edit: Edit,
    path: Path,
    label: &'static str,
    cx: Cx,
    k: &Kit,
) -> Stateful<Div> {
    let (id, danger) = match edit {
        Edit::Remove { .. } => (format!("explorer-remove-{name}"), true),
        _ => (format!("explorer-upgrade-{name}"), false),
    };
    let press = opens_form(edit, path, cx);
    design::sheet_row(
        SharedString::from(id),
        None,
        label,
        danger,
        &k.ladder,
        &k.theme,
        press,
    )
}
