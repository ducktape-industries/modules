//! A snapshot's screen: who made it, when, at which block, its id, then
//! what it changed against its parent, path by path. New design in the
//! kit's look; nearest Expo screen
//! `mobile/app/(tabs)/forge/[repo]/code/[...path].tsx`. Reads the history
//! and the snapshot's changes; writes Back.
use files::SnapshotRow;
use view::design::{date, grouped};

use super::*;

pub(super) fn snapshot(view: &Files, seq: u64, cx: Cx, k: &Kit) -> AnyElement {
    let page = page("files-snapshot", words::snapshot(seq), view, cx, k);
    let body = match view.snapshot(seq) {
        Some(row) => found(view, row, cx, k),
        None => match &view.history {
            Loadable::Failed(refusal) => refused("files-history", refusal, cx, k),
            Loadable::Ready(_) | Loadable::Reloading(..) => {
                quiet("files-not-listed", words::NOT_LISTED, k)
            }
            Loadable::Idle | Loadable::Loading(_) => {
                quiet("files-snapshot-reading", words::READING_HISTORY, k)
            }
        },
    };
    page.child(scroller("files-page").child(body))
        .into_any_element()
}

fn found(view: &Files, row: &SnapshotRow, cx: Cx, k: &Kit) -> AnyElement {
    div()
        .flex()
        .flex_col()
        .child(
            div()
                .id("files-snapshot-message")
                .p(space::BLOCK)
                .text_type(k.ladder.heading)
                .font_weight(FontWeight::MEDIUM)
                .child(words::message_or_none(&row.message).to_owned()),
        )
        .child(key_value(words::AUTHOR, view.author(&row.author), k))
        .child(key_value(words::TIME, date(row.time), k))
        .child(key_value(words::BLOCK, data(grouped(row.height), k), k))
        .child(key_value(
            words::SNAPSHOT,
            data(abi::hex(row.id.digest()), k),
            k,
        ))
        .child(design::section_head(
            "files/changes-head",
            words::CHANGES,
            &k.ladder,
            &k.theme,
        ))
        .child(changes(view, row, cx, k))
        .into_any_element()
}

/// What the snapshot changed, path by path.
fn changes(view: &Files, row: &SnapshotRow, cx: Cx, k: &Kit) -> AnyElement {
    if row.parent.is_none() {
        return quiet("files-first", words::FIRST_SNAPSHOT, k);
    }
    let Some(changed) = view
        .changed
        .ready()
        .filter(|changed| changed.seq == row.seq)
    else {
        return match view.changed.failed() {
            Some(refusal) => refused("files-changes", refusal, cx, k),
            None => quiet("files-changes-reading", words::READING_CHANGES, k),
        };
    };
    if changed.entries.is_empty() {
        return quiet("files-unchanged", words::NOTHING_CHANGED, k);
    }
    div()
        .id("files-changes")
        .flex()
        .flex_col()
        .children(changed.entries.iter().map(|entry| {
            design::row(
                ElementId::Name(format!("files/change{}", entry.path).into()),
                &k.ladder,
                &k.theme,
            )
            .min_h(k.ladder.row_tall)
            .child(
                title_and_caption(data(entry.path.clone(), k).truncate())
                    .child(row_caption(words::diff_kind(entry.kind), k)),
            )
        }))
        .into_any_element()
}
