//! A folder: the path as crumbs, the writes the reader may make here, then
//! its entries, each opening its folder or file. Reads the folder; writes
//! the route and opens a write's form.
use files::EntryInfo;
use view::{Div, Stateful};

use super::*;
use crate::state::{name_of, node_glyph};

pub(super) fn folder(view: &Files, path: &str, cx: Cx, k: &Kit) -> AnyElement {
    let page = div()
        .id("files-folder")
        .flex()
        .flex_col()
        .child(crumbs(path, cx, k));
    if let Some(folder) = view.folder_at(path) {
        let writable = view.reader.writes(&folder.children());
        let entries = entries(&folder.entries, writable, cx, k);
        return page
            .children(writes(view, cx, k))
            .child(entries)
            .into_any_element();
    }
    let waiting = match view.folder.failed() {
        Some(refusal) => refused("files-folder", refusal, cx, k),
        None => reading("files-folder-reading", words::READING_FOLDER, k),
    };
    page.child(waiting).into_any_element()
}

fn entries(entries: &[EntryInfo], writable: bool, cx: Cx, k: &Kit) -> AnyElement {
    if entries.is_empty() {
        let detail = words::empty_folder(writable);
        return design::empty_state(
            "files-empty",
            "folder-open",
            words::EMPTY_FOLDER,
            detail,
            &k.ladder,
            &k.theme,
        )
        .into_any_element();
    }
    // ponytail: every entry is drawn; a uniform_list once folders hold
    // more than a screenful of hundreds
    let rows = entries.iter().map(|entry| entry_row(entry, cx, k));
    div()
        .id("files-entries")
        .mt_2()
        .border_t_1()
        .border_color(k.theme.border)
        .flex()
        .flex_col()
        .children(rows)
        .into_any_element()
}

fn entry_row(entry: &EntryInfo, cx: Cx, k: &Kit) -> Stateful<Div> {
    let route = Route::of(entry);
    let open =
        cx.listener(move |view: &mut Files, _: &ClickEvent, _, cx| view.open(route.clone(), cx));
    let name = name_of(&entry.path).to_owned();
    row(
        format!("files/entry/{name}"),
        node_glyph(&entry.node),
        name,
        words::entry_caption(&entry.node),
        k,
        open,
    )
}
