//! The history screen: every snapshot, newest first, each opening its
//! screen, then the pins that name some of them. New design in the kit's
//! look; nearest Expo screen
//! `mobile/app/(tabs)/forge/[repo]/code/[...path].tsx` (its rows). Reads
//! the history and the authors' names; writes the route.
use files::{PinRow, SnapshotRow};
use view::design::{date, grouped};

use super::*;
use crate::state::History;

pub(super) fn history(view: &Files, cx: Cx, k: &Kit) -> AnyElement {
    let body = match &view.history {
        Loadable::Ready(history) | Loadable::Reloading(history, _) => listed(view, history, cx, k),
        Loadable::Failed(refusal) => refused("files-history", refusal, cx, k),
        Loadable::Idle | Loadable::Loading(_) => {
            quiet("files-history-reading", words::READING_HISTORY, k)
        }
    };
    page("files-history", words::HISTORY, view, cx, k)
        .child(scroller("files-page").child(body))
        .into_any_element()
}

fn listed(view: &Files, history: &History, cx: Cx, k: &Kit) -> AnyElement {
    if history.snapshots.is_empty() {
        return empty(
            "files-no-snapshots",
            words::NO_SNAPSHOTS,
            words::NO_SNAPSHOTS_DETAIL,
            k,
        );
    }
    let older = history.older.then(|| quiet("files-older", words::OLDER, k));
    let pins = match history.pins.is_empty() {
        true => quiet("files-no-pins", words::NO_PINS, k),
        false => div()
            .flex()
            .flex_col()
            .children(
                history
                    .pins
                    .iter()
                    .map(|pin| pin_row(view, history, pin, cx, k)),
            )
            .into_any_element(),
    };
    div()
        .flex()
        .flex_col()
        .children(
            history
                .snapshots
                .iter()
                .map(|snapshot| snapshot_row(view, snapshot, cx, k)),
        )
        .children(older)
        .child(
            design::section_head("files/pins-head", words::PINS, &k.ladder, &k.theme)
                .child(grouped(history.pins.len() as u64)),
        )
        .child(pins)
        .into_any_element()
}

/// One snapshot: its message over who made it and when.
fn snapshot_row(view: &Files, snapshot: &SnapshotRow, cx: Cx, k: &Kit) -> Stateful<Div> {
    let seq = snapshot.seq;
    let open = cx.listener(move |view: &mut Files, _: &ClickEvent, _, cx| {
        view.open(Route::Snapshot(seq), cx)
    });
    let caption = format!(
        "{} · {}",
        view.author(&snapshot.author),
        date(snapshot.time)
    );
    opening_row(
        ElementId::Name(format!("files/snapshot/{seq}").into()),
        "git-commit-horizontal",
        words::message_or_none(&snapshot.message).to_owned(),
        caption,
        k,
        open,
    )
}

/// One pin: its name over the snapshot it names, which opens when the
/// history lists it.
fn pin_row(view: &Files, history: &History, pin: &PinRow, cx: Cx, k: &Kit) -> Stateful<Div> {
    let named = history.snapshots.iter().find(|row| row.id == pin.snapshot);
    let caption = format!(
        "{} · {}",
        named.map_or_else(
            || words::OLDER_SNAPSHOT.to_owned(),
            |row| words::snapshot(row.seq)
        ),
        view.author(&pin.owner)
    );
    let id = ElementId::Name(format!("files/pin/{}", pin.name).into());
    match named {
        Some(row) => {
            let seq = row.seq;
            let open = cx.listener(move |view: &mut Files, _: &ClickEvent, _, cx| {
                view.open(Route::Snapshot(seq), cx)
            });
            opening_row(id, "pin", pin.name.clone(), caption, k, open)
        }
        None => still_row(id, "pin", pin.name.clone(), caption, k),
    }
}
