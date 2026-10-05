//! A change's details, one sidebar beside its conversation: who reviewed
//! it and how, what stands between it and its target ref, every line
//! comment in one place, and the channel it talks in.
use ducktape_view_guest::prelude::*;
use ducktape_view_guest::{Div, FontWeight, Stateful};

use crate::Forge;
use crate::ui::components::{badge, id, path_text, quiet, short_hex};
use forge::{Mergeability, Verdict};

/// The sidebar's width.
const SIDEBAR_W: Pixels = px(280.);
/// A fact's label column in Merge status.
const LABEL_W: Pixels = px(92.);

pub(crate) fn sidebar(forge: &Forge, cx: &mut Context<Forge>, theme: &Theme) -> Stateful<Div> {
    let column = div()
        .id(id("forge-dock"))
        .w(SIDEBAR_W)
        .flex_none()
        .flex()
        .flex_col()
        .min_h(px(0.))
        .overflow_y_scroll()
        .border_l_1()
        .border_color(theme.border)
        .bg(theme.background);
    let Some((change, _, _, _)) = forge.change() else {
        return column.child(quiet("Reading this change…", theme));
    };
    column
        .child(reviews(forge, theme))
        .child(merge_status(forge, theme))
        .child(comments(forge, cx, theme))
        .child(
            section("forge-channel", "Channel", None, theme).child(
                div()
                    .id(id("forge-channel-name"))
                    .child(format!("# {}", change.channel)),
            ),
        )
}

/// A sidebar section: its title (and a quiet note on the right), a
/// hairline under the whole of it.
fn section(key: &str, title: &str, note: Option<String>, theme: &Theme) -> Stateful<Div> {
    div()
        .id(id(key.to_owned()))
        .flex()
        .flex_col()
        .gap(design::space::XS)
        .px(design::space::BLOCK)
        .py(design::space::LG)
        .border_b_1()
        .border_color(theme.border)
        .child(
            div()
                .flex()
                .items_baseline()
                .mb(design::space::XXS)
                .child(
                    div()
                        .id(id(format!("{key}-title")))
                        .flex_1()
                        .text_size(design::text::SECONDARY)
                        .font_weight(FontWeight::SEMIBOLD)
                        .role(Role::Heading)
                        .aria_level(2)
                        .child(title.to_owned()),
                )
                .children(note.map(|note| {
                    design::mono(note)
                        .text_size(design::text::CAPTION)
                        .text_color(theme.muted)
                })),
        )
}

/// Each reviewer's latest verdict, then who was asked and has not
/// answered. Approvals are advisory: the merge is a compare-and-set on
/// both heads.
fn reviews(forge: &Forge, theme: &Theme) -> Stateful<Div> {
    let mut section = section("forge-reviews", "Reviews", Some("advisory".into()), theme);
    let Some((change, _, _, reviews)) = forge.change() else {
        return section;
    };
    let mut latest: Vec<(&forge::Principal, Verdict)> = Vec::new();
    for review in &reviews.items {
        match latest.iter_mut().find(|(who, _)| **who == review.author) {
            Some(held) => held.1 = review.draft.verdict,
            None => latest.push((&review.author, review.draft.verdict)),
        }
    }
    let waiting: Vec<&forge::Principal> = change
        .reviewers
        .iter()
        .filter(|asked| !latest.iter().any(|(who, _)| who == asked))
        .collect();
    if latest.is_empty() && waiting.is_empty() {
        return section.child(quiet("Nobody has reviewed this change yet.", theme));
    }
    for (index, (who, verdict)) in latest.into_iter().enumerate() {
        let (label, foreground, background) = match verdict {
            Verdict::Approve => ("approved", theme.success, theme.success_soft),
            Verdict::RequestChanges => ("changes", theme.danger, theme.danger_soft),
            Verdict::Comment => ("commented", theme.muted, theme.surface_raised),
        };
        section = section.child(person(
            forge.principal_name(who),
            badge(
                id(format!("forge-reviewer-verdict-{index}")),
                label,
                foreground,
                background,
            ),
            theme,
        ));
    }
    for (index, who) in waiting.into_iter().enumerate() {
        section = section.child(person(
            forge.principal_name(who),
            badge(
                id(format!("forge-reviewer-asked-{index}")),
                "asked",
                theme.muted,
                theme.surface_raised,
            ),
            theme,
        ));
    }
    section
}

/// A reviewer's line: avatar, name, their verdict on the right.
fn person(name: String, verdict: impl IntoElement, theme: &Theme) -> Div {
    div()
        .flex()
        .items_center()
        .gap(design::space::SM)
        .child(
            design::avatar(&name, design::size::AVATAR_SM, theme)
                .border_1()
                .border_color(theme.border)
                .font_weight(FontWeight::SEMIBOLD),
        )
        .child(div().flex_1().min_w(px(0.)).truncate().child(name))
        .child(verdict)
}

/// Whether the change can merge, how far apart its endpoints are, and the
/// two heads.
fn merge_status(forge: &Forge, theme: &Theme) -> Stateful<Div> {
    let mut section = section("forge-merge-status", "Merge status", None, theme);
    let Some((_, source, target, _)) = forge.change() else {
        return section;
    };
    let comparison = forge.compare();
    let (verdict, tone) = match comparison.map(|c| c.mergeability) {
        Some(Mergeability::UpToDate) => ("already contained", theme.muted),
        Some(Mergeability::FastForward) => ("fast-forward", theme.success),
        Some(Mergeability::Diverged) => ("diverged", theme.warning),
        Some(Mergeability::Unrelated) => ("unrelated histories", theme.danger),
        None => ("comparing…", theme.muted),
    };
    section = section.child(fact(
        "Mergeability",
        div().text_color(tone).child(verdict),
        theme,
    ));
    if let Some(comparison) = comparison {
        section = section.child(fact(
            "Distance",
            design::mono(format!(
                "{} ahead · {} behind",
                comparison.ahead, comparison.behind
            )),
            theme,
        ));
    }
    let head = |oid: &Option<String>| design::mono(oid.as_deref().map_or("gone".into(), short_hex));
    section
        .child(fact("Source head", head(source), theme))
        .child(fact("Target head", head(target), theme))
}

fn fact(label: &str, value: impl IntoElement, theme: &Theme) -> Div {
    div()
        .flex()
        .items_center()
        .gap(design::space::SM)
        .min_h(px(24.))
        .child(
            div()
                .w(LABEL_W)
                .flex_none()
                .text_color(theme.muted)
                .child(label.to_owned()),
        )
        .child(div().min_w(px(0.)).child(value))
}

/// Every line comment of this change, mine pending first, each opening
/// its file in the Files tab.
fn comments(forge: &Forge, cx: &mut Context<Forge>, theme: &Theme) -> Stateful<Div> {
    let Some((_, _, _, reviews)) = forge.change() else {
        return section("forge-comments", "Line comments", None, theme);
    };
    let mut rows = Vec::new();
    if let Some(review) = forge.review() {
        for staged in &review.comments {
            let path = staged.path.clone();
            let jump = cx.listener(move |forge, _: &ClickEvent, _, cx| {
                forge.open_file_comment(path.clone(), cx)
            });
            rows.push(comment_row(
                id(format!("forge-comment-draft-{}", staged.anchor())),
                format!("{}:{} · pending", path_text(&staged.path), staged.line),
                jump,
                theme,
            ));
        }
    }
    for review in &reviews.items {
        let author = forge.principal_name(&review.author);
        for comment in &review.draft.comments {
            let path = comment.path.clone();
            let jump = cx.listener(move |forge, _: &ClickEvent, _, cx| {
                forge.open_file_comment(path.clone(), cx)
            });
            rows.push(comment_row(
                id(format!(
                    "forge-comment-{}-{}-{}",
                    review.id,
                    path_text(&comment.path),
                    comment.line
                )),
                format!("{}:{} · {author}", path_text(&comment.path), comment.line),
                jump,
                theme,
            ));
        }
    }
    let count = rows.len();
    let section = section(
        "forge-comments",
        "Line comments",
        (count > 0).then(|| count.to_string()),
        theme,
    );
    match count {
        0 => section.child(quiet(
            "Nobody has written on a line of this change yet.",
            theme,
        )),
        _ => section.children(rows),
    }
}

fn comment_row(
    element_id: ElementId,
    text: String,
    jump: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
    theme: &Theme,
) -> Stateful<Div> {
    let theme = *theme;
    design::mono(text)
        .id(element_id)
        .truncate()
        .text_size(design::text::CAPTION)
        .text_color(theme.muted)
        .hover(move |style| style.text_color(theme.foreground))
        .role(Role::Link)
        .focusable()
        .on_click(jump)
}
