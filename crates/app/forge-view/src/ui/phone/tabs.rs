//! A repository's tab bodies on the phone (Expo `src/forge/repo.tsx`):
//! README, Code, Commits, Changes, Refs and Settings, each a list in the
//! kit's rows under the strip [`super::repo`] draws.
//!
//! Reads the browsed ref's tree, README, history, the changes under the
//! picked filter, the refs and their comparisons, and the repository's
//! settings and writers; writes the settings (`configure`), revokes and
//! grants, and opens a change from a ref (`start_change`).
use std::rc::Rc;

use view::design::{Kind, Ladder, TextType};
use view::prelude::*;
use view::{Div, Paged, Stateful};

use super::document;
use super::parts::{self, Chip, Row, Tone};
use crate::Forge;
use crate::state::{Filter, Menu, RepoTab, Sheet};
use crate::tree::{self, Slot};
use crate::ui::components::{self, id, ref_label};
use forge::{
    BlobView, ChangeSummary, CommitInfo, Content, Judgment, Mergeability, Query, Reply, Revision,
};

/// The open tab's body.
pub(super) fn render(
    forge: &Forge,
    cx: &mut Context<Forge>,
    ladder: &Ladder,
    theme: &Theme,
) -> AnyElement {
    match forge.nav().tab {
        RepoTab::Readme => readme(forge, cx, ladder, theme),
        RepoTab::Code => code(forge, cx, ladder, theme),
        RepoTab::Commits => commits(forge, cx, ladder, theme),
        RepoTab::Changes => changes(forge, cx, ladder, theme),
        RepoTab::Refs => refs(forge, cx, ladder, theme),
        RepoTab::Settings => settings(forge, cx, ladder, theme),
    }
}

/// A tab's column, the rest of the page's height.
fn body(key: &str) -> Stateful<Div> {
    div()
        .id(id(key.to_owned()))
        .flex_1()
        .min_h(px(0.))
        .flex()
        .flex_col()
}

/// A ref with nothing under it (`NoCommits`): how to push to it.
fn no_commits(forge: &Forge, ladder: &Ladder, theme: &Theme) -> AnyElement {
    let address = crate::ui::repo_link(forge, &forge.repo_name());
    parts::empty_state(
        id("forge-tree-no-commits"),
        "git-commit-horizontal",
        "No commits yet",
        format!("Push code to this ref to browse it here: git push {address} main"),
        ladder,
        theme,
    )
    .into_any_element()
}

/// What stands in for a tab that reads the browsed ref's tree before the
/// ref resolves: the unborn ref's push hint once the refs are read, else
/// the quiet line.
fn unresolved(forge: &Forge, ladder: &Ladder, theme: &Theme) -> Option<AnyElement> {
    if forge.head_oid().is_some() {
        return None;
    }
    Some(match forge.refs() {
        Some(_) => no_commits(forge, ladder, theme),
        None => parts::quiet(
            id("forge-ref-resolving"),
            "Resolving the ref…",
            ladder,
            theme,
        )
        .into_any_element(),
    })
}

// ------------------------------------------------------------------ README

/// The root README rendered (`ReadmeTab`): `README.md` over any other
/// `README*`, under the file bar saying the ref and its size.
fn readme(forge: &Forge, cx: &mut Context<Forge>, ladder: &Ladder, theme: &Theme) -> AnyElement {
    let page = body("forge-readme").overflow_y_scroll();
    if let Some(state) = unresolved(forge, ladder, theme) {
        return page.child(state).into_any_element();
    }
    let Some(root) = forge.tree_query(Vec::new()) else {
        return page.into_any_element();
    };
    let tree = parts::stage(
        forge,
        &root,
        "forge-readme-tree",
        "Reading the tree…",
        cx,
        (ladder, theme),
    );
    if let Err(state) = tree {
        return page.child(state).into_any_element();
    }
    let Some((name, oid)) = forge.readme() else {
        let ref_ = ref_label(&forge.head_name());
        return page
            .child(parts::empty_state(
                id("forge-readme-none"),
                "file-text",
                "No README",
                format!("Add a README.md at the root of {ref_}."),
                ladder,
                theme,
            ))
            .into_any_element();
    };
    page.children(readme_document(forge, &name, &oid, cx, ladder, theme))
        .into_any_element()
}

/// The README's file bar over its document, or over the read in its
/// place.
fn readme_document(
    forge: &Forge,
    name: &[u8],
    oid: &str,
    cx: &mut Context<Forge>,
    ladder: &Ladder,
    theme: &Theme,
) -> [AnyElement; 2] {
    let ref_ = ref_label(&forge.head_name());
    let query = Query::Blob {
        repo: forge.repo_name(),
        oid: oid.to_owned(),
        range: None,
    };
    let blob = match parts::stage(
        forge,
        &query,
        "forge-readme-blob",
        "Reading the README…",
        cx,
        (ladder, theme),
    ) {
        Ok(Reply::Blob { blob, .. }) => Ok(blob),
        Ok(_) => Err(div().into_any_element()),
        Err(state) => Err(state),
    };
    let right = match &blob {
        Ok(blob) => format!("{ref_} · {}", parts::size_of(blob.size)),
        Err(_) => ref_,
    };
    let bar = parts::file_bar(
        "forge-readme-title",
        components::path_text(name),
        right,
        ladder,
        theme,
    );
    let content = match blob {
        Ok(blob) => readme_body(forge, oid, blob, cx, ladder, theme),
        Err(state) => state,
    };
    [
        bar.into_any_element(),
        div()
            .p(design::space::BLOCK)
            .child(content)
            .into_any_element(),
    ]
}

fn readme_body(
    forge: &Forge,
    oid: &str,
    blob: &BlobView,
    cx: &mut Context<Forge>,
    ladder: &Ladder,
    theme: &Theme,
) -> AnyElement {
    if blob.content != Content::Text {
        return parts::quiet(
            id("forge-readme-binary"),
            "This README is not text.",
            ladder,
            theme,
        )
        .p_0()
        .into_any_element();
    }
    let blocks = forge.blob_cache.doc(oid, &blob.bytes);
    let links = crate::ui::code::links(Vec::new(), cx);
    document::markdown("forge-readme-body", &blocks, &links, ladder, theme)
}

// ------------------------------------------------------------------ Code

/// The root folder (`FolderBody`): the file bar, "Filter files", then the
/// folders (`name/`, a chevron) and the files (mono) the filter keeps; a
/// folder opens in place under itself, a file on its own page.
fn code(forge: &Forge, cx: &mut Context<Forge>, ladder: &Ladder, theme: &Theme) -> AnyElement {
    let bar = parts::file_bar(
        "forge-code-bar",
        forge.repo_name(),
        ref_label(&forge.head_name()),
        ladder,
        theme,
    );
    let filter = Input::new(id("forge-tree-search"), &forge.tree_search, "Filter files")
        .autocorrect(false)
        .autocapitalize(Autocapitalize::None)
        .virtual_keyboard(VirtualKeyboard::Search)
        .placeholder("Filter files");
    let page = body("forge-code").child(bar).child(
        div()
            .p(design::space::BLOCK)
            .pb(design::space::SM)
            .child(parts::search_field(filter, "search", ladder, theme)),
    );
    if let Some(state) = unresolved(forge, ladder, theme) {
        return page.child(state).into_any_element();
    }
    let Some(root) = forge.tree_query(Vec::new()) else {
        return page.into_any_element();
    };
    if let Err(state) = parts::stage(
        forge,
        &root,
        "forge-tree",
        "Reading the tree…",
        cx,
        (ladder, theme),
    ) {
        return page.child(state).into_any_element();
    }
    let rows = forge.tree_rows();
    if rows.is_empty() {
        return page
            .child(parts::empty_state(
                id("forge-tree-empty"),
                "folder-open",
                "Nothing here",
                "This tree holds no file the filter keeps.",
                ladder,
                theme,
            ))
            .into_any_element();
    }
    page.child(tree_list(forge, rows, cx, ladder, theme))
        .into_any_element()
}

/// Opens a folder in place, or a file on its own page.
fn press_entry(forge: &mut Forge, row: &tree::Row, cx: &mut Context<Forge>) {
    match &row.slot {
        Slot::Entry { .. } if row.is_dir() => forge.toggle_dir(row.path.clone(), cx),
        Slot::Entry { oid, .. } => forge.open_file(row.path.clone(), oid.clone(), cx),
        Slot::Loading | Slot::Failed(_) => {}
    }
}

fn tree_list(
    forge: &Forge,
    rows: Vec<tree::Row>,
    cx: &mut Context<Forge>,
    ladder: &Ladder,
    theme: &Theme,
) -> Stateful<Div> {
    let rows = Rc::new(rows);
    let count = rows.len();
    let pressed = rows.clone();
    let press: parts::Route<usize> = Rc::new(
        cx.processor(move |forge, index: usize, _, cx| press_entry(forge, &pressed[index], cx)),
    );
    let opened = rows.clone();
    let (ladder, theme) = (*ladder, *theme);
    let paint = move |index: usize, active: bool| {
        let press = press.clone();
        tree_row(&rows[index], index, active, press, &ladder, &theme)
    };
    parts::virtual_list(
        ("forge-tree-list", "Files"),
        count,
        &forge.tree_scroll,
        move |forge, index, _, cx| press_entry(forge, &opened[index], cx),
        paint,
        forge,
        cx,
    )
}

/// One entry, in from the side by its depth: a folder `name/` with a
/// chevron, a file mono; a folder still read, or refused, says so.
fn tree_row(
    row: &tree::Row,
    index: usize,
    active: bool,
    press: parts::Route<usize>,
    ladder: &Ladder,
    theme: &Theme,
) -> AnyElement {
    let key = id(format!("forge-tree-{}", components::path_text(&row.path)));
    let line = match &row.slot {
        Slot::Loading => return parts::quiet(key, "Reading…", ladder, theme).into_any_element(),
        Slot::Failed(sentence) => {
            return parts::notice(key, sentence.clone(), Tone::Danger, ladder, theme)
                .into_any_element();
        }
        Slot::Entry { .. } if row.is_dir() => Row::titled(key, format!("{}/", row.name), ladder)
            .lead(parts::lead_icon("folder", ladder, theme))
            .chevron(),
        Slot::Entry { .. } => Row::data(key, row.name.clone(), ladder).lead(parts::lead_icon(
            "file-text",
            ladder,
            theme,
        )),
    };
    let inset = design::space::BLOCK + design::space::LG * row.depth as f32;
    let line = line.draw(ladder, theme).pl(inset).on_click(
        move |_: &ClickEvent, window: &mut Window, app: &mut App| press(index, window, app),
    );
    parts::option(line, active).into_any_element()
}

// ------------------------------------------------------------------ Commits

/// The browsed ref's history (`CommitsTab`).
fn commits(forge: &Forge, cx: &mut Context<Forge>, ladder: &Ladder, theme: &Theme) -> AnyElement {
    if let Some(state) = unresolved(forge, ladder, theme) {
        return body("forge-commits").child(state).into_any_element();
    }
    log(forge, &forge.log_query(), "forge-log", cx, ladder, theme)
}

/// What a log says while a page of it is on its way.
const READING: &str = "Reading the history…";

/// A history as a list (`Log`): the commits read so far and, while it
/// goes on, the quiet line, which asks for the next page as it comes into
/// view; a commit opens on its own page.
pub(super) fn log(
    forge: &Forge,
    query: &Query,
    element_id: &str,
    cx: &mut Context<Forge>,
    ladder: &Ladder,
    theme: &Theme,
) -> AnyElement {
    let log = match landed(forge, query, element_id, cx, ladder, theme) {
        Ok(log) => log.clone(),
        Err(state) => return state,
    };
    let (held, count) = log.read(|log| (log.rows().len(), log.count()));
    if held == 0 {
        return parts::empty_state(
            id(format!("{element_id}-empty")),
            "git-commit-horizontal",
            "No commits",
            "This ref carries no history yet.",
            ladder,
            theme,
        )
        .into_any_element();
    }
    let list_id: &'static str = match element_id {
        "forge-change-log" => "forge-change-log-list",
        _ => "forge-log-list",
    };
    let pressed = log.clone();
    let list = components::list(
        list_id,
        "Commits",
        held,
        Some(&forge.log_scroll),
        forge,
        cx,
        move |forge, index, _, cx| {
            let oid = pressed.read(|log| log.rows().get(index).map(|commit| commit.oid.clone()));
            forge.open_commit(oid, cx)
        },
    );
    let paint = log_rows(log, list_id, forge, cx, ladder, theme);
    list.child(components::window(
        element_id,
        count,
        None,
        Some(&forge.log_scroll),
        paint,
    ))
    .into_any_element()
}

/// The log `query` asks once its first page landed, else what shows in
/// its place: the quiet line, or the refusal and Retry.
fn landed<'a>(
    forge: &'a Forge,
    query: &Query,
    element_id: &str,
    cx: &mut Context<Forge>,
    ladder: &Ladder,
    theme: &Theme,
) -> Result<&'a Entity<Paged<CommitInfo>>, AnyElement> {
    let log = forge.logs.get(query).map(|(log, _)| log);
    let refusal = log.and_then(|log| log.read(|log| log.failed().cloned()));
    match (log, refusal) {
        (_, Some(refusal)) => {
            let query = query.clone();
            let retry =
                cx.listener(move |forge, _: &ClickEvent, _, cx| forge.retry(query.clone(), cx));
            Err(parts::refused(
                element_id,
                refusal.message,
                retry,
                ladder,
                theme,
            ))
        }
        (Some(log), None) if !log.read(Paged::is_loading) => Ok(log),
        _ => Err(
            parts::quiet(id(format!("{element_id}-loading")), READING, ladder, theme)
                .into_any_element(),
        ),
    }
}

/// Paints the rows of a log's window: a commit row each, and past the
/// commits held, the quiet line that reads on.
fn log_rows(
    log: Entity<Paged<CommitInfo>>,
    list_id: &'static str,
    forge: &Forge,
    cx: &mut Context<Forge>,
    ladder: &Ladder,
    theme: &Theme,
) -> impl Fn(std::ops::Range<usize>, &mut Window, &mut App) -> Vec<AnyElement> + 'static {
    let (at, _) = components::cursor(forge, list_id);
    let walked = components::walked(forge, list_id);
    let open: parts::Route<String> =
        Rc::new(cx.processor(|forge, oid: String, _, cx| forge.open_commit(Some(oid), cx)));
    let (ladder, theme) = (*ladder, *theme);
    let more = format!("{list_id}-more");
    move |range, _, app| {
        log.update(app, |log, cx| log.show(range.clone(), cx));
        log.read(|log| {
            range
                .map(|index| match log.rows().get(index) {
                    Some(commit) => {
                        let open = open.clone();
                        let oid = commit.oid.clone();
                        let row = parts::commit_row(commit, &ladder, &theme)
                            .draw(&ladder, &theme)
                            .on_click(move |_: &ClickEvent, window: &mut Window, app: &mut App| {
                                open(oid.clone(), window, app)
                            });
                        parts::option(row, walked && index == at).into_any_element()
                    }
                    None => parts::quiet(id(more.clone()), READING, &ladder, &theme)
                        .min_h(px(design::height::touch::ROW_TALL as f32))
                        .into_any_element(),
                })
                .collect()
        })
    }
}

// ------------------------------------------------------------------ Changes

/// The filters in the Expo app's order (`query.ts` `FILTERS`).
const FILTERS: [Filter; 6] = [
    Filter::Open,
    Filter::Merged,
    Filter::Closed,
    Filter::Judgment,
    Filter::Authored,
    Filter::Involves,
];

/// The changes under the picked filter, their titles filtered on the
/// phone (`ChangesTab`).
fn changes(forge: &Forge, cx: &mut Context<Forge>, ladder: &Ladder, theme: &Theme) -> AnyElement {
    let search = Input::new(
        id("forge-changes-search"),
        &forge.change_search,
        "Search titles",
    )
    .autocorrect(false)
    .autocapitalize(Autocapitalize::None)
    .virtual_keyboard(VirtualKeyboard::Search)
    .placeholder("Search titles");
    let page = body("forge-changes")
        .child(filter_chips(forge, cx, ladder, theme))
        .child(
            div()
                .px(design::space::BLOCK)
                .pb(design::space::SM)
                .child(parts::search_field(search, "search", ladder, theme)),
        )
        .children(parts::pending(forge, "changes", ladder, theme));
    let repo = forge.repo_name();
    // a filter about "me" asks nothing while the reader holds no account
    let Some(query) = forge.changes_query(&repo) else {
        return page
            .child(parts::empty_state(
                id("forge-changes-no-account"),
                "user-round",
                "No account",
                "Create an account to see the changes that involve you.",
                ladder,
                theme,
            ))
            .into_any_element();
    };
    let listed = match parts::stage(
        forge,
        &query,
        "forge-changes-list",
        "Reading the changes…",
        cx,
        (ladder, theme),
    ) {
        Ok(reply) => listed(reply, &repo),
        Err(state) => return page.child(state).into_any_element(),
    };
    let needle = forge.change_search.text().trim().to_lowercase();
    let shown: Vec<(&ChangeSummary, Option<&Judgment>)> = listed
        .into_iter()
        .filter(|(summary, _)| needle.is_empty() || summary.title.to_lowercase().contains(&needle))
        .collect();
    if shown.is_empty() {
        return page
            .child(no_changes(forge.filter, ladder, theme))
            .into_any_element();
    }
    page.child(change_list(&shown, forge, cx, ladder, theme))
        .into_any_element()
}

/// The filter chips; Open carries how many are open.
fn filter_chips(
    forge: &Forge,
    cx: &mut Context<Forge>,
    ladder: &Ladder,
    theme: &Theme,
) -> Stateful<Div> {
    let open = forge.open_changes().map(|count| count as usize);
    let chips = FILTERS
        .iter()
        .map(|filter| Chip {
            id: id(format!("forge-filter-{}", filter.slug())),
            label: filter.label(),
            count: (*filter == Filter::Open).then_some(open).flatten(),
        })
        .collect();
    let chosen = FILTERS
        .iter()
        .position(|filter| *filter == forge.filter)
        .unwrap_or(0);
    let pick = cx.processor(|forge, index: usize, _, cx| forge.set_filter(FILTERS[index], cx));
    parts::chips("forge-filters", chips, chosen, ladder, theme, pick)
}

/// The changes a reply lists here: a change list as it is, the judgment
/// read narrowed to this repository.
fn listed<'a>(reply: &'a Reply, repo: &str) -> Vec<(&'a ChangeSummary, Option<&'a Judgment>)> {
    match reply {
        Reply::Changes { page, .. } => page.items.iter().map(|summary| (summary, None)).collect(),
        Reply::Judgment { page, .. } => page
            .items
            .iter()
            .filter(|item| item.change.repo == repo)
            .map(|item| (&item.change, Some(item)))
            .collect(),
        _ => Vec::new(),
    }
}

fn no_changes(filter: Filter, ladder: &Ladder, theme: &Theme) -> AnyElement {
    let (title, said) = match filter {
        Filter::Judgment => (
            "Nothing waits on you",
            "No review is requested of you and no thread of yours has an answer.",
        ),
        _ => (
            "No changes here",
            "Compare a branch from the Refs tab to open one.",
        ),
    };
    parts::empty_state(
        id("forge-changes-empty"),
        "git-pull-request",
        title,
        said,
        ladder,
        theme,
    )
    .into_any_element()
}

fn change_list(
    shown: &[(&ChangeSummary, Option<&Judgment>)],
    forge: &Forge,
    cx: &mut Context<Forge>,
    ladder: &Ladder,
    theme: &Theme,
) -> Stateful<Div> {
    let numbers: Vec<u64> = shown.iter().map(|(summary, _)| summary.n).collect();
    let rows = shown
        .iter()
        .map(|(summary, judgment)| {
            let n = summary.n;
            let open: parts::Press = Box::new(
                cx.listener(move |forge, _: &ClickEvent, _, cx| forge.open_change(Some(n), cx)),
            );
            (
                parts::change_row(summary, *judgment, false, forge, ladder, theme),
                open,
            )
        })
        .collect();
    parts::rows_list(
        "forge-change-list",
        "Changes",
        rows,
        move |forge, index, _, cx| forge.open_change(Some(numbers[index]), cx),
        forge,
        cx,
        (ladder, theme),
    )
    .flex_1()
    .min_h(px(0.))
    .overflow_y_scroll()
}

// ------------------------------------------------------------------ Refs

/// Branches | Tags; each branch's standing against the browsed ref, and
/// Compare to open a change from it (`RefsTab`).
fn refs(forge: &Forge, cx: &mut Context<Forge>, ladder: &Ladder, theme: &Theme) -> AnyElement {
    let all = forge.refs().unwrap_or_default();
    let count = |prefix: &[u8]| {
        all.iter()
            .filter(|info| info.name.starts_with(prefix))
            .count()
    };
    let chips = vec![
        Chip {
            id: id("forge-refs-branches"),
            label: "Branches",
            count: Some(count(b"refs/heads/")),
        },
        Chip {
            id: id("forge-refs-tags"),
            label: "Tags",
            count: Some(count(b"refs/tags/")),
        },
    ];
    let pick = cx.processor(|forge, index: usize, _, cx| {
        forge.refs_tags = index == 1;
        cx.notify();
    });
    let page = body("forge-refs")
        .child(parts::chips(
            "forge-refs-kind",
            chips,
            usize::from(forge.refs_tags),
            ladder,
            theme,
            pick,
        ))
        .children(parts::pending(forge, "changes", ladder, theme))
        .children(forbidden(forge).map(|said| {
            parts::quiet(id("forge-refs-forbidden"), said, ladder, theme).into_any_element()
        }));
    if let Err(state) = parts::stage(
        forge,
        &forge.refs_query(),
        "forge-refs-list",
        "Reading refs…",
        cx,
        (ladder, theme),
    ) {
        return page.child(state).into_any_element();
    }
    if all.is_empty() {
        return page
            .child(parts::empty_state(
                id("forge-refs-empty"),
                "git-branch",
                "No refs",
                "This repository is unborn: nothing has been pushed to it yet.",
                ladder,
                theme,
            ))
            .into_any_element();
    }
    let prefix: &[u8] = if forge.refs_tags {
        b"refs/tags/"
    } else {
        b"refs/heads/"
    };
    let names: Vec<Vec<u8>> = all
        .iter()
        .filter(|info| info.name.starts_with(prefix))
        .map(|info| info.name.clone())
        .collect();
    page.child(ref_list(forge, names, cx, ladder, theme))
        .into_any_element()
}

/// What this repository's settings forbid, if anything.
fn forbidden(forge: &Forge) -> Option<String> {
    let (info, _, _) = forge.repo()?;
    let what = match (
        info.repo.settings.allow_force,
        info.repo.settings.allow_delete,
    ) {
        (false, false) => "force pushes and ref deletions",
        (false, true) => "force pushes",
        (true, false) => "ref deletions",
        (true, true) => return None,
    };
    Some(format!("This repository forbids {what}."))
}

fn ref_list(
    forge: &Forge,
    names: Vec<Vec<u8>>,
    cx: &mut Context<Forge>,
    ladder: &Ladder,
    theme: &Theme,
) -> Stateful<Div> {
    let lines: Rc<Vec<RefLine>> =
        Rc::new(names.iter().map(|name| RefLine::of(forge, name)).collect());
    let routes = RefRoutes {
        browse: Rc::new(cx.processor(|forge, name: Vec<u8>, _, cx| forge.pick_ref(name, cx))),
        compare: Rc::new(
            cx.processor(|forge, name: Vec<u8>, window, cx| forge.start_change(name, window, cx)),
        ),
        may_compare: forge.writes_repo(),
    };
    let (ladder, theme) = (*ladder, *theme);
    let painted = lines.clone();
    let paint = move |index: usize, active: bool| {
        ref_row(&painted[index], &routes, active, &ladder, &theme)
    };
    parts::virtual_list(
        ("forge-refs-list", "Refs"),
        lines.len(),
        &forge.refs_scroll,
        move |forge, index, _, cx| forge.pick_ref(lines[index].name.clone(), cx),
        paint,
        forge,
        cx,
    )
}

/// What one ref's row says, read once a render.
struct RefLine {
    name: Vec<u8>,
    label: String,
    /// `<short> · <standing>`
    facts: String,
    browsing: bool,
    default: bool,
    branch: bool,
}

impl RefLine {
    fn of(forge: &Forge, name: &[u8]) -> RefLine {
        let target = forge
            .refs()
            .and_then(|refs| refs.iter().find(|info| info.name == name))
            .map(|info| components::short_hex(&info.target));
        let facts = target
            .into_iter()
            .chain(standing(forge, name))
            .collect::<Vec<_>>()
            .join(" · ");
        RefLine {
            name: name.to_vec(),
            label: ref_label(name),
            facts,
            browsing: name == forge.head_name(),
            default: name == forge.default_head(),
            branch: name.starts_with(b"refs/heads/"),
        }
    }
}

/// What a ref's row does: browse it, or Compare it into a new change
/// (offered to a writer).
struct RefRoutes {
    browse: parts::Route<Vec<u8>>,
    compare: parts::Route<Vec<u8>>,
    may_compare: bool,
}

/// One ref (`RefRow`): its name, `<short> · <standing>`, picked while it
/// is browsed; the default head wears `default`, another branch offers
/// Compare.
fn ref_row(
    line: &RefLine,
    routes: &RefRoutes,
    active: bool,
    ladder: &Ladder,
    theme: &Theme,
) -> AnyElement {
    let label = &line.label;
    let mut row = Row::titled(id(format!("forge-ref-row-{label}")), label.clone(), ladder)
        .lead(parts::lead_icon("git-branch", ladder, theme))
        .caption(parts::caption(line.facts.clone(), ladder, theme))
        .chosen(line.browsing);
    if line.default {
        row = row.tail(design::badge(
            id(format!("forge-ref-default-{label}")),
            "default",
            ladder,
            theme.muted,
            theme.surface_raised,
        ));
    } else if line.branch {
        let (compare, from) = (routes.compare.clone(), line.name.clone());
        let start = move |_: &ClickEvent, window: &mut Window, app: &mut App| {
            compare(from.clone(), window, app)
        };
        row = row.tail(
            parts::small_button(
                id(format!("forge-compare-{label}")),
                "Compare",
                ladder,
                theme,
                start,
            )
            .enabled(routes.may_compare),
        );
    }
    let (browse, name) = (routes.browse.clone(), line.name.clone());
    let row = row.draw(ladder, theme).on_click(
        move |_: &ClickEvent, window: &mut Window, app: &mut App| browse(name.clone(), window, app),
    );
    parts::option(row, active).into_any_element()
}

/// How a branch stands against the browsed ref: `browsing` for that ref
/// itself, else `N ahead · M behind · <word>` once compared.
fn standing(forge: &Forge, name: &[u8]) -> Option<String> {
    let head = forge.head_name();
    if name == head {
        return Some("browsing".to_owned());
    }
    if !name.starts_with(b"refs/heads/") {
        return None;
    }
    let query = Query::Compare {
        repo: forge.repo_name(),
        from: Revision::Ref(name.to_vec()),
        into: Revision::Ref(head),
    };
    Some(match forge.ready(&query) {
        Some(Reply::Compare { comparison, .. }) => {
            let word = match comparison.mergeability {
                Mergeability::UpToDate => "merged",
                Mergeability::FastForward => "fast-forward",
                Mergeability::Diverged => "diverged",
                Mergeability::Unrelated => "unrelated",
            };
            format!(
                "{} ahead · {} behind · {word}",
                comparison.ahead, comparison.behind
            )
        }
        _ => "comparing…".to_owned(),
    })
}

// ------------------------------------------------------------------ Settings

/// The repository's settings and writers (`SettingsTab`): what a switch
/// changes is saved at once; everything is read-only for a non-owner.
fn settings(forge: &Forge, cx: &mut Context<Forge>, ladder: &Ladder, theme: &Theme) -> AnyElement {
    let page = body("forge-settings")
        .overflow_y_scroll()
        .children(parts::pending(forge, "settings", ladder, theme));
    let reply = parts::stage(
        forge,
        &forge.repo_query(),
        "forge-settings-repo",
        "Reading this repository…",
        cx,
        (ladder, theme),
    );
    let (repo, writers) = match reply {
        Ok(Reply::Repo { repo, writers, .. }) => (repo, writers),
        Ok(_) => return page.into_any_element(),
        Err(state) => return page.child(state).into_any_element(),
    };
    let owner = forge.owns_repo();
    let section = |key: &str, label: &str| {
        design::section_head(id(key.to_owned()), label.to_owned(), None, ladder, theme)
    };
    page.child(section("forge-settings-refs", "Refs"))
        .child(default_head(
            forge,
            &repo.repo.settings.head,
            owner,
            cx,
            ladder,
            theme,
        ))
        .child(section("forge-settings-access", "Access"))
        .child(
            Row::titled(id("forge-writer-owner"), "Owner", ladder)
                .tail(
                    div()
                        .text_type(ladder.secondary)
                        .child(forge.principal_name(&repo.repo.owner)),
                )
                .draw(ladder, theme),
        )
        .children(switches(
            forge,
            &repo.repo.settings,
            owner,
            cx,
            ladder,
            theme,
        ))
        .child(section("forge-settings-writers", "Can write"))
        .children(writer_rows(forge, &writers.items, owner, cx, ladder, theme))
        .into_any_element()
}

/// "Default head": the ref, and for the owner the sheet that picks
/// another.
fn default_head(
    forge: &Forge,
    head: &[u8],
    owner: bool,
    cx: &mut Context<Forge>,
    ladder: &Ladder,
    theme: &Theme,
) -> AnyElement {
    let mut row = Row::titled(id(Menu::Head.key()), "Default head", ladder)
        .caption(parts::caption(
            "What a clone checks out and where changes point by default.",
            ladder,
            theme,
        ))
        .tail(parts::data(ref_label(head)).text_type(ladder.secondary));
    if owner {
        row = row.chevron();
    }
    let row = row.draw(ladder, theme);
    match owner && forge.pending_in("settings").is_empty() {
        true => row
            .role(Role::Button)
            .focusable()
            .on_click(cx.listener(|forge, event: &ClickEvent, window, cx| {
                forge.open_dropdown(Menu::Head, event, window, cx)
            }))
            .into_any_element(),
        false => row.into_any_element(),
    }
}

/// The force and delete switches: a flip saves at once.
fn switches(
    forge: &Forge,
    settings: &forge::Settings,
    owner: bool,
    cx: &mut Context<Forge>,
    ladder: &Ladder,
    theme: &Theme,
) -> [AnyElement; 2] {
    let may = owner && forge.pending_in("settings").is_empty();
    let force = cx.listener(|forge, _: &ClickEvent, _, cx| {
        if let Some(form) = &mut forge.repo_settings {
            form.allow_force = !form.allow_force;
        }
        forge.configure(cx);
    });
    let delete = cx.listener(|forge, _: &ClickEvent, _, cx| {
        if let Some(form) = &mut forge.repo_settings {
            form.allow_delete = !form.allow_delete;
        }
        forge.configure(cx);
    });
    let flag =
        |key: &str, title: &'static str, said: &'static str, on: bool, flip: parts::Press| {
            Row::titled(id(format!("{key}-row")), title, ladder)
                .caption(parts::caption(said, ladder, theme))
                .tail(design::switch(
                    id(key.to_owned()),
                    title,
                    on,
                    may,
                    ladder,
                    theme,
                    flip,
                ))
                .draw(ladder, theme)
                .into_any_element()
        };
    [
        flag(
            "forge-settings-force",
            "Allow force pushes",
            "Let writers rewrite a branch's history.",
            settings.allow_force,
            Box::new(force),
        ),
        flag(
            "forge-settings-delete",
            "Allow ref deletion",
            "Let writers delete branches and tags.",
            settings.allow_delete,
            Box::new(delete),
        ),
    ]
}

/// Who may write: a row each, Revoke beside it for the owner; none says
/// so; the owner's "Grant write access…" opens the people sheet.
fn writer_rows(
    forge: &Forge,
    writers: &[forge::Principal],
    owner: bool,
    cx: &mut Context<Forge>,
    ladder: &Ladder,
    theme: &Theme,
) -> Vec<AnyElement> {
    let busy = !forge.pending_in("settings").is_empty();
    let mut rows: Vec<AnyElement> = writers
        .iter()
        .map(|writer| {
            let name = forge.principal_name(writer);
            let mut row = Row::titled(id(format!("forge-writer-{name}")), name.clone(), ladder);
            if owner {
                let who = writer.clone();
                let revoke =
                    cx.listener(move |forge, _: &ClickEvent, _, cx| forge.revoke(who.clone(), cx));
                row = row.tail(
                    parts::small_button(
                        id(format!("forge-revoke-{name}")),
                        "Revoke",
                        ladder,
                        theme,
                        revoke,
                    )
                    .kind(Kind::Quiet)
                    .enabled(!busy),
                );
            }
            row.draw(ladder, theme).into_any_element()
        })
        .collect();
    if writers.is_empty() {
        rows.push(
            parts::quiet(
                id("forge-writers-none"),
                "Only the owner writes here.",
                ladder,
                theme,
            )
            .into_any_element(),
        );
    }
    if owner {
        let grant =
            cx.listener(|forge, _: &ClickEvent, _, cx| forge.open_sheet(Some(Sheet::Grant), cx));
        let row = Row::titled(id("forge-grant"), "Grant write access…", ladder)
            .caption(parts::caption(
                "Pick a member. They can push and merge here.",
                ladder,
                theme,
            ))
            .chevron()
            .draw(ladder, theme)
            .role(Role::Button)
            .focusable()
            .when(!busy, |row| row.on_click(grant));
        rows.push(row.into_any_element());
    }
    rows
}
