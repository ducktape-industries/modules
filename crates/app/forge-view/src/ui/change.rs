//! One Change: its header, the three tabs a reviewer lives in, and its
//! details beside them. Conversation is chat's hidden channel; Files is
//! the reviewer's home.
use ducktape_view_guest::design;
use ducktape_view_guest::prelude::*;
use ducktape_view_guest::{
    AnchoredPositionMode, Div, Editor, EditorElement, FontWeight, MouseDownEvent, Point, Stateful,
};

use crate::Forge;
use crate::state::{ChangeTab, verdict_label};
use crate::ui::changes::{revision_name, state_chip};
use crate::ui::components::{badge, button, heading, id, path_text, quiet, ref_label, short_hex};
use crate::ui::{PAGE_X, TAB_BAR_H, commits, diff, dock, pending, scroller, staged};
use ducktape_view_guest::Anchor;
use forge::{Change, ChangeState, FileDiff, Query, Reply, Verdict};

/// The Files tab's file list.
const FILE_LIST_W: Pixels = px(240.);
/// The finish-review panel.
const FINISH_W: Pixels = px(380.);

pub(crate) fn render(forge: &Forge, cx: &mut Context<Forge>, theme: &Theme) -> AnyElement {
    let Some(query) = forge.change_query() else {
        return div().into_any_element();
    };
    let reply = match staged(
        forge,
        &query,
        "forge-change",
        "Reading this change…",
        cx,
        theme,
    ) {
        Ok(reply) => reply,
        Err(state) => return state,
    };
    let Reply::Change { change, .. } = reply else {
        return div().into_any_element();
    };
    let scope = crate::state::change_key(&forge.repo_name(), change.n);
    let mut column = div()
        .id(id("forge-change-detail"))
        .flex()
        .flex_col()
        .flex_1()
        .min_h(px(0.))
        .child(header(forge, cx, theme))
        .child(tabs(forge, cx, theme))
        .child(pending(forge, &scope, theme));
    if let Some(form) = &forge.form {
        column = column.child(crate::ui::changes::form(form, forge, cx, theme));
    }
    let tab = forge.nav().change_tab;
    let body: AnyElement = match tab {
        ChangeTab::Conversation => crate::ui::conversation::render(forge, cx, theme),
        ChangeTab::Commits => match forge.endpoints() {
            Some((from, into)) => commits::log(
                forge,
                &commits::query(forge, from, Some(into)),
                "forge-change-log",
                cx,
                theme,
            ),
            None => div().into_any_element(),
        },
        ChangeTab::Files => files(forge, cx, theme),
    };
    // wide, the details stand beside the conversation and the commits (the
    // files keep their width for the diff); narrow, "Details" lays them
    // over the whole screen
    let mut row = div()
        .id(id("forge-change-body"))
        .relative()
        .flex()
        .flex_1()
        .min_h(px(0.))
        .child(
            div()
                .flex_1()
                .min_w(px(0.))
                .flex()
                .flex_col()
                .min_h(px(0.))
                .child(body),
        );
    if forge.layout.narrow() {
        if forge.layout.dock_open {
            row = row.child(design::over(
                id("forge-details-over"),
                dock::sidebar(forge, cx, theme).w_full(),
                theme,
            ));
        }
    } else if tab != ChangeTab::Files {
        row = row.child(dock::sidebar(forge, cx, theme));
    }
    column.child(row).into_any_element()
}

fn header(forge: &Forge, cx: &mut Context<Forge>, theme: &Theme) -> AnyElement {
    let Some((change, source, _, _)) = forge.change() else {
        return div().into_any_element();
    };
    let back = cx.listener(|forge, _: &ClickEvent, _, cx| forge.open_change(None, cx));
    let author = forge.principal_name(&change.author);
    let mut meta = format!(
        "{author} wants {} → {}",
        ref_label(&revision_name(&change.from)),
        ref_label(&change.into)
    );
    if let Some(head) = source {
        meta = format!("{meta} · head {}", short_hex(head));
    }
    let mut column = div()
        .id(id("forge-change-header"))
        .flex()
        .flex_col()
        .gap(design::space::XS)
        .px(PAGE_X)
        .pt(design::space::LG)
        .pb(design::space::MD)
        .child(
            div()
                .id(id("forge-change-back"))
                .text_size(design::text::SECONDARY)
                .text_color(theme.muted)
                .hover(|style| style.text_color(theme.foreground))
                .role(Role::Link)
                .focusable()
                .on_click(back)
                .child(format!("← {} · Changes", forge.repo_name())),
        )
        .child(title_line(forge, change, cx, theme))
        .child(
            design::mono(meta)
                .text_size(design::text::CAPTION)
                .text_color(theme.muted),
        );
    if closing(forge, change) {
        column = column.child(
            div()
                .id(id("forge-close-warning"))
                .text_size(design::text::SECONDARY)
                .text_color(theme.danger)
                .child("Closing is final: a closed change cannot be reopened."),
        );
    }
    // why Merge is off: a strip while the change is open, a quiet line
    // once it has ended
    match forge.merge_block() {
        Some(crate::actions::MergeBlock::NotOpen) => {
            column = column.child(
                div()
                    .id(id("forge-merge-refusal"))
                    .text_size(design::text::SECONDARY)
                    .text_color(theme.muted)
                    .child(crate::actions::MergeBlock::NotOpen.sentence()),
            );
        }
        Some(blocked) => {
            column = column.child(
                div()
                    .id(id("forge-merge-refusal"))
                    .mt(design::space::SM)
                    .px(design::space::LG)
                    .py(design::space::MD)
                    .bg(theme.warning_soft)
                    .text_color(theme.warning)
                    .text_size(design::text::SECONDARY)
                    .child(blocked.sentence()),
            );
        }
        None => {}
    }
    column.into_any_element()
}

/// Whether this change's Close waits for its second press.
fn closing(forge: &Forge, change: &Change) -> bool {
    forge.closing == Some(crate::state::change_key(&forge.repo_name(), change.n))
}

/// The change's title, number and state, and what an open change offers:
/// Edit, Close, and Merge (kept in place, off, while it cannot run).
fn title_line(
    forge: &Forge,
    change: &Change,
    cx: &mut Context<Forge>,
    theme: &Theme,
) -> Stateful<Div> {
    let edit = cx.listener(|forge, _: &ClickEvent, _, cx| forge.start_edit(cx));
    let close = cx.listener(|forge, _: &ClickEvent, _, cx| forge.close_change(cx));
    let merge = cx.listener(|forge, _: &ClickEvent, _, cx| forge.merge(cx));
    let open = change.state == ChangeState::Open;
    let mine = forge.me_principal().as_ref() == Some(&change.author);
    let closing = closing(forge, change);
    let mergeable = forge.merge_block().is_none();
    let mut top = div()
        .id(id("forge-change-head"))
        .flex()
        .flex_wrap()
        .items_center()
        .gap(design::space::SM)
        .child(heading(
            id("forge-change-title"),
            change.title.clone(),
            1,
            theme,
        ))
        .child(
            div()
                .text_size(design::text::TITLE)
                .text_color(theme.muted)
                .child(format!("#{}", change.n)),
        )
        .child(state_chip(change.state, change.n, theme))
        .child(div().flex_1());
    // an ended change has nothing left to edit, close or merge
    if open {
        top = top
            .child(
                button(id("forge-edit-change"), "Edit", theme, edit)
                    .kind(design::Kind::Quiet)
                    .enabled(mine),
            )
            .child(
                button(
                    id("forge-close-change"),
                    if closing { "Close for good" } else { "Close" },
                    theme,
                    close,
                )
                .kind(design::Kind::Outline)
                .enabled(forge.writes_repo()),
            )
            .child(
                button(id("forge-merge"), "Merge", theme, merge)
                    .kind(design::Kind::Primary)
                    .enabled(mergeable && forge.writes_repo()),
            );
    }
    top
}

/// The change's tabs, each with its count once it is known.
fn tabs(forge: &Forge, cx: &mut Context<Forge>, theme: &Theme) -> Stateful<Div> {
    let mut bar = div()
        .id(id("forge-change-tabs"))
        .h(TAB_BAR_H)
        .flex()
        .items_center()
        .gap(design::space::XL)
        .px(PAGE_X)
        .border_b_1()
        .border_color(theme.border);
    for tab in ChangeTab::ALL {
        let pick = cx.listener(move |forge, _: &ClickEvent, _, cx| forge.open_change_tab(tab, cx));
        bar = bar.child(crate::ui::components::tab(
            id(format!("forge-change-tab-{}", tab.slug())),
            tab.label(),
            tab_count(forge, tab),
            forge.nav().change_tab == tab,
            theme,
            pick,
        ));
    }
    bar
}

/// What a change tab holds, where the view has read it: the lines of the
/// conversation, the commits the target lacks, the files the diff touches.
fn tab_count(forge: &Forge, tab: ChangeTab) -> Option<u64> {
    match tab {
        ChangeTab::Conversation => {
            let (change, _, _, _) = forge.change()?;
            match forge.messages.get(&change.channel)? {
                ducktape_view_guest::Loadable::Ready(rows) => Some(rows.len() as u64),
                _ => None,
            }
        }
        ChangeTab::Commits => forge.compare().map(|comparison| comparison.ahead),
        ChangeTab::Files => match forge.ready(&forge.diff_query()?)? {
            Reply::Diff { total_files, .. } => Some(*total_files),
            _ => None,
        },
    }
}

// ------------------------------------------------------------------ files

fn files(forge: &Forge, cx: &mut Context<Forge>, theme: &Theme) -> AnyElement {
    let Some(query) = forge.diff_query() else {
        return quiet("Comparing the endpoints…", theme);
    };
    let mut columns = div().id(id("forge-files")).flex().flex_1().min_h(px(0.));
    if forge.layout.tree_visible() {
        columns = columns.child(file_tree(forge, &query, cx, theme));
    }
    let pane = div()
        .id(id("forge-files-pane"))
        .flex_1()
        .min_w(px(0.))
        .flex()
        .flex_col()
        .min_h(px(0.))
        .child(review_bar(forge, cx, theme))
        .child(diff::composer(forge, cx, theme))
        .child(diff::render(forge, &query, "forge-diff", true, cx, theme));
    columns.child(pane).into_any_element()
}

/// The reviewer's file list: how much each file changes, a tick for the
/// ones read, and a click that shows one file alone.
fn file_tree(forge: &Forge, query: &Query, cx: &mut Context<Forge>, theme: &Theme) -> AnyElement {
    let column = div()
        .id(id("forge-file-tree"))
        .w(FILE_LIST_W)
        .flex_none()
        .flex()
        .flex_col()
        .min_h(px(0.))
        .border_r_1()
        .border_color(theme.border);
    let Some(Reply::Diff {
        page, total_files, ..
    }) = forge.ready(query)
    else {
        return column
            .child(quiet("Reading the diff…", theme))
            .into_any_element();
    };
    let (added, deleted) = page.items.iter().fold((0, 0), |(a, d), file| {
        (a + file.additions, d + file.deletions)
    });
    let all = cx.listener(|forge, _: &ClickEvent, _, cx| forge.single_file(None, cx));
    let header = div()
        .id(id("forge-file-tree-header"))
        .flex()
        .items_center()
        .gap(design::space::SM)
        .px(design::space::BLOCK)
        .h(px(36.))
        .child(
            design::mono(format!(
                "{} · +{added} −{deleted}",
                design::plural(*total_files, "file", "files")
            ))
            .id(id("forge-files-title"))
            .text_size(design::text::CAPTION)
            .text_color(theme.muted),
        )
        .child(div().flex_1())
        .when(forge.nav().diff_path.is_some(), |header| {
            header.child(
                button(id("forge-files-all"), "All files", theme, all).kind(design::Kind::Quiet),
            )
        });
    let mut list = scroller("forge-file-tree-list").p_0().gap_0();
    for file in &page.items {
        list = list.children(file_row(forge, file, cx, theme));
    }
    list = list.child(
        div()
            .px(design::space::BLOCK)
            .py(design::space::LG)
            .child(quiet("Tick a file once you have read it.", theme)),
    );
    column.child(header).child(list).into_any_element()
}

/// One file of the diff: its viewed tick, its path, its counts and
/// comments.
fn file_row(
    forge: &Forge,
    file: &FileDiff,
    cx: &mut Context<Forge>,
    theme: &Theme,
) -> Option<AnyElement> {
    let path = diff::path_of(file)?;
    let label = path_text(&path);
    let viewed = forge
        .file_key(&path)
        .is_some_and(|key| forge.viewed.contains(&key));
    let comments = file_comments(forge, &path);
    let selected = forge.nav().diff_path.as_deref() == Some(path.as_slice());
    let pick = cx.listener({
        let path = path.clone();
        move |forge, _: &ClickEvent, _, cx| forge.single_file(Some(path.clone()), cx)
    });
    let tick = cx.listener({
        let path = path.clone();
        move |forge, _: &ClickEvent, _, cx| forge.toggle_viewed(&path, cx)
    });
    let theme = *theme;
    let check = div()
        .id(id(format!("forge-viewed-{label}")))
        .size(px(14.))
        .flex_none()
        .flex()
        .items_center()
        .justify_center()
        .border_1()
        .border_color(if viewed {
            theme.primary
        } else {
            theme.border_strong
        })
        .when(viewed, |check| {
            check.bg(theme.primary).text_color(theme.primary_foreground)
        })
        .text_size(px(10.))
        .role(Role::CheckBox)
        .aria_label(format!("Viewed {label}"))
        .aria_selected(viewed)
        .focusable()
        // a tick is not also a click on the row
        .occlude()
        .on_click(tick)
        .child(if viewed { "✓" } else { "" });
    let mut row = div()
        .id(id(format!("forge-file-{label}")))
        .h(px(30.))
        .px(design::space::BLOCK)
        .flex()
        .items_center()
        .gap(design::space::SM)
        .when(selected, |row| row.bg(theme.surface_raised))
        .hover(move |style| style.bg(theme.surface))
        .role(Role::Button)
        .aria_selected(selected)
        .focusable()
        .on_click(pick)
        .child(check)
        .child(
            design::mono(label.clone())
                .flex_1()
                .min_w(px(0.))
                .truncate()
                .text_size(design::text::CAPTION)
                .text_color(if viewed {
                    theme.muted
                } else {
                    theme.foreground
                }),
        );
    if comments > 0 {
        row = row.child(badge(
            id(format!("forge-file-comments-{label}")),
            comments.to_string(),
            theme.muted,
            theme.surface_raised,
        ));
    }
    row = row.child(
        div()
            .flex()
            .gap(design::space::XXS)
            .font_family(design::fonts::FAMILY_MONO)
            .text_size(design::text::CAPTION)
            .when(file.additions > 0, |counts| {
                counts.child(
                    div()
                        .text_color(theme.success)
                        .child(format!("+{}", file.additions)),
                )
            })
            .when(file.deletions > 0, |counts| {
                counts.child(
                    div()
                        .text_color(theme.danger)
                        .child(format!("−{}", file.deletions)),
                )
            }),
    );
    Some(row.into_any_element())
}

/// The line comments on one file: my review's drafts plus the published.
fn file_comments(forge: &Forge, path: &[u8]) -> usize {
    let drafts = forge
        .review()
        .map(|review| {
            review
                .comments
                .iter()
                .filter(|comment| comment.path == path)
                .count()
        })
        .unwrap_or(0);
    let landed = forge
        .change()
        .map(|(_, _, _, reviews)| {
            reviews
                .items
                .iter()
                .flat_map(|review| review.draft.comments.iter())
                .filter(|comment| comment.path == path)
                .count()
        })
        .unwrap_or(0);
    drafts + landed
}

/// The strip above the diff: what a review is, or where this one stands.
fn strip(theme: &Theme) -> Stateful<Div> {
    div()
        .id(id("forge-review-bar"))
        .flex()
        .items_center()
        .gap(design::space::MD)
        .px(design::space::BLOCK)
        .h(px(36.))
        .flex_none()
        .bg(theme.surface)
        .border_b_1()
        .border_color(theme.border)
}

/// No review under way: how to comment, and Start review.
fn idle_review_bar(forge: &Forge, cx: &mut Context<Forge>, theme: &Theme) -> AnyElement {
    let start = cx.listener(|forge, _: &ClickEvent, _, cx| forge.start_review(cx));
    strip(theme)
        .child(quiet(
            "Comment on any line by clicking its gutter number.",
            theme,
        ))
        .child(div().flex_1())
        .child(
            button(id("forge-start-review"), "Start review", theme, start)
                .kind(design::Kind::Primary)
                .enabled(forge.may_write()),
        )
        .into_any_element()
}

/// Start review → staged comments → finish as exactly one operation. The
/// finish form opens as a panel under its button.
fn review_bar(forge: &Forge, cx: &mut Context<Forge>, theme: &Theme) -> AnyElement {
    let Some(review) = forge.review() else {
        return idle_review_bar(forge, cx, theme);
    };
    let cancel = cx.listener(|forge, _: &ClickEvent, _, cx| forge.cancel_review(cx));
    let finishing = cx.listener(|forge, _: &ClickEvent, _, cx| forge.finishing(true, cx));
    let pending = match review.comments.len() {
        1 => "1 line comment pending".to_owned(),
        n => format!("{n} line comments pending"),
    };
    let mut finish = div().relative().flex_none().child(
        button(
            id("forge-finish-review"),
            "Finish review ⌄",
            theme,
            finishing,
        )
        .kind(design::Kind::Primary)
        .enabled(forge.may_write()),
    );
    if review.finishing {
        finish = finish.child(finish_panel(forge, review, cx, theme));
    }
    let bar = strip(theme)
        .child(badge(
            id("forge-review-state"),
            "Reviewing",
            theme.muted,
            theme.surface_raised,
        ))
        .child(quiet(
            format!("pinned at {} · {pending}", short_hex(&review.commit)),
            theme,
        ))
        .child(div().flex_1())
        .child(
            button(id("forge-cancel-review"), "Discard", theme, cancel).kind(design::Kind::Quiet),
        )
        .child(finish);
    let mut column = div().flex().flex_col().child(bar);
    if !review.error.is_empty() {
        column = column.child(
            div()
                .id(id("forge-review-error"))
                .px(design::space::BLOCK)
                .py(design::space::XS)
                .text_size(design::text::SECONDARY)
                .text_color(theme.danger)
                .child(review.error.clone()),
        );
    }
    column.into_any_element()
}

/// The finish form, over the diff under its button: what the review says,
/// its verdict, Submit review. A press outside folds it; nothing typed is
/// lost.
fn finish_panel(
    forge: &Forge,
    review: &crate::state::ReviewSession,
    cx: &mut Context<Forge>,
    theme: &Theme,
) -> AnyElement {
    // a document per change: the host keeps a document by its name, and
    // one name across changes would carry one change's body into another's
    let document = format!(
        "forge-review-body-{}",
        forge.review_key().unwrap_or_default()
    );
    let fold = cx.listener(|forge, _: &MouseDownEvent, _, cx| forge.finishing(false, cx));
    let verdict = review.verdict.unwrap_or(Verdict::Comment);
    let submit = cx.listener(move |forge, _: &ClickEvent, _, cx| forge.finish_review(verdict, cx));
    let panel = div()
        .id(id("forge-finish-panel"))
        .w(FINISH_W)
        .flex()
        .flex_col()
        .gap(design::space::LG)
        .p(design::space::BLOCK)
        .border_1()
        .border_color(theme.border_strong)
        .bg(theme.background)
        .shadow_lg()
        .occlude()
        .on_mouse_down_out(fold)
        .child(
            div()
                .font_weight(FontWeight::SEMIBOLD)
                .child("Finish your review"),
        )
        .child(review_body(&review.body, document, theme))
        .child(verdicts(forge, verdict, cx, theme))
        .child(
            div().flex().justify_end().child(
                button(id("forge-submit-review"), "Submit review", theme, submit)
                    .kind(design::Kind::Primary)
                    .enabled(forge.may_write()),
            ),
        );
    // the panel hangs from the button's right edge: a zero-width box pinned
    // there is where its anchor starts
    div()
        .absolute()
        .top_0()
        .right_0()
        .child(deferred(
            anchored()
                .anchor(Anchor::TopRight)
                .position_mode(AnchoredPositionMode::Local)
                .position(Point {
                    x: px(0.),
                    y: px(0.),
                })
                .offset(Point {
                    x: px(0.),
                    y: design::size::CONTROL + design::space::XXS,
                })
                .snap_to_window_with_margin(ducktape_view_guest::Edges::all(design::space::SM))
                .child(panel),
        ))
        .into_any_element()
}

/// What the review says overall, typed while finishing.
fn review_body(body: &Editor, document: String, theme: &Theme) -> impl IntoElement + use<> {
    EditorElement::plain(
        id("forge-review-body"),
        body,
        document,
        |forge: &mut Forge| forge.review_mut().map(|review| &mut review.body),
        "Review body",
    )
    .min_h(design::size::CONTROL * 2.5)
    .w_full()
    .px_2()
    .border_1()
    .border_color(theme.border_strong)
    .bg(theme.background)
    .text_color(theme.foreground)
    .placeholder("What this review says overall")
}

/// The three verdicts as radio rows, each with what it means.
fn verdicts(
    forge: &Forge,
    picked: Verdict,
    cx: &mut Context<Forge>,
    theme: &Theme,
) -> Stateful<Div> {
    let theme = *theme;
    let mut column = div()
        .id(id("forge-verdicts"))
        .flex()
        .flex_col()
        .gap(design::space::SM)
        .role(Role::RadioGroup);
    for (verdict, slug, about) in [
        (Verdict::Comment, "comment", "Feedback without a verdict."),
        (Verdict::Approve, "approve", "Ready to merge as it is."),
        (
            Verdict::RequestChanges,
            "request-changes",
            "Needs work before it merges.",
        ),
    ] {
        let on = verdict == picked;
        let pick = cx.listener(move |forge, _: &ClickEvent, _, cx| forge.pick_verdict(verdict, cx));
        let dot = div()
            .size(px(14.))
            .mt(px(2.))
            .flex_none()
            .rounded_full()
            .border_1()
            .border_color(if on {
                theme.foreground
            } else {
                theme.border_strong
            })
            .when(on, |dot| dot.bg(theme.foreground));
        column = column.child(
            div()
                .id(id(format!("forge-verdict-{slug}")))
                .flex()
                .items_start()
                .gap(design::space::SM)
                .role(Role::RadioButton)
                .aria_selected(on)
                .when(!forge.may_write(), |row| row.aria_disabled(true))
                .focusable()
                .on_click(pick)
                .child(dot)
                .child(
                    div().flex().flex_col().child(verdict_label(verdict)).child(
                        div()
                            .text_size(design::text::SECONDARY)
                            .text_color(theme.muted)
                            .child(about),
                    ),
                ),
        );
    }
    column
}
