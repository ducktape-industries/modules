//! The diff viewer: unified, drawn by this view from the program's typed
//! hunks. No patch text is parsed anywhere — a literal `++ x` is a source
//! line here, because it arrives as one.
//!
//! Every row is virtual, and a line's gutter number is its comment button.
//! A virtual list draws every row one height, so what readers write on a
//! line (its comments, a draft, the open composer) takes rows of its own
//! after the line ([`Notes`]) rather than growing the line's row.
//! The rows of a diff are laid out once per reply and kept ([`DiffCache`])
//! as indices into it: a frame paints the rows on screen, not the diff,
//! and no line is copied out of the reply.
use std::cell::RefCell;
use std::collections::BTreeMap;
use std::rc::Rc;
use view::prelude::*;

use crate::Forge;
use crate::ui::components::{PRESS_TARGET, badge, empty_state, id, path_text, quiet};
use crate::ui::staged;
use forge::{Content, FileDiff, FileStatus, LineKind, Query, Reply, Side};
use view::design::Ladder;

/// A route an event of a virtual row takes back into the view.
pub(crate) type Route<E> = Rc<dyn Fn(&E, &mut Window, &mut App)>;

/// A line-number gutter column, and the +/− marker's.
const GUTTER_W: Pixels = px(44.);
const MARKER_W: Pixels = px(12.);

/// A reviewable diff's row: its gutter's press target. Every row of it,
/// a note's included, is this tall, so a note's rows meet edge to edge.
const ROW_H: Pixels = PRESS_TARGET;
/// The composer's buttons' label line: on it an SDK button, its padding
/// and edge around the label, is a row tall (measured on the host: Cancel
/// 24 px, Stage 22). On the text's own line it is 27 px, sized for a
/// control's 28 px row (`design::size::CONTROL`), and stood past this
/// shorter row onto the field's edge above and the box's below.
const BUTTON_LINE: Pixels = px(16.);
/// How wide a note box grows.
const NOTE_W: Pixels = px(520.);
/// What one line of a note box holds, in character widths (a wide CJK
/// character counts two): a line of capitals, about 8 px a character at
/// the note's size, still fits the box's 498 px of text.
const NOTE_CHARS: usize = 60;

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
    count: usize,
    /// the rows that are lines with a number: the grid a review walks
    lines: usize,
    /// the longest line, the row the list measures its width by
    widest: Option<usize>,
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
    fn layout(reply: Rc<Reply>, only: Option<&[u8]>) -> Rows {
        let mut rows = Rows {
            reply: reply.clone(),
            files: Vec::new(),
            count: 0,
            lines: 0,
            widest: None,
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
            let mut laid = FileRows {
                file: index,
                first_row: rows.count,
                first_line: rows.lines,
                hunks: Vec::with_capacity(file.hunks.len()),
            };
            let mut row = laid.first_row + 1;
            for hunk in &file.hunks {
                laid.hunks.push((row, rows.lines));
                for (at, line) in hunk.lines.iter().enumerate() {
                    if line.bytes.len() > longest {
                        longest = line.bytes.len();
                        rows.widest = Some(row + 1 + at);
                    }
                }
                row += 1 + hunk.lines.len();
                rows.lines += hunk.lines.len();
            }
            rows.files.push(laid);
            rows.count = row + usize::from(file.hunks.is_empty());
        }
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
        let at = self.files.partition_point(|file| file.first_row <= index);
        let Some(laid) = at.checked_sub(1).map(|at| &self.files[at]) else {
            return Painted::banner(Kind::Hunk, String::new(), Vec::new());
        };
        let file = &self.page()[laid.file];
        let path = path_of(file).unwrap_or_default();
        if index == laid.first_row {
            return Painted::banner(Kind::File, file_header(file), path);
        }
        let hunk = laid.hunks.partition_point(|(first, _)| *first <= index);
        let Some((first, _)) = hunk.checked_sub(1).map(|hunk| laid.hunks[hunk]) else {
            if index == laid.first_row + 1 && file.hunks.is_empty() {
                return Painted::banner(Kind::Hunk, why(file).into(), path);
            }
            return Painted::banner(Kind::Hunk, String::new(), Vec::new());
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
            return Painted::banner(Kind::Hunk, String::new(), Vec::new());
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

    /// The row of the line whose `new_side` gutter carries `number` in the
    /// file at `path`, when that line is laid out.
    fn row_of_anchor(&self, path: &[u8], new_side: bool, number: u64) -> Option<usize> {
        let page = self.page();
        let at = self.files.iter().position(|laid| {
            let file = &page[laid.file];
            file.new_path.as_deref().or(file.old_path.as_deref()) == Some(path)
        })?;
        let laid = &self.files[at];
        let end = self
            .files
            .get(at + 1)
            .map_or(self.lines, |next| next.first_line);
        let hunks = &page[laid.file].hunks;
        laid.hunks
            .iter()
            .enumerate()
            .find_map(|(h, &(row, first))| {
                let next = laid.hunks.get(h + 1).map_or(end, |&(_, line)| line);
                hunks[h].lines[..next - first]
                    .iter()
                    .position(|line| {
                        let carried = if new_side {
                            line.new_line
                        } else {
                            line.old_line
                        };
                        carried == Some(number)
                    })
                    .map(|offset| row + 1 + offset)
            })
    }
}

/// What readers wrote on the lines laid out, as rows of the list: each
/// line's notes (its published comments, this reader's draft or the open
/// composer) follow it, a box drawn a row at a time. The diff's own rows
/// keep their indices ([`Rows`]); a list index is found through these.
#[derive(Default)]
struct Notes {
    /// in row order: the row of the line, and the rows after it
    zones: Vec<(usize, Vec<NoteRow>)>,
    /// how many note rows come before each zone, and after the last
    before: Vec<usize>,
    /// each zone's first list index
    starts: Vec<usize>,
    /// the list index of the open composer's last row, when it is drawn
    composer: Option<usize>,
}

/// One row of a note box: its key, what it holds, and whether it is the
/// box's top or bottom edge.
struct NoteRow {
    key: String,
    kind: NoteKind,
    first: bool,
    last: bool,
}

enum NoteKind {
    /// who wrote it (or where the composer anchors), and whether the
    /// review was pinned at another head
    Head(String, bool),
    Text(String),
    /// the composer's field, the line its error shows on, and its buttons
    Field,
    Error,
    Actions,
}

/// A list index: one of the diff's own rows, or a row of a note.
enum Slot<'a> {
    Row(usize),
    Note(&'a NoteRow),
}

impl Notes {
    fn of(forge: &Forge, rows: &Rows) -> Notes {
        let mut zones: BTreeMap<usize, Vec<NoteRow>> = BTreeMap::new();
        let mut place = |path: &[u8], new_side: bool, line: u64, rows_of_box: Vec<NoteRow>| {
            if let Some(row) = rows.row_of_anchor(path, new_side, line) {
                zones.entry(row).or_default().extend(rows_of_box);
            }
        };
        for (n, (path, new_side, line, author, body, outdated)) in
            published_comments(forge).into_iter().enumerate()
        {
            let text = wrap(&body).into_iter().map(NoteKind::Text);
            let rows_of_box = boxed(
                &format!("forge-diff-comment-{n}"),
                std::iter::once(NoteKind::Head(author, outdated)).chain(text),
            );
            place(&path, new_side, line, rows_of_box);
        }
        let review = forge.review();
        let open = review.and_then(|review| review.open.as_ref());
        for (n, draft) in review
            .iter()
            .flat_map(|review| &review.comments)
            .enumerate()
        {
            if open.is_some_and(|open| open.anchors(draft)) {
                continue;
            }
            let text = wrap(&draft.body.text()).into_iter().map(NoteKind::Text);
            let head = NoteKind::Head("Your pending comment".into(), false);
            let rows_of_box = boxed(
                &format!("forge-diff-draft-{n}"),
                std::iter::once(head).chain(text),
            );
            place(&draft.path, draft.new_side, draft.line, rows_of_box);
        }
        if let Some(open) = open {
            place(
                &open.path,
                open.new_side,
                open.line,
                boxed("forge-comment", composer_parts(open.anchor())),
            );
        }
        Notes::laid(zones)
    }

    fn laid(zones: BTreeMap<usize, Vec<NoteRow>>) -> Notes {
        let composes =
            |zone: &[NoteRow]| zone.iter().any(|note| matches!(note.kind, NoteKind::Field));
        let mut notes = Notes::default();
        let mut added = 0;
        for (row, rows_of_zone) in zones {
            notes.before.push(added);
            notes.starts.push(row + 1 + added);
            if composes(&rows_of_zone) {
                notes.composer = Some(row + added + rows_of_zone.len());
            }
            added += rows_of_zone.len();
            notes.zones.push((row, rows_of_zone));
        }
        notes.before.push(added);
        notes
    }

    /// The note rows in the list.
    fn added(&self) -> usize {
        self.before.last().copied().unwrap_or(0)
    }

    /// The note rows before zone `zone` (all of them past the last).
    fn added_before(&self, zone: usize) -> usize {
        self.before.get(zone).copied().unwrap_or(0)
    }

    /// The list index of the diff's row `row`.
    fn index_of(&self, row: usize) -> usize {
        let zones = self.zones.partition_point(|(after, _)| *after < row);
        row + self.added_before(zones)
    }

    /// What list index `index` draws.
    fn slot(&self, index: usize) -> Slot<'_> {
        let at = self.starts.partition_point(|start| *start <= index);
        if let Some(zone) = at.checked_sub(1)
            && let Some(note) = self.zones[zone].1.get(index - self.starts[zone])
        {
            return Slot::Note(note);
        }
        Slot::Row(index - self.added_before(at))
    }
}

/// The open composer's rows, under where it anchors: its field, its error's
/// line, its buttons, then a row of room. On rows one height the gaps are
/// rows too: the buttons clear the field by the error's line and the box's
/// edge by the last row, and typing or an error moves none of them.
fn composer_parts(anchor: String) -> [NoteKind; 5] {
    [
        NoteKind::Head(anchor, false),
        NoteKind::Field,
        NoteKind::Error,
        NoteKind::Actions,
        NoteKind::Text(String::new()),
    ]
}

/// A box's rows, keyed under `key`: its parts in order, the first its top
/// edge and the last its bottom.
fn boxed(key: &str, parts: impl IntoIterator<Item = NoteKind>) -> Vec<NoteRow> {
    let mut rows: Vec<NoteRow> = parts
        .into_iter()
        .enumerate()
        .map(|(n, kind)| NoteRow {
            key: format!("{key}-{n}"),
            kind,
            first: n == 0,
            last: false,
        })
        .collect();
    if let Some(last) = rows.last_mut() {
        last.last = true;
    }
    rows
}

/// `text` as the lines a note box draws, broken between words.
// ponytail: breaks by a character budget, not by the font's widths (a
// view cannot measure text); a line that still runs past the box is cut
// with an ellipsis, and the whole comment reads in the Conversation tab.
fn wrap(text: &str) -> Vec<String> {
    let width = |word: &str| -> usize {
        word.chars()
            .map(|c| if c >= '\u{1100}' { 2 } else { 1 })
            .sum()
    };
    let mut lines = Vec::new();
    for paragraph in text.trim_end().split('\n') {
        let mut line = String::new();
        let mut used = 0;
        for word in paragraph.split(' ') {
            let w = width(word);
            if used > 0 && used + 1 + w > NOTE_CHARS {
                lines.push(std::mem::take(&mut line));
                used = 0;
            }
            if used > 0 {
                line.push(' ');
                used += 1;
            }
            line.push_str(word);
            used += w;
        }
        lines.push(line);
    }
    lines
}

/// The open composer's field and the routes of its buttons, for the rows
/// that draw it.
struct Composer {
    body: TextField,
    error: String,
    stage: Route<()>,
    cancel: Route<()>,
}

/// Scrolls the open composer into view: it follows its line, and its field
/// must be in the frame that gives it the keys.
pub(crate) fn reveal_composer(forge: &Forge) {
    let Some(shared) = forge.diff_query().and_then(|query| forge.shared(&query)) else {
        return;
    };
    let rows = forge
        .diff_cache
        .rows(&shared, forge.nav().diff_path.clone());
    if let Some(last) = Notes::of(forge, &rows).composer {
        forge
            .diff_scroll
            .scroll_to_item(last, ScrollStrategy::Nearest);
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
    ladder: &Ladder,
) -> AnyElement {
    let ladder = *ladder;
    let reply = match staged(
        forge,
        query,
        element_id,
        "Reading the diff…",
        cx,
        theme,
        &ladder,
    ) {
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
    let shown = |file: &FileDiff| only.is_none() || only.as_deref() == path_of(file).as_deref();
    if !page.items.iter().any(shown) {
        // Expo's: a change's diff `send` (`forge/change.tsx:342`), a
        // commit's `git-commit-horizontal` (`commit/[oid].tsx:96`)
        let glyph = match reviewable {
            true => "send",
            false => "git-commit-horizontal",
        };
        return empty_state(
            id(format!("{element_id}-empty")),
            glyph,
            "No changes",
            if *total_files == 0 {
                "These endpoints hold the same tree."
            } else {
                "No file here matches what is selected."
            },
            &ladder,
            theme,
        )
        .into_any_element();
    }
    // A screen with no file tree beside it (a commit's own diff) carries the
    // file headers above the virtual list, where they stay in view. It lists
    // every file, the one shown alone marked, as the Files tab's list does:
    // a press narrows the diff, not the list.
    let strip =
        (!reviewable).then(|| file_strip(shared.clone(), page.items.len(), forge, cx, theme));
    let rows = forge.diff_cache.rows(&shared, only);
    // what readers have written on these lines, as rows after each line
    let notes = Rc::new(match reviewable {
        true => Notes::of(forge, &rows),
        false => Notes::default(),
    });
    let count = rows.count + notes.added();
    let theme = *theme;
    let comment: Route<Anchor> = Rc::new(cx.listener(|forge, at: &Anchor, window, cx| {
        forge.open_comment(at.0.clone(), at.1, at.2, window, cx)
    }));
    let composer = forge
        .review()
        .and_then(|review| Some((review.open.as_ref()?, &review.error)))
        .map(|(open, error)| Composer {
            body: open.body.clone(),
            error: error.clone(),
            stage: Rc::new(cx.listener(|forge, _: &(), _, cx| forge.stage_comment(cx))),
            cancel: Rc::new(cx.listener(|forge, _: &(), _, cx| forge.discard_comment(cx))),
        });
    let handle = forge.diff_scroll.clone();
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
    let slots = notes.clone();
    let list = crate::ui::components::rows(
        element_id,
        count,
        rows.widest.map(|row| notes.index_of(row)),
        Some(&handle),
        move |index| match slots.slot(index) {
            Slot::Note(note) => note_row(note, composer.as_ref(), &theme, &ladder),
            Slot::Row(row) => {
                let active = active
                    .filter(|(line, _)| *line == row)
                    .map(|(_, cell)| (cell, walked));
                paint_row(
                    &painted.painted(row),
                    row,
                    reviewable,
                    active,
                    &comment,
                    &theme,
                )
            }
        },
    );
    let list = match lines {
        0 => list,
        _ => {
            let reveal = rows.clone();
            let pressed = rows.clone();
            // An open composer is the grid's edit mode: the keys typed in
            // its field reach the grid around it too, where Space and Enter
            // would press the active gutter and move the composer away, so
            // the grid walks nothing until it is staged or cancelled.
            let walkable = if notes.composer.is_some() { 0 } else { lines };
            design::composite(id(format!("{element_id}-lines")), Role::Grid, "Diff")
                .active(at, walkable)
                .cells(cell, cells)
                .on_move(cx.processor(move |forge, index: usize, _, cx| {
                    forge.diff_cursor = Some((index, 0));
                    let row = notes.index_of(reveal.row_of_line(index));
                    forge
                        .diff_scroll
                        .scroll_to_item(row, ScrollStrategy::Nearest);
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

/// The file strip's rows, each this tall.
const STRIP_ROW_H: f32 = 21.;
/// The rows the strip shows before it scrolls, so a commit touching many
/// files still shows its diff beneath.
const STRIP_ROWS: usize = 8;
/// The strip's inset above and below its rows.
const STRIP_PAD: f32 = 4.;
/// The strip's list: its id, and its cursor's.
pub(crate) const STRIP_ID: &str = "forge-commit-diff-files";

/// Which files a commit's diff touches (`count` of them), above the rows
/// themselves. Each one shows its file alone; the commit's All files shows
/// them all again. A virtual list, so a commit touching a release's files
/// draws the ones on screen; one Tab stop: ↑ ↓ walk the files, Enter shows
/// the active one.
fn file_strip(
    shared: Rc<Reply>,
    count: usize,
    forge: &Forge,
    cx: &mut Context<Forge>,
    theme: &Theme,
) -> AnyElement {
    let (at, _) = crate::ui::components::cursor(forge, STRIP_ID);
    let active = at.min(count.saturating_sub(1));
    let walked = crate::ui::components::walked(forge, STRIP_ID);
    let selected = forge.nav().diff_path.clone();
    let pressed = shared.clone();
    let list = crate::ui::components::list(
        STRIP_ID,
        "Files",
        count,
        Some(&forge.strip_scroll),
        forge,
        cx,
        move |forge, index, _, cx| {
            if let Some(path) = file_at(&pressed, index).and_then(path_of) {
                forge.single_file(Some(path), cx)
            }
        },
    )
    .flex_none()
    .h(px(
        count.min(STRIP_ROWS) as f32 * STRIP_ROW_H + 2. * STRIP_PAD
    ))
    .px_2()
    .py(px(STRIP_PAD))
    .border_b_1()
    .border_color(theme.border);
    let pick: Route<Vec<u8>> = Rc::new(
        cx.listener(|forge, path: &Vec<u8>, _, cx| forge.single_file(Some(path.clone()), cx)),
    );
    let theme = *theme;
    let rows = crate::ui::components::rows(
        "forge-commit-diff-strip",
        count,
        None,
        Some(&forge.strip_scroll),
        move |index| {
            let file = file_at(&shared, index);
            let Some((file, path)) = file.and_then(|file| Some((file, path_of(file)?))) else {
                return div().h(px(STRIP_ROW_H)).into_any_element();
            };
            let label = path_text(&path);
            let shown = selected.as_deref() == Some(path.as_slice());
            let click = {
                let pick = pick.clone();
                move |_: &ClickEvent, window: &mut Window, app: &mut App| pick(&path, window, app)
            };
            let row = div()
                .id(id(format!("forge-commit-diff-file-{label}")))
                .w_full()
                .h(px(STRIP_ROW_H))
                .flex()
                .items_center()
                .gap_2()
                .text_size(design::text::SECONDARY)
                .when(shown, |row| row.bg(theme.surface_raised))
                .when(walked && index == active && !shown, |row| {
                    row.bg(theme.hover)
                })
                .aria_selected(shown)
                .on_click(click)
                .child(crate::ui::bold(label))
                .child(quiet(status_label(file.status), &theme))
                .children(counts(file).map(|counts| quiet(counts, &theme)));
            design::item(row, Role::ListBoxOption, index == active).into_any_element()
        },
    );
    list.child(rows).into_any_element()
}

/// File `index` of a diff reply.
pub(crate) fn file_at(reply: &Reply, index: usize) -> Option<&FileDiff> {
    match reply {
        Reply::Diff { page, .. } => page.items.get(index),
        _ => None,
    }
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

impl Painted {
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
    let status = status_label(file.status);
    match counts(file) {
        Some(counts) => format!("{header} · {status} · {counts}"),
        None => format!("{header} · {status}"),
    }
}

/// The lines a file adds and removes, when they were counted: a binary,
/// an oversize file or a submodule has none to count, not zero.
fn counts(file: &FileDiff) -> Option<String> {
    (file.content == Content::Text).then(|| format!("+{} −{}", file.additions, file.deletions))
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

/// One of the diff's own rows. Each is one line high, as the list draws
/// every row the height of the one it measures: a header padded taller
/// spilled into the row under it and was drawn over by it.
fn paint_row(
    row: &Painted,
    index: usize,
    reviewable: bool,
    active: Option<(usize, bool)>,
    comment: &Route<Anchor>,
    theme: &Theme,
) -> AnyElement {
    match row.kind {
        Kind::File => div()
            .id(id(format!("forge-diff-file-{}", path_text(&row.path))))
            .w_full()
            .px(design::space::BLOCK)
            .font_family(design::fonts::FAMILY_MONO)
            .text_size(design::text::CAPTION)
            .font_weight(view::FontWeight::MEDIUM)
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

/// `active`: which of the line's gutters the arrows are on, under a review,
/// and whether they have walked there: only then is it drawn.
fn line_row(
    row: &Painted,
    index: usize,
    reviewable: bool,
    active: Option<(usize, bool)>,
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
    div()
        .id(id(format!("forge-diff-line-{index}")))
        .w_full()
        .flex()
        .items_center()
        .gap_1()
        .px(design::space::BLOCK)
        .bg(background)
        .when(reviewable, |body| body.h(ROW_H).role(Role::Row))
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
            view::design::mono(row.text.clone())
                .flex_1()
                .min_w(px(0.))
                .text_color(colour),
        )
        .into_any_element()
}

/// One row of a note box, under the line it is on: the box's edges, and
/// what this row of it holds.
fn note_row(
    note: &NoteRow,
    composer: Option<&Composer>,
    theme: &Theme,
    ladder: &Ladder,
) -> AnyElement {
    let slice = div()
        .flex_1()
        .min_w(px(0.))
        .max_w(NOTE_W)
        .h(ROW_H)
        .flex()
        .items_center()
        .gap(design::space::SM)
        .px(design::space::MD)
        .bg(theme.background)
        .border_l_1()
        .border_r_1()
        .border_color(theme.border_strong)
        .when(note.first, |slice| slice.border_t_1())
        .when(note.last, |slice| slice.border_b_1());
    let slice = match (&note.kind, composer) {
        (NoteKind::Head(who, outdated), _) => slice
            .text_size(design::text::SECONDARY)
            .text_color(theme.muted)
            .child(who.clone())
            .when(*outdated, |slice| {
                slice.child(badge(
                    id(format!("{}-outdated", note.key)),
                    "outdated",
                    ladder,
                    theme.warning,
                    theme.warning_soft,
                ))
            }),
        (NoteKind::Text(text), _) => {
            slice.child(div().flex_1().min_w(px(0.)).truncate().child(text.clone()))
        }
        (NoteKind::Field, Some(composer)) => {
            let stage = composer.stage.clone();
            slice.child(
                Input::new(id("forge-comment-body"), &composer.body, "Line comment")
                    .h(ROW_H - px(4.))
                    .flex_1()
                    .px_2()
                    .border_1()
                    .border_color(theme.border_strong)
                    .bg(theme.background)
                    .text_size(design::text::SECONDARY)
                    .text_color(theme.foreground)
                    .placeholder("What should change here?")
                    .on_submit(move |_: &(), window: &mut Window, app: &mut App| {
                        stage(&(), window, app)
                    }),
            )
        }
        (NoteKind::Error, Some(composer)) => slice.child(
            div()
                .id(id("forge-comment-error"))
                .flex_1()
                .min_w(px(0.))
                .truncate()
                .text_size(design::text::SECONDARY)
                .text_color(theme.danger)
                .child(composer.error.clone()),
        ),
        (NoteKind::Actions, Some(composer)) => {
            let (stage, cancel) = (composer.stage.clone(), composer.cancel.clone());
            slice
                .justify_end()
                .line_height(BUTTON_LINE)
                .child(crate::ui::components::button(
                    id("forge-comment-cancel"),
                    "Cancel",
                    ladder,
                    theme,
                    move |_: &ClickEvent, window: &mut Window, app: &mut App| {
                        cancel(&(), window, app)
                    },
                ))
                .child(
                    crate::ui::components::button(
                        id("forge-comment-save"),
                        "Stage",
                        ladder,
                        theme,
                        move |_: &ClickEvent, window: &mut Window, app: &mut App| {
                            stage(&(), window, app)
                        },
                    )
                    .kind(design::Kind::Primary),
                )
        }
        (NoteKind::Field | NoteKind::Error | NoteKind::Actions, None) => slice,
    };
    div()
        .id(id(note.key.clone()))
        .w_full()
        .h(ROW_H)
        .flex()
        // under the code, past the gutters and the marker
        .pl(GUTTER_W * 2. + MARKER_W + design::space::BLOCK + design::space::SM)
        .pr(design::space::BLOCK)
        .child(slice)
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
        assert_eq!((rows.count, rows.lines), (9, 3));
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
                (Kind::File, "b.bin · added".into()),
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

    /// A comment sits on the line whose gutter on its side carries its
    /// number: a context line by either, an added line by its new number
    /// alone, a removed line by its old; never another file's line.
    #[test]
    fn a_note_finds_its_line() {
        let line = |kind, old, new| forge::DiffLine {
            kind,
            old_line: old,
            new_line: new,
            bytes: b"x\n".to_vec(),
        };
        let mut edited = file("a.rs", &[]);
        edited.hunks[0].lines = vec![
            line(LineKind::Context, Some(1), Some(1)),
            line(LineKind::Deleted, Some(2), None),
            line(LineKind::Added, None, Some(2)),
        ];
        let rows = Rows::layout(reply(vec![edited, file("b.rs", &["one"])]), None);
        let of = |path: &[u8], new_side, number| rows.row_of_anchor(path, new_side, number);
        assert_eq!(of(b"a.rs", false, 1), Some(2));
        assert_eq!(of(b"a.rs", true, 1), Some(2));
        assert_eq!(of(b"a.rs", false, 2), Some(3));
        assert_eq!(of(b"a.rs", true, 2), Some(4));
        assert_eq!(of(b"a.rs", true, 3), None);
        assert_eq!(of(b"b.rs", true, 1), Some(7));
        assert_eq!(of(b"c.rs", true, 1), None);
        assert_eq!(anchors(&rows.painted(2)).len(), 2, "both gutters");
    }

    /// Note rows follow their line in the list and the diff's own rows keep
    /// theirs: a list index finds either, and a row finds its list index.
    #[test]
    fn note_rows_follow_their_line() {
        let zone = |rows: usize| boxed("n", (0..rows).map(|_| NoteKind::Text(String::new())));
        let notes = Notes::laid(BTreeMap::from([(2, zone(3)), (5, zone(2))]));
        assert_eq!(notes.added(), 5);
        let drawn: Vec<String> = (0..12)
            .map(|index| match notes.slot(index) {
                Slot::Row(row) => format!("r{row}"),
                Slot::Note(note) => note.key.clone(),
            })
            .collect();
        assert_eq!(
            drawn,
            [
                "r0", "r1", "r2", "n-0", "n-1", "n-2", "r3", "r4", "r5", "n-0", "n-1", "r6"
            ]
        );
        for (row, index) in [(0, 0), (2, 2), (3, 6), (5, 8), (6, 11)] {
            assert_eq!(notes.index_of(row), index, "row {row}");
        }
    }

    #[test]
    fn every_note_is_laid_after_its_line() {
        let comment = |key: &str| {
            boxed(
                key,
                [
                    NoteKind::Head(String::new(), false),
                    NoteKind::Text(String::new()),
                ],
            )
        };
        let composer = boxed("c", composer_parts(String::new()));
        let mut two = comment("a");
        two.extend(comment("b"));
        let zones = BTreeMap::from([
            (2, two),
            (5, comment("d")),
            (7, composer),
            (8, comment("e")),
        ]);
        let notes = Notes::laid(zones);
        let drawn: Vec<String> = (0..23)
            .map(|index| match notes.slot(index) {
                Slot::Row(row) => format!("r{row}"),
                Slot::Note(note) => note.key.clone(),
            })
            .collect();
        assert_eq!(
            drawn,
            [
                "r0", "r1", "r2", "a-0", "a-1", "b-0", "b-1", "r3", "r4", "r5", "d-0", "d-1", "r6",
                "r7", "c-0", "c-1", "c-2", "c-3", "c-4", "r8", "e-0", "e-1", "r9"
            ]
        );
        assert_eq!(notes.composer, Some(18));
    }

    /// A note's text breaks between words under the budget, keeps its own
    /// line breaks, and counts a wide character twice.
    #[test]
    fn a_note_breaks_between_words() {
        let words = "word ".repeat(30);
        let lines = wrap(&format!("{words}\nnext"));
        assert!(lines.iter().all(|line| line.chars().count() <= NOTE_CHARS));
        assert_eq!(lines.len(), 4, "{lines:?}");
        assert_eq!(lines.last().map(String::as_str), Some("next"));
        let wide = "\u{d55c}".repeat(NOTE_CHARS / 2 + 1);
        assert_eq!(wrap(&format!("{wide} a")), [wide.clone(), "a".to_owned()]);
    }
}
