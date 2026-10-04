//! Commits: a virtualized log of the picked ref, read a page at a time as
//! it is scrolled, and one commit's own diff against its first parent.
use ducktape_view_guest::design;
use std::rc::Rc;

use ducktape_view_guest::prelude::*;
use ducktape_view_guest::{Div, Paged, Stateful};

use crate::Forge;
use crate::queries::PAGE;
use crate::ui::components::{badge, button, empty_state, heading, id, quiet, short_hex};
use crate::ui::{diff, fact, refused};
use forge::{CommitInfo, Query};

/// The log of `from`, less what `exclude` reaches.
pub(crate) fn query(
    forge: &Forge,
    from: forge::Revision,
    exclude: Option<forge::Revision>,
) -> Query {
    Query::Log {
        repo: forge.repo_name(),
        from,
        exclude,
        page: PAGE,
    }
}

pub(crate) fn render(forge: &Forge, cx: &mut Context<Forge>, theme: &Theme) -> AnyElement {
    if let Some(oid) = forge.nav().commit.clone() {
        return detail(forge, &oid, cx, theme);
    }
    log(forge, &forge.log_query(), "forge-log", cx, theme)
}

/// The log `query` asks, once its first page has landed; until then, what
/// the screen shows in its place: `reading`, or its refusal with a Retry.
fn landed<'a>(
    forge: &'a Forge,
    query: &Query,
    element_id: &str,
    reading: &str,
    cx: &mut Context<Forge>,
    theme: &Theme,
) -> Result<&'a Entity<Paged<CommitInfo>>, AnyElement> {
    let log = forge.logs.get(query).map(|(log, _)| log);
    let refusal = log.and_then(|log| log.read(|log| log.failed().cloned()));
    match (log, refusal) {
        (_, Some(refusal)) => Err(refused(query, element_id, &refusal, cx, theme)),
        (Some(log), None) if !log.read(Paged::is_loading) => Ok(log),
        _ => Err(crate::ui::reading(element_id, reading, theme)),
    }
}

/// One log, virtualized: the commits read so far and, while the history
/// goes on, a row that reads on. The next page is asked for as that row
/// comes into the list's window.
pub(crate) fn log(
    forge: &Forge,
    query: &Query,
    element_id: &str,
    cx: &mut Context<Forge>,
    theme: &Theme,
) -> AnyElement {
    let log = match landed(forge, query, element_id, READING, cx, theme) {
        Ok(log) => log.clone(),
        Err(state) => return state,
    };
    let (held, count) = log.read(|log| (log.rows().len(), log.count()));
    if held == 0 {
        return empty_state(
            id(format!("{element_id}-empty")),
            "No commits",
            "This ref carries no history yet.",
            theme,
        )
        .into_any_element();
    }
    let theme = *theme;
    let open: crate::ui::diff::Route<String> =
        Rc::new(cx.listener(|forge, oid: &String, _, cx| forge.open_commit(Some(oid.clone()), cx)));
    let handle = forge.log_scroll.clone();
    // one Tab stop: ↑ ↓ walk the commits (the virtual list scrolled to the
    // active one before it claims), Enter opens it
    let list_id: &'static str = match element_id {
        "forge-change-log" => "forge-change-log-list",
        _ => "forge-log-list",
    };
    let (at, _) = crate::ui::components::cursor(forge, list_id);
    let at = at.min(held - 1);
    let pressed = log.clone();
    let list = crate::ui::components::list(
        list_id,
        "Commits",
        held,
        Some(&handle),
        forge,
        cx,
        move |forge, index, _, cx| {
            let oid = pressed.read(|log| log.rows().get(index).map(|commit| commit.oid.clone()));
            forge.open_commit(oid, cx)
        },
    );
    let more = format!("{element_id}-more");
    let rows = crate::ui::components::window(
        element_id,
        count,
        None,
        Some(&handle),
        move |range, _, app| {
            log.update(app, |log, cx| log.show(range.clone(), cx));
            log.read(|log| {
                range
                    .map(|index| match log.rows().get(index) {
                        Some(commit) => row(commit, index == at, &open, &theme),
                        None => reading_on(&more, &theme),
                    })
                    .collect()
            })
        },
    );
    list.child(rows).into_any_element()
}

/// What a log says while a page of it is on its way.
const READING: &str = "Reading the history…";

/// The row past the commits held: the loading line, at a row's height.
fn reading_on(key: &str, theme: &Theme) -> AnyElement {
    div()
        .id(id(key.to_owned()))
        .w_full()
        .flex()
        .items_center()
        .min_h(design::size::CONTROL)
        .px_2()
        .child(quiet(READING, theme))
        .into_any_element()
}

/// One commit of a log: its short id, summary, author and date.
fn row(
    commit: &CommitInfo,
    active: bool,
    open: &crate::ui::diff::Route<String>,
    theme: &Theme,
) -> AnyElement {
    let oid = commit.oid.clone();
    let open = open.clone();
    let clicked = oid.clone();
    let mut row = crate::ui::components::row(format!("forge-commit-{oid}"), theme)
        .active(active.then_some(0))
        .on_click(move |_: &ClickEvent, window: &mut Window, app: &mut App| {
            open(&clicked, window, app)
        })
        .cell(
            div()
                .w(OID_W)
                .whitespace_nowrap()
                .font_family(design::fonts::FAMILY_MONO)
                .text_size(design::text::SECONDARY)
                .text_color(theme.muted)
                .child(short_hex(&oid)),
        )
        .cell(div().flex_1().truncate().child(summary(commit)))
        .cell(quiet(
            String::from_utf8_lossy(&commit.author.name).into_owned(),
            theme,
        ))
        .cell(quiet(
            design::date(
                u64::try_from(commit.author.time)
                    .unwrap_or(0)
                    .saturating_mul(1000),
            ),
            theme,
        ));
    if commit.parents.len() > 1 {
        row = row.cell(badge(
            id(format!("forge-commit-merge-{oid}")),
            format!("{} parents", commit.parents.len()),
            theme.accent_foreground,
            theme.accent_soft,
        ));
    }
    row.into_any_element()
}

/// A commit's ids, parents, author and message.
fn facts(commit: &CommitInfo, theme: &Theme) -> Stateful<Div> {
    let parents = if commit.parents.is_empty() {
        "root commit".to_owned()
    } else {
        commit
            .parents
            .iter()
            .map(|parent| short_hex(parent))
            .collect::<Vec<_>>()
            .join(", ")
    };
    let author = format!(
        "{} <{}> at {}",
        String::from_utf8_lossy(&commit.author.name),
        String::from_utf8_lossy(&commit.author.email),
        commit.author.time
    );
    div()
        .id(id("forge-commit-facts"))
        .flex()
        .flex_col()
        .gap_1()
        .px_3()
        .py_2()
        .child(fact("Commit", commit.oid.clone(), theme))
        .child(fact("Tree", commit.tree.clone(), theme))
        .child(fact("Parents", parents, theme))
        .child(fact("Author", author, theme))
        .child(quiet(
            String::from_utf8_lossy(&commit.message).into_owned(),
            theme,
        ))
}

/// The short commit id column.
const OID_W: Pixels = px(100.);

fn summary(commit: &CommitInfo) -> String {
    String::from_utf8_lossy(&commit.message)
        .lines()
        .next()
        .unwrap_or_default()
        .to_owned()
}

fn detail(forge: &Forge, oid: &str, cx: &mut Context<Forge>, theme: &Theme) -> AnyElement {
    let close = cx.listener(|forge, _: &ClickEvent, _, cx| forge.open_commit(None, cx));
    let commit = forge.commit(oid);
    let mut column = div()
        .id(id("forge-commit-detail"))
        .flex()
        .flex_col()
        .flex_1()
        .min_h(px(0.))
        .child(
            div()
                .id(id("forge-commit-header"))
                .flex()
                .items_center()
                .gap_2()
                .px_3()
                .py_2()
                .border_b_1()
                .border_color(theme.border)
                .child(heading(
                    id("forge-commit-title"),
                    commit.as_ref().map_or_else(|| short_hex(oid), summary),
                    2,
                    theme,
                ))
                .child(div().flex_1())
                .child(button(id("forge-commit-close"), "Back", theme, close)),
        );
    if let Some(commit) = &commit {
        column = column.child(facts(commit, theme));
    }
    let diff = match forge.commit_diff_query(oid) {
        Some(query) => diff::render(forge, &query, "forge-commit-diff", false, cx, theme),
        // the diff is against the commit's parent: the commit comes first,
        // by its own row where the log on screen does not hold it
        None => {
            let own = forge.commit_query(oid);
            landed(
                forge,
                &own,
                "forge-commit-diff",
                "Reading the diff…",
                cx,
                theme,
            )
            .err()
            .unwrap_or_else(|| div().into_any_element())
        }
    };
    column.child(diff).into_any_element()
}
