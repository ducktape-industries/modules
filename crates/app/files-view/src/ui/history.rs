//! The history: every snapshot newest first, each opening its page, then
//! the pins that name some of them; and one snapshot's page, who made it
//! and when, and what it changed against its parent. Reads the history,
//! the authors' names and a snapshot's changes; writes the route.
use files::{PinRow, SnapshotRow};
use view::design::{date, grouped};

use super::*;
use crate::state::{Changed, History};

pub(super) fn history(view: &Files, cx: Cx, k: &Kit) -> AnyElement {
    let history = match &view.history {
        Loadable::Ready(history) | Loadable::Reloading(history, _) => history,
        Loadable::Failed(refusal) => return refused("files-history", refusal, cx, k),
        Loadable::Idle | Loadable::Loading(_) => {
            return reading("files-history-reading", words::READING_HISTORY, k);
        }
    };
    div()
        .id("files-history")
        .flex()
        .flex_col()
        .child(snapshots(view, history, cx, k))
        .child(heading(
            "files/pins-head",
            words::PINS,
            Some(grouped(history.pins.len() as u64)),
            k,
        ))
        .child(pins(view, history, cx, k))
        .into_any_element()
}

fn snapshots(view: &Files, history: &History, cx: Cx, k: &Kit) -> AnyElement {
    if history.snapshots.is_empty() {
        return design::empty_state(
            "files-no-snapshots",
            words::NO_SNAPSHOTS,
            words::NO_SNAPSHOTS_DETAIL,
            &k.theme,
        )
        .into_any_element();
    }
    let older = history.older.then(|| {
        design::quiet(words::OLDER, &k.theme)
            .id("files-older")
            .px_5()
            .py_3()
    });
    div()
        .flex()
        .flex_col()
        .child(heading("files/snapshots-head", words::HISTORY, None, k))
        .children(
            history
                .snapshots
                .iter()
                .map(|snapshot| snapshot_row(view, snapshot, cx, k)),
        )
        .children(older)
        .into_any_element()
}

/// One snapshot: its message, who made it and when.
pub(super) fn snapshot_row(view: &Files, snapshot: &SnapshotRow, cx: Cx, k: &Kit) -> Stateful<Div> {
    let seq = snapshot.seq;
    let open = cx.listener(move |view: &mut Files, _: &ClickEvent, _, cx| {
        view.open(Route::Snapshot(seq), cx)
    });
    let aside = format!(
        "{} · {}",
        view.author(&snapshot.author),
        date(snapshot.time)
    );
    row(
        format!("files/snapshot/{seq}"),
        "git-commit-horizontal",
        words::message_or_none(&snapshot.message).to_owned(),
        aside,
        k,
        open,
    )
}

fn pins(view: &Files, history: &History, cx: Cx, k: &Kit) -> AnyElement {
    if history.pins.is_empty() {
        return design::quiet(words::NO_PINS, &k.theme)
            .id("files-no-pins")
            .px_5()
            .py_3()
            .into_any_element();
    }
    let rows = history
        .pins
        .iter()
        .map(|pin| pin_row(view, history, pin, cx, k));
    div().flex().flex_col().children(rows).into_any_element()
}

/// One pin: its name, and the snapshot it names, which opens when the
/// history lists it.
fn pin_row(view: &Files, history: &History, pin: &PinRow, cx: Cx, k: &Kit) -> AnyElement {
    let named = history.snapshots.iter().find(|row| row.id == pin.snapshot);
    let aside = format!(
        "{} · {}",
        named.map_or_else(
            || words::OLDER_SNAPSHOT.to_owned(),
            |row| words::snapshot(row.seq)
        ),
        view.author(&pin.owner)
    );
    let id = format!("files/pin/{}", pin.name);
    match named {
        Some(row) => {
            let seq = row.seq;
            let open = cx.listener(move |view: &mut Files, _: &ClickEvent, _, cx| {
                view.open(Route::Snapshot(seq), cx)
            });
            super::row(id, "pin", pin.name.clone(), aside, k, open).into_any_element()
        }
        None => row_look(id, "pin", pin.name.clone(), aside, k).into_any_element(),
    }
}

/// One snapshot's page.
pub(super) fn snapshot(view: &Files, seq: u64, cx: Cx, k: &Kit) -> AnyElement {
    let page = div().id("files-snapshot").flex().flex_col();
    let Some(row) = view.snapshot(seq) else {
        let waiting = match &view.history {
            Loadable::Failed(refusal) => refused("files-history", refusal, cx, k),
            Loadable::Ready(_) | Loadable::Reloading(..) => {
                design::quiet(words::NOT_LISTED, &k.theme)
                    .id("files-not-listed")
                    .px_5()
                    .py_4()
                    .into_any_element()
            }
            Loadable::Idle | Loadable::Loading(_) => {
                reading("files-snapshot-reading", words::READING_HISTORY, k)
            }
        };
        return page.child(waiting).into_any_element();
    };
    page.child(title(
        "files-snapshot-title",
        words::message_or_none(&row.message),
        Some(words::snapshot(seq)),
        k,
    ))
    .child(line(words::AUTHOR, view.author(&row.author), k))
    .child(line(words::TIME, date(row.time), k))
    .child(line(words::BLOCK, data(grouped(row.height)), k))
    .child(line(words::SNAPSHOT, data(abi::hex(row.id.digest())), k))
    .child(heading("files/changes-head", words::CHANGES, None, k))
    .child(changes(view, row, cx, k))
    .into_any_element()
}

/// What the snapshot changed, path by path.
fn changes(view: &Files, row: &SnapshotRow, cx: Cx, k: &Kit) -> AnyElement {
    if row.parent.is_none() {
        return design::quiet(words::FIRST_SNAPSHOT, &k.theme)
            .id("files-first")
            .px_5()
            .py_3()
            .into_any_element();
    }
    let changed: Option<&Changed> = view
        .changed
        .ready()
        .filter(|changed| changed.seq == row.seq);
    let Some(changed) = changed else {
        return match view.changed.failed() {
            Some(refusal) => refused("files-changes", refusal, cx, k),
            None => reading("files-changes-reading", words::READING_CHANGES, k),
        };
    };
    if changed.entries.is_empty() {
        return design::quiet(words::NOTHING_CHANGED, &k.theme)
            .id("files-unchanged")
            .px_5()
            .py_3()
            .into_any_element();
    }
    let lines = changed.entries.iter().map(|entry| {
        div()
            .min_h(ROW_H)
            .flex()
            .items_center()
            .gap_3()
            .px_5()
            .border_b_1()
            .border_color(k.theme.border)
            .child(
                div()
                    .w(LABEL_W)
                    .flex_none()
                    .text_size(design::text::SECONDARY)
                    .text_color(k.theme.muted)
                    .child(words::diff_kind(entry.kind)),
            )
            .child(data(entry.path.clone()))
    });
    div()
        .id("files-changes")
        .flex()
        .flex_col()
        .children(lines)
        .into_any_element()
}
