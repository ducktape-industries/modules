//! A file: the path as crumbs, its name over its kind and size, the writes
//! the reader may make to it, then its text, where a link points, or how
//! big it is when it is not text. Reads the file; writes the route and
//! opens a write's form.
use files::Node;

use super::*;
use crate::state::{Opened, name_of};

pub(super) fn file(view: &Files, path: &str, cx: Cx, k: &Kit) -> AnyElement {
    let page = div()
        .id("files-file")
        .flex()
        .flex_col()
        .child(crumbs(path, cx, k));
    let Some(file) = view.file_at(path) else {
        let waiting = match view.file.failed() {
            Some(refusal) => refused("files-file", refusal, cx, k),
            None => reading("files-file-reading", words::READING_FILE, k),
        };
        return page.child(waiting).into_any_element();
    };
    let Some(entry) = file.file() else {
        return page
            .child(design::empty_state(
                "files-gone",
                name_of(path).to_owned(),
                words::GONE,
                &k.theme,
            ))
            .into_any_element();
    };
    let under = format!(
        "{} · {}",
        words::kind(&entry.node, file.text().is_some()),
        words::size(file.size())
    );
    page.child(title("files-file-title", name_of(path), Some(under), k))
        .children(writes(view, cx, k))
        .child(body(file, &entry.node, k))
        .into_any_element()
}

fn body(file: &Opened, node: &Node, k: &Kit) -> AnyElement {
    let body = div().id("files-file-body").px_5().py_3();
    if let Node::Symlink { target } = node {
        return body
            .child(line(words::TARGET, data(target.clone()), k))
            .into_any_element();
    }
    let Some(text) = file.text() else {
        return body
            .text_color(k.theme.muted)
            .child(words::not_text(file.size()))
            .into_any_element();
    };
    let cut = (!file.whole).then(|| {
        div()
            .pb_2()
            .text_size(design::text::SECONDARY)
            .text_color(k.theme.muted)
            .child(words::cut(file.size()))
    });
    body.children(cut)
        .child(
            div()
                .id("files-text")
                .p_3()
                .border_1()
                .border_color(k.theme.border)
                .bg(k.theme.surface)
                .font_family(design::fonts::FAMILY_MONO)
                .text_size(design::text::MONO)
                .child(text.to_owned()),
        )
        .into_any_element()
}
