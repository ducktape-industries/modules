//! One commit (Expo `app/(tabs)/forge/[repo]/commit/[oid].tsx`, board
//! 65): its title over the repository and the ref, Copy in the title
//! row; its facts and its files, then every file's diff against its first
//! parent (the empty tree for a root commit), one after another; a file's
//! row scrolls to its diff.
//!
//! Reads the commit (from the log on screen, else its own row) and its
//! diff; writes the clipboard.
use std::rc::Rc;

use view::design::{Ladder, Nav};
use view::prelude::*;
use view::{Div, Stateful};

use super::lines::{self, Item, Notes, Shown};
use super::parts::{self, Row};
use crate::Forge;
use crate::id;
use crate::select::path_of;
use crate::words::{self, path_text, ref_label};
use forge::{CommitInfo, FileDiff, Reply};
use view::design::short_hex;

pub(super) fn render(
    forge: &Forge,
    nav: Nav,
    cx: &mut Context<Forge>,
    ladder: &Ladder,
    theme: &Theme,
) -> AnyElement {
    let oid = forge.nav().commit.clone().unwrap_or_default();
    let commit = forge.commit(&oid);
    let title = commit
        .as_ref()
        .map(parts::summary_of)
        .filter(|summary| !summary.is_empty())
        .unwrap_or_else(|| short_hex(&oid));
    let caption = format!("{} · {}", forge.repo_name(), ref_label(&forge.head_name()));
    design::page("forge-page-commit", title, nav, ladder, theme)
        .caption(caption)
        .trailing(copy_button(forge, &oid, cx, ladder, theme))
        .children(parts::notices(forge, ladder, theme))
        .child(body(forge, &oid, cx, ladder, theme))
        .into_any_element()
}

/// Copy the commit id: the glyph turns to a check once it is on the
/// clipboard (the Expo app says so in a toast the view has none of).
fn copy_button(
    forge: &Forge,
    oid: &str,
    cx: &mut Context<Forge>,
    ladder: &Ladder,
    theme: &Theme,
) -> impl IntoElement {
    let copied = forge.copied.as_deref() == Some(oid);
    let held = oid.to_owned();
    let copy = cx.listener(move |forge, _: &ClickEvent, _, cx| {
        cx.host()
            .notify::<view::methods::ClipboardWrite>(held.clone());
        forge.copied = Some(held.clone());
        cx.notify();
    });
    let glyph = if copied { "check" } else { "copy" };
    design::icon_button(
        id("forge-commit-copy"),
        design::icon(glyph, ladder.icon),
        "Copy the commit id",
        ladder,
        theme,
        copy,
    )
}

/// The facts and the files as the block's head, the diff's rows under
/// them.
fn body(
    forge: &Forge,
    oid: &str,
    cx: &mut Context<Forge>,
    ladder: &Ladder,
    theme: &Theme,
) -> AnyElement {
    let reply = forge
        .commit_diff_query(oid)
        .and_then(|query| forge.shared(&query));
    let files: Vec<&FileDiff> = match reply.as_deref() {
        Some(Reply::Diff { page, .. }) => page.items.iter().collect(),
        _ => Vec::new(),
    };
    let rows: Vec<Item> = std::iter::once(Item::Head)
        .chain(lines::diff_rows(&files, &Notes::default()))
        .collect();
    let held = oid.to_owned();
    let head: lines::Head = Rc::new(move |forge, cx| head(forge, &held, cx));
    let shown = Shown {
        key: "forge-commit-diff",
        page: lines::page_of((oid, reply.as_ref().map(|reply| Rc::as_ptr(reply) as usize))),
        rows,
        head: Some(head),
        wrapped: false,
    };
    lines::block(shown, forge, cx, ladder, theme)
}

/// The block's head: the commit's facts (or the read in their place),
/// then its files (or the diff's read).
fn head(forge: &Forge, oid: &str, cx: &mut Context<Forge>) -> AnyElement {
    let theme = *cx.global::<Theme>();
    let ladder = &design::TOUCH;
    let Some(commit) = forge.commit(oid) else {
        return parts::quiet(
            id("forge-commit-loading"),
            "Reading the commit…",
            ladder,
            &theme,
        )
        .into_any_element();
    };
    let files = match forge.commit_diff_query(oid) {
        Some(query) => match parts::stage(
            forge,
            &query,
            "forge-commit-files",
            words::READING_DIFF,
            cx,
            (ladder, &theme),
        ) {
            Ok(Reply::Diff { page, .. }) => file_rows(&page.items, cx, ladder, &theme),
            Ok(_) => Vec::new(),
            Err(state) => vec![state],
        },
        None => Vec::new(),
    };
    div()
        .id(id("forge-commit-head"))
        .w(px(forge.layout.width))
        .flex()
        .flex_col()
        .child(facts(&commit, ladder, &theme))
        .children(files)
        .into_any_element()
}

/// Commit, Tree, Parents, Author; the whole message when it says more
/// than its first line.
fn facts(commit: &CommitInfo, ladder: &Ladder, theme: &Theme) -> Div {
    let parents = match commit.parents.is_empty() {
        true => words::ROOT_COMMIT.to_owned(),
        false => commit
            .parents
            .iter()
            .map(|parent| short_hex(parent))
            .collect::<Vec<_>>()
            .join(", "),
    };
    let author = format!(
        "{} · {}",
        String::from_utf8_lossy(&commit.author.name),
        parts::when(commit.author.time)
    );
    let message = String::from_utf8_lossy(&commit.message).trim().to_owned();
    let more = (message != parts::summary_of(commit)).then(|| {
        div()
            .p(design::space::BLOCK)
            .text_type(ladder.secondary)
            .text_color(theme.muted)
            .child(message)
    });
    div()
        .flex()
        .flex_col()
        .child(parts::key_value(
            "Commit",
            parts::data(commit.oid.clone()),
            ladder,
            theme,
        ))
        .child(parts::key_value(
            "Tree",
            parts::data(commit.tree.clone()),
            ladder,
            theme,
        ))
        .child(parts::key_value(
            "Parents",
            parts::data(parents),
            ladder,
            theme,
        ))
        .child(parts::key_value(
            "Author",
            div().child(author),
            ladder,
            theme,
        ))
        .children(more)
}

/// "Files" with how many and their delta, a row per file (its delta in
/// the data face) scrolling to its diff; or that the commit changes
/// nothing.
fn file_rows(
    files: &[FileDiff],
    cx: &mut Context<Forge>,
    ladder: &Ladder,
    theme: &Theme,
) -> Vec<AnyElement> {
    if files.is_empty() {
        let none = design::empty_state(
            id("forge-commit-empty"),
            "git-commit-horizontal",
            words::NO_CHANGES,
            words::SAME_TREE,
            ladder,
            theme,
        );
        return vec![none.into_any_element()];
    }
    let added: u64 = files.iter().map(|file| file.additions).sum();
    let deleted: u64 = files.iter().map(|file| file.deletions).sum();
    let head = design::section_head(id("forge-commit-files-head"), "Files", ladder, theme).child(
        parts::delta(&format!("{} · ", files.len()), added, deleted, theme),
    );
    let mut at = 1;
    let rows = files.iter().map(|file| {
        let row = file_row(file, at, cx, ladder, theme).into_any_element();
        at += 1 + rows_of(file);
        row
    });
    std::iter::once(head.into_any_element())
        .chain(rows)
        .collect()
}

/// How many rows a file's diff takes after its head (`paint`): its
/// hunks' heads and lines, or the one saying why it has none.
fn rows_of(file: &FileDiff) -> usize {
    match file.hunks.is_empty() {
        true => 1,
        false => file.hunks.iter().map(|hunk| 1 + hunk.lines.len()).sum(),
    }
}

/// A file: its icon, its path in the data face, its delta; a press
/// scrolls the block to the file's head, row `at`.
fn file_row(
    file: &FileDiff,
    at: usize,
    cx: &mut Context<Forge>,
    ladder: &Ladder,
    theme: &Theme,
) -> Stateful<Div> {
    let path = path_text(&path_of(file).unwrap_or_default());
    let jump = cx.listener(move |forge, _: &ClickEvent, _, _| forge.lines.scroll_to(at));
    let delta = div()
        .font_family(design::fonts::FAMILY_MONO)
        .text_type(ladder.secondary)
        .child(parts::delta("", file.additions, file.deletions, theme));
    Row::data(id(format!("forge-commit-file-{path}")), path)
        .lead(parts::lead_icon("file-text", ladder, theme))
        .tail(delta)
        .press(ladder, theme)
        .role(Role::Button)
        .focusable()
        .on_click(jump)
}
