//! The diff viewer: unified, drawn by this view from the program's typed
//! hunks. No patch text is parsed anywhere — a literal `++ x` is a source
//! line here, because it arrives as one.
//!
//! Every row is virtual, and a line's gutter number is its comment button.
//! The rows of a diff are laid out once per reply and kept ([`DiffCache`])
//! as indices into it: a frame paints the rows on screen, not the diff,
//! and no line is copied out of the reply.
use ducktape_view_guest::prelude::*;
use std::cell::RefCell;
use std::rc::Rc;

use crate::Forge;
use crate::state::PendingComment;
use crate::ui::components::{PRESS_TARGET, badge, empty_state, id, path_text, quiet};
use crate::ui::staged;
use forge::{Content, FileDiff, FileStatus, LineKind, Query, Reply, Side};

/// A route an event of a virtual row takes back into the view.
pub(crate) type Route<E> = Rc<dyn Fn(&E, &mut Window, &mut App)>;

/// A line-number gutter column, and the +/− marker's.
const GUTTER_W: Pixels = px(44.);
const MARKER_W: Pixels = px(12.);

/// The anchor a gutter button carries: path, new side, line.
pub(crate) type Anchor = (Vec<u8>, bool, u64);

/// One row of the diff as it is painted: a file's header, a hunk's, or a
/// line. Built for the rows on screen, from the held reply.
struct Painted {
    kind: Kind,
    text: String,
    path: Vec<u8>,
    old: Option<u64>,
    new: Option<u64>,
    line: LineKind,
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum Kind {
    File,
    Hunk,
    Line,
}

/// A diff laid out as rows, once: where each file's and each hunk's rows
/// start in the held reply. No line is copied: a row is found by its index
/// and its text read from the reply when it is painted.
struct Rows {
    reply: Rc<Reply>,
    files: Vec<FileRows>,
    /// the rows of the list, the closing row of a cut layout included
    count: usize,
    /// the rows that are lines with a number: the grid a review walks
    lines: usize,
    /// the longest line, the row the list measures its width by
    widest: Option<usize>,
    /// the files past the host's list ceiling, not laid out here
    left_out: usize,
    /// the lines of a file shown alone past the ceiling, not laid out here
    cut_lines: usize,
}

/// One file's place in the rows: its index in the reply's page, its first
/// row and first numbered line, and each hunk's (its header row, its first
/// line). A file without hunks has one row after its header saying why.
struct FileRows {
    file: usize,
    first_row: usize,
    first_line: usize,
    hunks: Vec<(usize, usize)>,
}

/// The most rows one list may hold on the host, by the wire. A diff past
/// it lays out whole files up to it and closes with a row naming the rest;
/// a file past it shown alone draws its lines up to it, the rest counted.
const LIST_CEILING: usize = ducktape_view_guest::wire::MAX_UNIFORM_LIST_COUNT;

/// The rows of the diff on screen, by the reply landed and the one file
/// shown, if one: the same reply lays out the same, so a frame reuses them.
#[derive(Default)]
pub(crate) struct DiffCache(RefCell<Option<(Key, Rc<Rows>)>>);

type Key = (*const Reply, Option<Vec<u8>>);

impl DiffCache {
    fn rows(&self, reply: &Rc<Reply>, only: Option<Vec<u8>>) -> Rc<Rows> {
        let key = (Rc::as_ptr(reply), only);
        let mut slot = self.0.borrow_mut();
        if let Some((held, rows)) = slot.as_ref()
            && *held == key
        {
            return rows.clone();
        }
        let rows = Rc::new(Rows::layout(reply.clone(), key.1.as_deref()));
        *slot = Some((key, rows.clone()));
        rows
    }

    /// Forgets the rows once the reply they index is no longer held: a
    /// screen left drops its read, and the layout must not keep the diff
    /// in memory in its place.
    pub(crate) fn drop_unshared(&self) {
        let mut slot = self.0.borrow_mut();
        if slot
            .as_ref()
            .is_some_and(|(_, rows)| Rc::strong_count(&rows.reply) == 1)
        {
            *slot = None;
        }
    }
}

impl Rows {
    /// Lays out the files of the reply (one, under `only`) in order, whole
    /// files while they fit the list ceiling with the closing row; the one
    /// file under `only` up to that ceiling.
    fn layout(reply: Rc<Reply>, only: Option<&[u8]>) -> Rows {
        let mut rows = Rows {
            reply: reply.clone(),
            files: Vec::new(),
            count: 0,
            lines: 0,
            widest: None,
            left_out: 0,
            cut_lines: 0,
        };
        let page = match &*reply {
            Reply::Diff { page, .. } => &page.items[..],
            _ => &[],
        };
        let mut longest = 0;
        for (index, file) in page.iter().enumerate() {
            if only.is_some_and(|only| path_of(file).as_deref() != Some(only)) {
                continue;
            }
            let span = 1
                + file
                    .hunks
                    .iter()
                    .map(|hunk| 1 + hunk.lines.len())
                    .sum::<usize>()
                + usize::from(file.hunks.is_empty());
            let fits = rows.count + span < LIST_CEILING;
            if rows.left_out > 0 || (!fits && only.is_none()) {
                rows.left_out += 1;
                continue;
            }
            let mut laid = FileRows {
                file: index,
                first_row: rows.count,
                first_line: rows.lines,
                hunks: Vec::with_capacity(file.hunks.len()),
            };
            let mut row = laid.first_row + 1;
            for hunk in &file.hunks {
                // the rows left before the closing row, this hunk's header
                // first; only a file shown alone runs out of them
                let room = (LIST_CEILING - 1).saturating_sub(row + 1);
                let shown = hunk.lines.len().min(room);
                rows.cut_lines += hunk.lines.len() - shown;
                if room == 0 {
                    continue;
                }
                laid.hunks.push((row, rows.lines));
                for (at, line) in hunk.lines[..shown].iter().enumerate() {
                    if line.bytes.len() > longest {
                        longest = line.bytes.len();
                        rows.widest = Some(row + 1 + at);
                    }
                }
                row += 1 + shown;
                rows.lines += shown;
            }
            rows.files.push(laid);
            rows.count = row + usize::from(file.hunks.is_empty());
        }
        rows.count += usize::from(rows.cut());
        rows
    }

    fn page(&self) -> &[FileDiff] {
        match &*self.reply {
            Reply::Diff { page, .. } => &page.items,
            _ => &[],
        }
    }

    /// The row `index` is, found through the file and hunk it falls in.
    fn painted(&self, index: usize) -> Painted {
        if self.cut() && index + 1 == self.count {
            return Painted::banner(Kind::Hunk, self.closing(), Vec::new());
        }
        let at = self.files.partition_point(|file| file.first_row <= index);
        let Some(laid) = at.checked_sub(1).map(|at| &self.files[at]) else {
            return Painted::banner(Kind::Hunk, self.closing(), Vec::new());
        };
        let file = &self.page()[laid.file];
        let path = path_of(file).unwrap_or_default();
        if index == laid.first_row {
            return Painted::banner(Kind::File, file_header(file), path);
        }
        let hunk = laid.hunks.partition_point(|(first, _)| *first <= index);
        let Some((first, _)) = hunk.checked_sub(1).map(|hunk| laid.hunks[hunk]) else {
            // past the header of a file with no hunks, or past its last
            // row: the closing row
            if index == laid.first_row + 1 && file.hunks.is_empty() {
                return Painted::banner(Kind::Hunk, why(file).into(), path);
            }
            return Painted::banner(Kind::Hunk, self.closing(), Vec::new());
        };
        let hunk = &file.hunks[hunk - 1];
        let Some(line) = (index - first).checked_sub(1) else {
            let text = format!(
                "@@ -{},{} +{},{} @@",
                hunk.old.start, hunk.old.count, hunk.new.start, hunk.new.count
            );
            return Painted::banner(Kind::Hunk, text, path);
        };
        let Some(line) = hunk.lines.get(line) else {
            return Painted::banner(Kind::Hunk, self.closing(), Vec::new());
        };
        Painted {
            kind: Kind::Line,
            text: String::from_utf8_lossy(&line.bytes)
                .trim_end_matches('\n')
                .to_owned(),
            path,
            old: line.old_line,
            new: line.new_line,
            line: line.kind,
        }
    }

    /// The row of the `line`th numbered line.
    fn row_of_line(&self, line: usize) -> usize {
        let at = self.files.partition_point(|file| file.first_line <= line);
        let Some(laid) = at.checked_sub(1).map(|at| &self.files[at]) else {
            return 0;
        };
        let hunk = laid.hunks.partition_point(|(_, first)| *first <= line);
        let Some((row, first)) = hunk.checked_sub(1).map(|hunk| laid.hunks[hunk]) else {
            return laid.first_row;
        };
        row + 1 + (line - first)
    }

    /// Whether the list ends in a closing row: files or lines left out.
    fn cut(&self) -> bool {
        self.left_out > 0 || self.cut_lines > 0
    }

    /// What the closing row of a cut layout says.
    fn closing(&self) -> String {
        if self.cut_lines > 0 {
            let lines = design::plural(self.cut_lines as u64, "more line", "more lines");
            return format!("{lines} not drawn here");
        }
        format!(
            "{} not drawn here: open one from the file list",
            design::plural(self.left_out as u64, "more file", "more files")
        )
    }
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
    let Some(shared) = forge.shared(query) else {
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
    let rows = forge.diff_cache.rows(&shared, only);
    let count = rows.count;
    let theme = *theme;
    let comment: Route<Anchor> = Rc::new(cx.listener(|forge, at: &Anchor, window, cx| {
        forge.open_comment(at.0.clone(), at.1, at.2, window, cx)
    }));
    let handle = forge.diff_scroll.clone();
    // what a reader has written on these lines, looked up per row painted
    let drafts: Rc<Vec<PendingComment>> = Rc::new(
        reviewable
            .then(|| forge.review())
            .flatten()
            .map(|review| review.comments.clone())
            .unwrap_or_default(),
    );
    let published: Rc<Vec<Anchored>> = Rc::new(if reviewable {
        published_comments(forge)
    } else {
        Vec::new()
    });
    // Under a review the lines are a grid: ↑ ↓ walk the lines with a
    // gutter, ← → a line's gutters (old, new), Enter comments there. The
    // active line is scrolled into view before its gutter claims, since a
    // virtual row off screen has no node to claim with. At rest the first
    // gutter is claimed and drawn like any other.
    let lines = if reviewable { rows.lines } else { 0 };
    let walked = forge.diff_cursor.is_some();
    let (at, cell) = forge.diff_cursor.unwrap_or_default();
    let at = at.min(lines.saturating_sub(1));
    let cells = if lines > 0 {
        anchors(&rows.painted(rows.row_of_line(at))).len()
    } else {
        0
    };
    let cell = cell.min(cells.saturating_sub(1));
    let active = (lines > 0).then(|| (rows.row_of_line(at), cell));
    let press = comment.clone();
    let painted = rows.clone();
    let list = crate::ui::components::rows(
        element_id,
        count,
        rows.widest,
        Some(&handle),
        move |index| {
            let active = active
                .filter(|(line, _)| *line == index)
                .map(|(_, cell)| (cell, walked));
            let row = painted.painted(index);
            let notes = Notes {
                draft: drafts
                    .iter()
                    .find(|c| anchored(&row, &c.path, c.new_side, c.line))
                    .map(|c| c.body.text().clone()),
                published: published
                    .iter()
                    .filter(|(p, side, at, _, _, _)| anchored(&row, p, *side, *at))
                    .map(|(_, _, _, author, body, outdated)| {
                        (author.clone(), body.clone(), *outdated)
                    })
                    .collect(),
            };
            paint_row(&row, index, reviewable, active, notes, &comment, &theme)
        },
    );
    let list = match lines {
        0 => list,
        _ => {
            let reveal = rows.clone();
            let pressed = rows.clone();
            design::composite(id(format!("{element_id}-lines")), Role::Grid, "Diff")
                .active(at, lines)
                .cells(cell, cells)
                .on_move(cx.processor(move |forge, index: usize, _, cx| {
                    forge.diff_cursor = Some((index, 0));
                    forge
                        .diff_scroll
                        .scroll_to_item(reveal.row_of_line(index), ScrollStrategy::Nearest);
                    cx.notify();
                }))
                .on_move_cell(cx.processor(|forge, cell: usize, _, cx| {
                    let (line, _) = forge.diff_cursor.unwrap_or_default();
                    forge.diff_cursor = Some((line, cell));
                    cx.notify();
                }))
                .on_press(move |index, window, app| {
                    let row = pressed.painted(pressed.row_of_line(index));
                    if let Some(anchor) = anchors(&row).get(cell) {
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

/// The most of the pane the file strip takes: past it the strip scrolls,
/// so a commit touching many files still shows its diff beneath.
const FILE_STRIP_MAX_H: Pixels = px(168.);

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
        .flex_none()
        .max_h(FILE_STRIP_MAX_H)
        .overflow_y_scroll()
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

/// A line's gutters as anchors, in paint order: the old side, then the
/// new; a line with one number has one.
fn anchors(row: &Painted) -> Vec<Anchor> {
    [(false, row.old), (true, row.new)]
        .into_iter()
        .filter_map(|(new_side, number)| Some((row.path.clone(), new_side, number?)))
        .collect()
}

/// Whether a comment at `path:line (side)` sits on this line: a line is
/// anchored on its new side when it has one, else its old.
fn anchored(row: &Painted, path: &[u8], new_side: bool, line: u64) -> bool {
    row.kind == Kind::Line
        && row.path == path
        && new_side == row.new.is_some()
        && row.new.or(row.old) == Some(line)
}

/// What readers wrote on one line: this reader's draft, and the published
/// comments (author, body, outdated).
struct Notes {
    draft: Option<String>,
    published: Vec<(String, String, bool)>,
}

impl Painted {
    /// A row that is not a source line: a file's header, a hunk's, or the
    /// closing row.
    fn banner(kind: Kind, text: String, path: Vec<u8>) -> Painted {
        Painted {
            kind,
            text,
            path,
            old: None,
            new: None,
            line: LineKind::Context,
        }
    }
}

/// A file's header row: its path (old → new when renamed), status, counts.
fn file_header(file: &FileDiff) -> String {
    let header = match (&file.old_path, &file.new_path) {
        (Some(old), Some(new)) if old != new => {
            format!("{} → {}", path_text(old), path_text(new))
        }
        _ => path_text(&path_of(file).unwrap_or_default()),
    };
    format!(
        "{header} · {} · +{} −{}",
        status_label(file.status),
        file.additions,
        file.deletions
    )
}

/// Why a file has no lines to show.
fn why(file: &FileDiff) -> &'static str {
    match file.content {
        Content::Binary => "Binary file — no lines to show",
        Content::Oversize => "Too large to diff inline",
        Content::Gitlink => "Submodule pointer",
        Content::Text => "No line changes",
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
    active: Option<(usize, bool)>,
    notes: Notes,
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
        Kind::Line => line_row(row, index, reviewable, active, notes, comment, theme),
    }
}

/// `active`: which of the line's gutters the arrows are on, under a review,
/// and whether they have walked there: only then is it drawn.
fn line_row(
    row: &Painted,
    index: usize,
    reviewable: bool,
    active: Option<(usize, bool)>,
    notes: Notes,
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
    let (claimed, walked) = match active {
        Some((cell, walked)) => (Some(cell), walked),
        None => (None, false),
    };
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
            (claimed == Some(0), walked),
            comment,
            theme,
        ))
        .child(gutter(
            row,
            true,
            reviewable,
            (claimed == Some(cells), walked),
            comment,
            theme,
        ))
        .child(
            div()
                .id(id(format!("forge-diff-sign-{index}")))
                .w(MARKER_W)
                .role(Role::ListMarker)
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
    if notes.published.is_empty() && notes.draft.is_none() {
        return body.into_any_element();
    }
    div()
        .id(id(format!("forge-diff-thread-{index}")))
        .w_full()
        .flex()
        .flex_col()
        .child(body)
        .children(
            notes
                .published
                .iter()
                .enumerate()
                .map(|(at, (author, text, outdated))| {
                    published_comment(index, at, author, text, *outdated, theme)
                }),
        )
        .children(notes.draft.as_ref().map(|draft| {
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
/// at `path:line (side)`; otherwise it is the line's marker. Either way a
/// drag down the diff copies the code without it.
fn gutter(
    row: &Painted,
    new_side: bool,
    reviewable: bool,
    // the keys' claim is on it; it is drawn once the arrows walked there
    (active, walked): (bool, bool),
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
        return cell().into_any_element();
    };
    let side = if new_side { "new" } else { "old" };
    if !reviewable {
        return cell()
            .id(id(format!(
                "forge-gutter-{}-{side}-{number}-cell",
                path_text(&row.path)
            )))
            .role(Role::ListMarker)
            .child(number.to_string())
            .into_any_element();
    }
    let at = (row.path.clone(), new_side, number);
    let comment = comment.clone();
    let button = div()
        .id(id(format!(
            "forge-gutter-{}-{side}-{number}",
            path_text(&row.path)
        )))
        .w_full()
        .min_h(PRESS_TARGET)
        .flex()
        .items_center()
        .hover(|style| style.bg(theme.accent_soft))
        .when(active && walked, |button| button.bg(theme.accent_soft))
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
            Input::new(id("forge-comment-body"), &open.body, "Line comment")
                .h(design::size::CONTROL)
                .w_full()
                .px_2()
                .border_1()
                .border_color(theme.border_strong)
                .bg(theme.background)
                .text_color(theme.foreground)
                .placeholder("What should change here?")
                .on_submit(cx.listener(|forge, _: &(), _, cx| forge.stage_comment(cx))),
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

#[cfg(test)]
mod tests {
    use super::*;

    fn file(path: &str, lines: &[&str]) -> FileDiff {
        FileDiff {
            old_path: None,
            new_path: Some(path.as_bytes().to_vec()),
            status: FileStatus::Added,
            old_oid: None,
            new_oid: Some("new".into()),
            old_kind: None,
            new_kind: Some(forge::EntryKind::File),
            old_size: 0,
            new_size: 1,
            content: Content::Text,
            additions: lines.len() as u64,
            deletions: 0,
            hunks: vec![forge::DiffHunk {
                old: forge::LineRange { start: 0, count: 0 },
                new: forge::LineRange {
                    start: 1,
                    count: lines.len() as u64,
                },
                lines: lines
                    .iter()
                    .enumerate()
                    .map(|(i, text)| forge::DiffLine {
                        kind: LineKind::Added,
                        old_line: None,
                        new_line: Some(i as u64 + 1),
                        bytes: format!("{text}\n").into_bytes(),
                    })
                    .collect(),
            }],
        }
    }

    fn reply(files: Vec<FileDiff>) -> Rc<Reply> {
        Rc::new(Reply::Diff {
            height: 1,
            base: Some("base".into()),
            head: "head".into(),
            total_files: files.len() as u64,
            page: forge::PageResponse {
                height: 1,
                items: files,
                next: None,
            },
        })
    }

    /// The same reply is laid out once: a frame gets the rows the last one
    /// had. Another reply, or one file of it, is laid out anew.
    #[test]
    fn rows_are_kept_per_reply() {
        let cache = DiffCache::default();
        let a = reply(vec![file("a.rs", &["one", "a longer line", "three"])]);
        let first = cache.rows(&a, None);
        assert_eq!((first.count, first.lines, first.widest), (5, 3, Some(3)));
        assert!(Rc::ptr_eq(&first, &cache.rows(&a, None)));
        assert!(!Rc::ptr_eq(&first, &cache.rows(&a, Some(b"a.rs".to_vec()))));
        let again = reply(vec![file("a.rs", &["one"])]);
        assert!(!Rc::ptr_eq(&first, &cache.rows(&again, None)));
        // the reply still held elsewhere keeps its rows; dropped, they go
        cache.drop_unshared();
        assert!(cache.0.borrow().is_some());
        drop(again);
        cache.drop_unshared();
        assert!(cache.0.borrow().is_none());
    }

    /// Every row is found by its index: headers, hunks, lines, a file with
    /// no hunks, and the grid of numbered lines walks the lines alone.
    #[test]
    fn rows_are_found_by_index() {
        let mut empty = file("b.bin", &[]);
        empty.hunks.clear();
        empty.content = Content::Binary;
        let rows = Rows::layout(
            reply(vec![
                file("a.rs", &["one", "two"]),
                empty,
                file("c.rs", &["three"]),
            ]),
            None,
        );
        assert_eq!((rows.count, rows.lines, rows.left_out), (9, 3, 0));
        let texts: Vec<(Kind, String)> = (0..rows.count)
            .map(|index| rows.painted(index))
            .map(|row| (row.kind, row.text))
            .collect();
        assert_eq!(
            texts,
            [
                (Kind::File, "a.rs · added · +2 −0".into()),
                (Kind::Hunk, "@@ -0,0 +1,2 @@".into()),
                (Kind::Line, "one".into()),
                (Kind::Line, "two".into()),
                (Kind::File, "b.bin · added · +0 −0".into()),
                (Kind::Hunk, "Binary file — no lines to show".into()),
                (Kind::File, "c.rs · added · +1 −0".into()),
                (Kind::Hunk, "@@ -0,0 +1,1 @@".into()),
                (Kind::Line, "three".into()),
            ]
        );
        assert_eq!(
            (0..3)
                .map(|line| rows.row_of_line(line))
                .collect::<Vec<_>>(),
            [2, 3, 8]
        );
        assert_eq!(rows.painted(8).path, b"c.rs");
    }

    /// A diff past the host's list ceiling lays out whole files up to it
    /// and closes with a row naming the files not drawn.
    #[test]
    fn a_diff_past_the_list_ceiling_closes_with_the_files_left_out() {
        let lines: Vec<String> = (0..1000).map(|n| n.to_string()).collect();
        let lines: Vec<&str> = lines.iter().map(String::as_str).collect();
        // 70 files of 1002 rows each: 65 fit under 65,536 with the closing row
        let files = (0..70).map(|n| file(&format!("f{n}.rs"), &lines)).collect();
        let rows = Rows::layout(reply(files), None);
        assert_eq!(rows.left_out, 5);
        assert_eq!(rows.count, 65 * 1002 + 1);
        assert!(rows.count <= LIST_CEILING);
        let closing = rows.painted(rows.count - 1);
        assert_eq!(
            closing.text,
            "5 more files not drawn here: open one from the file list"
        );
        assert_eq!(rows.painted(rows.count - 2).text, "999");
    }

    /// A file longer than the ceiling on its own is left out of the whole
    /// diff with the files after it; opened alone, it draws as many of its
    /// lines as the list holds and the closing row counts the rest.
    #[test]
    fn a_file_past_the_ceiling_opened_alone_draws_up_to_it() {
        let lines: Vec<String> = (0..70_000).map(|n| n.to_string()).collect();
        let lines: Vec<&str> = lines.iter().map(String::as_str).collect();
        let diff = reply(vec![file("data.txt", &lines), file("z.rs", &["one"])]);
        let whole = Rows::layout(diff.clone(), None);
        assert_eq!((whole.count, whole.left_out), (1, 2));
        let alone = Rows::layout(diff, Some(b"data.txt"));
        assert_eq!(alone.count, LIST_CEILING);
        assert_eq!(alone.lines, LIST_CEILING - 3);
        assert_eq!(
            alone.painted(alone.count - 1).text,
            "4,467 more lines not drawn here"
        );
        assert_eq!(alone.painted(alone.count - 2).text, "65532");
        assert_eq!(alone.row_of_line(alone.lines - 1), alone.count - 2);
    }

    /// A comment sits on a line by its path, the side it is anchored on and
    /// its number there, never on a header row.
    #[test]
    fn a_note_finds_its_line() {
        let rows = Rows::layout(reply(vec![file("a.rs", &["one", "two"])]), None);
        assert!(anchored(&rows.painted(3), b"a.rs", true, 2));
        assert!(!anchored(&rows.painted(3), b"a.rs", false, 2));
        assert!(!anchored(&rows.painted(3), b"b.rs", true, 2));
        assert!(!anchored(&rows.painted(0), b"a.rs", true, 2));
        assert_eq!(anchors(&rows.painted(3)), [(b"a.rs".to_vec(), true, 2)]);
    }
}
