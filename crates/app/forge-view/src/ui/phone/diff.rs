//! One file of a change's diff (Expo
//! `app/(tabs)/forge/[repo]/changes/[n]/diff.tsx`, board 70): the file's
//! name over the change and where the file stands among its files, ‹ ›
//! to walk them; its lines with what readers wrote on them
//! ([`super::lines`]); the review bar at the foot.
//!
//! Reads the change's diff, its reviews and the review being written;
//! writes the review: Start, a comment at a pressed line number, Discard,
//! and Finish (the sheet [`super::review`]).
use view::Div;
use view::design::{Kind, Ladder};
use view::prelude::*;

use super::lines::{self, Notes, Published, Shown};
use super::parts::{self, Back};
use crate::id;
use crate::select::path_of;
use crate::state::change_key;
use crate::words::{self, path_text};
use crate::{Forge, Stage};
use forge::{FileDiff, Reply, Side};

pub(super) fn render(
    forge: &Forge,
    back: Back,
    cx: &mut Context<Forge>,
    ladder: &Ladder,
    theme: &Theme,
) -> AnyElement {
    let path = forge.nav().diff_path.clone().unwrap_or_default();
    let files = diff_files(forge);
    let at = files
        .iter()
        .position(|file| path_of(file).as_deref() == Some(&path));
    let n = forge.nav().change.unwrap_or_default();
    let title = match forge.change() {
        Some((change, ..)) => format!("{} · #{n}", change.title),
        None => format!("#{n}"),
    };
    let caption = match at {
        Some(at) => format!("{title} · {} of {}", at + 1, files.len()),
        None => title,
    };
    let name = path_text(&path);
    let name = name.rsplit('/').next().unwrap_or_default().to_owned();
    let key = change_key(&forge.repo_name(), n);
    design::page("forge-page-file", name, back, ladder, theme)
        .caption(caption)
        .trailing(walk(
            "forge-file-previous",
            ("chevron-left", "Previous file"),
            at.and_then(|at| at.checked_sub(1)),
            &files,
            cx,
            ladder,
            theme,
        ))
        .trailing(walk(
            "forge-file-next",
            ("chevron-right", "Next file"),
            at.map(|at| at + 1),
            &files,
            cx,
            ladder,
            theme,
        ))
        .children(parts::notices(forge, ladder, theme))
        .children(parts::pending(forge, &key, ladder, theme))
        .child(body(forge, &path, cx, ladder, theme))
        .child(review_bar(forge, cx, ladder, theme))
        .into_any_element()
}

/// The files of the change's diff, once read.
fn diff_files(forge: &Forge) -> Vec<&FileDiff> {
    match forge.diff_query().and_then(|query| forge.ready(&query)) {
        Some(Reply::Diff { page, .. }) => page.items.iter().collect(),
        _ => Vec::new(),
    }
}

/// ‹ or ›: the file `to` walks to, off at either end.
fn walk(
    key: &'static str,
    (glyph, name): (&str, &'static str),
    to: Option<usize>,
    files: &[&FileDiff],
    cx: &mut Context<Forge>,
    ladder: &Ladder,
    theme: &Theme,
) -> impl IntoElement {
    let next = to
        .and_then(|to| files.get(to))
        .and_then(|file| path_of(file));
    let enabled = next.is_some();
    let go = cx.listener(move |forge, _: &ClickEvent, _, cx| {
        if let Some(path) = &next {
            forge.single_file(Some(path.clone()), cx);
        }
    });
    design::icon_button(
        id(key),
        design::icon(glyph, ladder.icon),
        name,
        ladder,
        theme,
        go,
    )
    .enabled(enabled)
}

/// The file's lines once the diff is read; until then why not.
fn body(
    forge: &Forge,
    path: &[u8],
    cx: &mut Context<Forge>,
    ladder: &Ladder,
    theme: &Theme,
) -> AnyElement {
    let Some(query) = forge.diff_query() else {
        let waiting = match forge.change() {
            Some(_) => words::COMPARING_ENDPOINTS,
            None => words::READING_CHANGE,
        };
        return parts::quiet(id("forge-diff-waiting"), waiting, ladder, theme)
            .flex_1()
            .into_any_element();
    };
    let reply = match forge.stage(&query) {
        Stage::Ready(reply) => reply,
        _ => {
            let state = parts::stage(
                forge,
                &query,
                "forge-diff",
                words::READING_DIFF,
                cx,
                (ladder, theme),
            )
            .err();
            return div().flex_1().children(state).into_any_element();
        }
    };
    let Reply::Diff { page, .. } = reply else {
        return div().flex_1().into_any_element();
    };
    let files: Vec<&FileDiff> = page
        .items
        .iter()
        .filter(|file| path_of(file).as_deref() == Some(path))
        .collect();
    let notes = notes(forge);
    let shown = Shown {
        key: "forge-diff",
        page: lines::page_of((std::ptr::from_ref(reply) as usize, path)),
        rows: lines::diff_rows(&files, &notes),
        head: None,
        wrapped: false,
    };
    lines::block(shown, forge, cx, ladder, theme)
}

/// What readers wrote on the change's lines: every published comment,
/// against the head on screen; my staged ones and the open composer; the
/// gutter a comment button while I hold an account and nothing of mine
/// is on its way.
fn notes(forge: &Forge) -> Notes {
    let reviews = forge
        .change()
        .map(|(.., reviews)| reviews.items.as_slice())
        .unwrap_or_default();
    let published = reviews
        .iter()
        .flat_map(|review| {
            let author = forge.principal_name(&review.author);
            let outdated = forge.outdated(&review.draft.commit_oid);
            review.draft.comments.iter().map(move |comment| Published {
                anchor: (
                    comment.path.clone(),
                    comment.side == Side::New,
                    comment.line,
                ),
                author: author.clone(),
                body: comment.body.clone(),
                height: review.height,
                outdated,
            })
        })
        .collect();
    let review = forge.review();
    let staged = review
        .map(|review| {
            review
                .comments
                .iter()
                .map(|comment| {
                    (
                        (comment.path.clone(), comment.new_side, comment.line),
                        comment.body.text(),
                    )
                })
                .collect()
        })
        .unwrap_or_default();
    let open = review
        .and_then(|review| review.open.as_ref())
        .map(|open| (open.path.clone(), open.new_side, open.line));
    Notes {
        commentable: forge.me_principal().is_some() && !parts::sending(forge),
        published,
        staged,
        open,
    }
}

/// The review bar: under a review, `Reviewing · N staged`, Discard and
/// Finish review; else how to comment, and Start review. 16 in, 10 over
/// and under, a hairline above.
fn review_bar(forge: &Forge, cx: &mut Context<Forge>, ladder: &Ladder, theme: &Theme) -> Div {
    let bar = div()
        .flex()
        .flex_none()
        .items_center()
        .gap(design::space::SM)
        .px(design::space::BLOCK)
        .py(design::space::MD)
        .border_t_1()
        .border_color(theme.border);
    let said = |text: String| {
        div()
            .flex_1()
            .text_type(ladder.secondary)
            .text_color(theme.muted)
            .child(text)
    };
    let Some(review) = forge.review() else {
        let start = cx.listener(|forge, _: &ClickEvent, _, cx| forge.start_review(cx));
        let startable = forge.me_principal().is_some()
            && forge.change().is_some_and(|(_, head, ..)| head.is_some())
            && forge.compare().is_some();
        return bar
            .child(said(
                "Comment on any line by pressing its number.".to_owned(),
            ))
            .child(
                design::button(
                    id("forge-start-review"),
                    words::START_REVIEW,
                    ladder,
                    theme,
                    start,
                )
                .kind(Kind::Outline)
                .enabled(startable),
            );
    };
    let discard = cx.listener(|forge, _: &ClickEvent, _, cx| forge.cancel_review(cx));
    let finish = cx.listener(|forge, _: &ClickEvent, _, cx| forge.finishing(true, cx));
    bar.child(said(format!(
        "Reviewing · {} staged",
        review.comments.len()
    )))
    .child(
        design::button(
            id("forge-discard-review"),
            "Discard",
            ladder,
            theme,
            discard,
        )
        .kind(Kind::Quiet)
        .enabled(!parts::sending(forge)),
    )
    .child(
        design::button(
            id("forge-finish-review"),
            "Finish review",
            ladder,
            theme,
            finish,
        )
        .kind(Kind::Outline)
        .enabled(!parts::sending(forge)),
    )
}
