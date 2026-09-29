//! The diff viewer: unified, drawn by this view from the program's typed
//! hunks. No patch text is parsed anywhere — a literal `++ x` is a source
//! line here, because it arrives as one.
//!
//! Every row is virtual, and a line's gutter number is its comment button.
use ducktape_view_guest::design;
use std::rc::Rc;

use ducktape_view_guest::ScrollStrategy;
use ducktape_view_guest::prelude::*;

use crate::Forge;
use crate::state::ReviewSession;
use crate::ui::components::{badge, empty_state, id, path_text, quiet};
use crate::ui::staged;
use forge::{Content, FileDiff, FileStatus, LineKind, Query, Reply, Side};

/// A route an event of a virtual row takes back into the view.
pub(crate) type Route<E> = Rc<dyn Fn(&E, &mut Window, &mut App)>;

/// A line-number gutter column, and the +/− marker's.
const GUTTER_W: Pixels = px(44.);
const MARKER_W: Pixels = px(12.);

/// The anchor a gutter button carries: path, new side, line.
pub(crate) type Anchor = (Vec<u8>, bool, u64);

/// One painted row of the flattened diff.
#[derive(Clone)]
struct Painted {
    kind: Kind,
    text: String,
    path: Vec<u8>,
    old: Option<u64>,
    new: Option<u64>,
    line: LineKind,
    /// a comment this reader has staged at this anchor
    draft: Option<String>,
    /// published line comments anchored here: author, body, outdated
    published: Vec<(String, String, bool)>,
}

#[derive(Clone, Copy, PartialEq)]
enum Kind {
    File,
    Hunk,
    Line,
}

/// `reviewable` turns the gutter into comment buttons.
pub(crate) fn render(
    forge: &Forge,
    query: &Query,
    element_id: &str,
    reviewable: bool,
    cx: &mut Context<Forge>,
    theme: &Theme,
) -> AnyElement {
    let reply = match staged(forge, query, element_id, "Reading the diff…", cx, theme) {
        Ok(reply) => reply,
        Err(state) => return state,
    };
    let Reply::Diff {
        page, total_files, ..
    } = reply
    else {
        return div().into_any_element();
    };
    let only = forge.nav().diff_path.clone();
    let files: Vec<&FileDiff> = page
        .items
        .iter()
        .filter(|file| only.is_none() || only.as_deref() == path_of(file).as_deref())
        .collect();
    if files.is_empty() {
        return empty_state(
            id(format!("{element_id}-empty")),
            "No changes",
            if *total_files == 0 {
                "These endpoints hold the same tree."
            } else {
                "No file here matches what is selected."
            },
            theme,
        )
        .into_any_element();
    }
    // A screen with no file tree beside it (a commit's own diff) carries the
    // file headers above the virtual list, where they stay in view.
    let strip = (!reviewable).then(|| file_strip(&files, element_id, cx, theme));
    let rows = paint(forge, &files, reviewable);
    let count = rows.len();
    // A diff's width is set by its longest source line, so that row is the
    // one the list measures — and the one a headless render always draws.
    let widest = rows
        .iter()
        .enumerate()
        .filter(|(_, row)| row.kind == Kind::Line)
        .max_by_key(|(_, row)| row.text.len())
        .map(|(index, _)| index);
    let theme = *theme;
    let comment: Route<Anchor> =
        Rc::new(cx.listener(|forge, at: &Anchor, _, cx| {
            forge.open_comment(at.0.clone(), at.1, at.2, cx)
        }));
    let handle = forge.diff_scroll.clone();
    // Under a review the lines are a grid: ↑ ↓ walk the lines with a
    // gutter, ← → a line's gutters (old, new), Enter comments there. The
    // active line is scrolled into view before its gutter claims, since a
    // virtual row off screen has no node to claim with.
    let lines: Vec<usize> = rows
        .iter()
        .enumerate()
        .filter(|(_, row)| reviewable && row.kind == Kind::Line && row.old.or(row.new).is_some())
        .map(|(index, _)| index)
        .collect();
    let anchors: Vec<Vec<Anchor>> = lines
        .iter()
        .map(|index| {
            let row = &rows[*index];
            [(false, row.old), (true, row.new)]
                .into_iter()
                .filter_map(|(new_side, number)| Some((row.path.clone(), new_side, number?)))
                .collect()
        })
        .collect();
    let (at, cell) = forge.diff_cursor;
    let at = at.min(lines.len().saturating_sub(1));
    let cell = cell.min(
        anchors
            .get(at)
            .map_or(0, |cells| cells.len().saturating_sub(1)),
    );
    let active = lines.get(at).map(|line| (*line, cell));
    let press = comment.clone();
    let list =
        crate::ui::components::rows(element_id, count, widest, Some(&handle), move |index| {
            let active = active
                .filter(|(line, _)| *line == index)
                .map(|(_, cell)| cell);
            paint_row(&rows[index], index, reviewable, active, &comment, &theme)
        });
    let list = match lines.is_empty() {
        true => list,
        false => {
            let reveal = lines.clone();
            let pressed = anchors.clone();
            design::composite(id(format!("{element_id}-lines")), Role::Grid, "Diff")
                .active(at, lines.len())
                .cells(cell, anchors[at].len())
                .on_move(cx.processor(move |forge, index: usize, _, cx| {
                    forge.diff_cursor = (index, 0);
                    forge
                        .diff_scroll
                        .scroll_to_item(reveal[index], ScrollStrategy::Nearest);
                    cx.notify();
                }))
                .on_move_cell(cx.processor(|forge, cell: usize, _, cx| {
                    forge.diff_cursor.1 = cell;
                    cx.notify();
                }))
                .on_press(move |index, window, app| {
                    if let Some(anchor) = pressed[index].get(cell) {
                        press(anchor, window, app);
                    }
                })
                .build()
                .flex_1()
                .min_h(px(0.))
                .flex()
                .flex_col()
                .child(list)
                .into_any_element()
        }
    };
    let Some(strip) = strip else {
        return list.into_any_element();
    };
    div()
        .id(id(format!("{element_id}-pane")))
        .flex()
        .flex_col()
        .flex_1()
        .min_h(px(0.))
        .child(strip)
        .child(list)
        .into_any_element()
}

/// Which files this diff touches, above the rows themselves. Each one
/// narrows the list to itself.
fn file_strip(
    files: &[&FileDiff],
    element_id: &str,
    cx: &mut Context<Forge>,
    theme: &Theme,
) -> AnyElement {
    let mut strip = div()
        .id(id(format!("{element_id}-files")))
        .flex()
        .flex_col()
        .gap_0p5()
        .px_2()
        .py_1()
        .border_b_1()
        .border_color(theme.border);
    for file in files {
        let Some(path) = path_of(file) else { continue };
        let label = path_text(&path);
        let pick = cx.listener({
            let path = path.clone();
            move |forge, _: &ClickEvent, _, cx| forge.single_file(Some(path.clone()), cx)
        });
        strip = strip.child(
            div()
                .id(id(format!("{element_id}-file-{label}")))
                .flex()
                .gap_2()
                .text_size(design::text::SECONDARY)
                .role(Role::Button)
                .focusable()
                .on_click(pick)
                .child(crate::ui::bold(label))
                .child(quiet(status_label(file.status), theme))
                .child(quiet(
                    format!("+{} −{}", file.additions, file.deletions),
                    theme,
                )),
        );
    }
    strip.into_any_element()
}

pub(crate) fn path_of(file: &FileDiff) -> Option<Vec<u8>> {
    file.new_path.clone().or_else(|| file.old_path.clone())
}

fn status_label(status: FileStatus) -> &'static str {
    match status {
        FileStatus::Added => "added",
        FileStatus::Deleted => "deleted",
        FileStatus::Modified => "modified",
        FileStatus::ModeChanged => "mode changed",
        FileStatus::TypeChanged => "type changed",
    }
}

impl Painted {
    /// A row that is not a source line: a file's header or a hunk's.
    fn banner(kind: Kind, text: String, path: &[u8]) -> Painted {
        Painted {
            kind,
            text,
            path: path.to_vec(),
            old: None,
            new: None,
            line: LineKind::Context,
            draft: None,
            published: Vec::new(),
        }
    }
}

fn paint(forge: &Forge, files: &[&FileDiff], reviewable: bool) -> Vec<Painted> {
    let review = reviewable.then(|| forge.review()).flatten();
    let published = if reviewable {
        published_comments(forge)
    } else {
        Vec::new()
    };
    let mut rows = Vec::new();
    for file in files {
        paint_file(&mut rows, file, review, &published);
    }
    rows
}

/// One file's rows: its header, then each hunk's header and lines, or one
/// line saying why it has none.
fn paint_file(
    rows: &mut Vec<Painted>,
    file: &FileDiff,
    review: Option<&ReviewSession>,
    published: &[Anchored],
) {
    let path = path_of(file).unwrap_or_default();
    let header = match (&file.old_path, &file.new_path) {
        (Some(old), Some(new)) if old != new => {
            format!("{} → {}", path_text(old), path_text(new))
        }
        _ => path_text(&path),
    };
    let text = format!(
        "{header} · {} · +{} −{}",
        status_label(file.status),
        file.additions,
        file.deletions
    );
    rows.push(Painted::banner(Kind::File, text, &path));
    if file.hunks.is_empty() {
        let why = match file.content {
            Content::Binary => "Binary file — no lines to show",
            Content::Oversize => "Too large to diff inline",
            Content::Gitlink => "Submodule pointer",
            Content::Text => "No line changes",
        };
        rows.push(Painted::banner(Kind::Hunk, why.into(), &path));
        return;
    }
    for hunk in &file.hunks {
        let text = format!(
            "@@ -{},{} +{},{} @@",
            hunk.old.start, hunk.old.count, hunk.new.start, hunk.new.count
        );
        rows.push(Painted::banner(Kind::Hunk, text, &path));
        for line in &hunk.lines {
            let anchor_side = line.new_line.is_some();
            let anchor_line = line.new_line.or(line.old_line).unwrap_or(0);
            rows.push(Painted {
                kind: Kind::Line,
                text: String::from_utf8_lossy(&line.bytes)
                    .trim_end_matches('\n')
                    .to_owned(),
                path: path.clone(),
                old: line.old_line,
                new: line.new_line,
                line: line.kind,
                draft: review
                    .and_then(|review| review.staged(&path, anchor_side, anchor_line))
                    .map(|staged| staged.body.clone()),
                published: published
                    .iter()
                    .filter(|(p, side, at, _, _, _)| {
                        *p == path && *side == anchor_side && *at == anchor_line
                    })
                    .map(|(_, _, _, author, body, outdated)| {
                        (author.clone(), body.clone(), *outdated)
                    })
                    .collect(),
            });
        }
    }
}

type Anchored = (Vec<u8>, bool, u64, String, String, bool);

/// Every published line comment of the open change, by anchor.
fn published_comments(forge: &Forge) -> Vec<Anchored> {
    let Some((_, _, _, reviews)) = forge.change() else {
        return Vec::new();
    };
    let mut all = Vec::new();
    for review in &reviews.items {
        let author = forge.principal_name(&review.author);
        let outdated = forge.outdated(&review.draft.commit_oid);
        for comment in &review.draft.comments {
            all.push((
                comment.path.clone(),
                comment.side == Side::New,
                comment.line,
                author.clone(),
                comment.body.clone(),
                outdated,
            ));
        }
    }
    all
}

fn paint_row(
    row: &Painted,
    index: usize,
    reviewable: bool,
    active: Option<usize>,
    comment: &Route<Anchor>,
    theme: &Theme,
) -> AnyElement {
    match row.kind {
        Kind::File => div()
            .id(id(format!("forge-diff-file-{}", path_text(&row.path))))
            .w_full()
            .px(design::space::BLOCK)
            .pt(design::space::LG)
            .pb(design::space::XS)
            .font_family(design::fonts::FAMILY_MONO)
            .text_size(design::text::CAPTION)
            .font_weight(ducktape_view_guest::FontWeight::MEDIUM)
            .child(row.text.clone())
            .into_any_element(),
        Kind::Hunk => div()
            .id(id(format!("forge-diff-hunk-{index}")))
            .w_full()
            .px(design::space::BLOCK)
            .font_family(design::fonts::FAMILY_MONO)
            .text_size(design::text::CAPTION)
            .text_color(theme.faint)
            .child(row.text.clone())
            .into_any_element(),
        Kind::Line => line_row(row, index, reviewable, active, comment, theme),
    }
}

/// `active`: which of the line's gutters the arrows are on, under a review.
fn line_row(
    row: &Painted,
    index: usize,
    reviewable: bool,
    active: Option<usize>,
    comment: &Route<Anchor>,
    theme: &Theme,
) -> AnyElement {
    let (background, colour) = match row.line {
        LineKind::Added => (theme.success_soft, theme.foreground),
        LineKind::Deleted => (theme.danger_soft, theme.foreground),
        LineKind::Context => (theme.background, theme.foreground),
    };
    let marker = match row.line {
        LineKind::Added => "+",
        LineKind::Deleted => "−",
        LineKind::Context => " ",
    };
    // the gutters are the row's cells, in paint order: the old side, then
    // the new; a line with one number has one cell
    let cells = usize::from(row.old.is_some());
    let body = div()
        .id(id(format!("forge-diff-line-{index}")))
        .w_full()
        .flex()
        .items_center()
        .gap_1()
        .px(design::space::BLOCK)
        .bg(background)
        .when(reviewable, |body| body.role(Role::Row))
        .child(gutter(
            row,
            false,
            reviewable,
            active == Some(0),
            comment,
            theme,
        ))
        .child(gutter(
            row,
            true,
            reviewable,
            active == Some(cells),
            comment,
            theme,
        ))
        .child(
            div()
                .w(MARKER_W)
                .font_family(design::fonts::FAMILY_MONO)
                .text_size(design::text::SECONDARY)
                .text_color(theme.muted)
                .child(marker),
        )
        .child(
            ducktape_view_guest::design::mono(row.text.clone())
                .flex_1()
                .min_w(px(0.))
                .text_color(colour),
        );
    if row.published.is_empty() && row.draft.is_none() {
        return body.into_any_element();
    }
    div()
        .id(id(format!("forge-diff-thread-{index}")))
        .w_full()
        .flex()
        .flex_col()
        .child(body)
        .children(
            row.published
                .iter()
                .enumerate()
                .map(|(at, (author, text, outdated))| {
                    published_comment(index, at, author, text, *outdated, theme)
                }),
        )
        .children(row.draft.as_ref().map(|draft| {
            comment_box(
                id(format!("forge-diff-draft-{index}")),
                "Your pending comment".to_owned(),
                draft,
                None,
                theme,
            )
        }))
        .into_any_element()
}

/// One published line comment under its line.
fn published_comment(
    index: usize,
    at: usize,
    author: &str,
    text: &str,
    outdated: bool,
    theme: &Theme,
) -> AnyElement {
    let tag = outdated.then(|| {
        badge(
            id(format!("forge-diff-outdated-{index}-{at}")),
            "outdated",
            theme.warning,
            theme.warning_soft,
        )
        .into_any_element()
    });
    comment_box(
        id(format!("forge-diff-comment-{index}-{at}")),
        author.to_owned(),
        text,
        tag,
        theme,
    )
}

/// A comment boxed under the line it is on: who (or whose draft) over
/// what it says.
fn comment_box(
    element_id: ElementId,
    who: String,
    text: &str,
    tag: Option<AnyElement>,
    theme: &Theme,
) -> AnyElement {
    div()
        .id(element_id)
        .w_full()
        .pl(GUTTER_W * 2. + MARKER_W + design::space::BLOCK + design::space::SM)
        .pr(design::space::BLOCK)
        .py(design::space::XS)
        .child(
            div()
                .max_w(px(520.))
                .flex()
                .flex_col()
                .gap(design::space::HAIR)
                .px(design::space::MD)
                .py(design::space::SM)
                .border_1()
                .border_color(theme.border_strong)
                .bg(theme.background)
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap(design::space::SM)
                        .text_size(design::text::SECONDARY)
                        .text_color(theme.muted)
                        .child(who)
                        .children(tag),
                )
                .child(div().child(text.to_owned())),
        )
        .into_any_element()
}

/// A gutter number. Under a review it is the button that anchors a comment
/// at `path:line (side)`; otherwise it is a number.
fn gutter(
    row: &Painted,
    new_side: bool,
    reviewable: bool,
    active: bool,
    comment: &Route<Anchor>,
    theme: &Theme,
) -> AnyElement {
    let number = if new_side { row.new } else { row.old };
    let cell = || {
        div()
            .w(GUTTER_W)
            .font_family(design::fonts::FAMILY_MONO)
            .text_size(design::text::CAPTION)
            .text_color(theme.muted)
    };
    let Some(number) = number else {
        return cell().child(" ").into_any_element();
    };
    if !reviewable {
        return cell().child(number.to_string()).into_any_element();
    }
    let at = (row.path.clone(), new_side, number);
    let comment = comment.clone();
    let side = if new_side { "new" } else { "old" };
    let button = div()
        .id(id(format!(
            "forge-gutter-{}-{side}-{number}",
            path_text(&row.path)
        )))
        .w_full()
        .hover(|style| style.bg(theme.accent_soft))
        .when(active, |button| button.bg(theme.accent_soft))
        .aria_label("Comment on this line")
        .on_click(move |_: &ClickEvent, window: &mut Window, app: &mut App| {
            comment(&at, window, app)
        })
        .child(number.to_string());
    cell()
        .id(id(format!(
            "forge-gutter-{}-{side}-{number}-cell",
            path_text(&row.path)
        )))
        .role(Role::GridCell)
        .child(design::item(button, Role::Button, active))
        .into_any_element()
}

/// The composer for the one anchor whose gutter was clicked.
pub(crate) fn composer(forge: &Forge, cx: &mut Context<Forge>, theme: &Theme) -> AnyElement {
    let Some(review) = forge.review() else {
        return div().into_any_element();
    };
    let Some(open) = review.open.clone() else {
        return div().into_any_element();
    };
    let typed = cx.listener(|forge, text: &String, _, cx| forge.typed_comment(text.clone(), cx));
    let save = cx.listener(|forge, _: &ClickEvent, _, cx| forge.stage_comment(cx));
    let cancel = cx.listener(|forge, _: &ClickEvent, _, cx| forge.discard_comment(cx));
    let mut card = div()
        .id(id("forge-comment"))
        .flex()
        .flex_col()
        .gap_2()
        .m_2()
        .p_2()
        .border_1()
        .border_color(theme.border_strong)
        .bg(theme.surface)
        .child(quiet(open.anchor(), theme))
        .child(
            Input::new(id("forge-comment-body"), "Line comment")
                .h(design::size::CONTROL)
                .w_full()
                .px_2()
                .border_1()
                .border_color(theme.border_strong)
                .bg(theme.background)
                .text_color(theme.foreground)
                .value(open.body)
                .placeholder("What should change here?")
                .on_input(typed),
        );
    if !review.error.is_empty() {
        card = card.child(
            div()
                .id(id("forge-comment-error"))
                .text_size(design::text::SECONDARY)
                .text_color(theme.danger)
                .child(review.error.clone()),
        );
    }
    card.child(
        div()
            .flex()
            .gap_2()
            .child(div().flex_1())
            .child(crate::ui::components::button(
                id("forge-comment-cancel"),
                "Cancel",
                theme,
                cancel,
            ))
            .child(
                crate::ui::components::button(id("forge-comment-save"), "Stage", theme, save)
                    .kind(design::Kind::Primary),
            ),
    )
    .into_any_element()
}
