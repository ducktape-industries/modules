//! The Change screens: the list, the detail header, the conversation, the
//! reviewer's Files tab, and the one operation a review becomes.
use super::{booted, change_screen, change_screen_as, opened};
use crate::api::SubmitForge;
use crate::state::ChangeTab;
use ducktape_view_guest::methods::Submit;
use ducktape_view_guest::testing::TestAppContext;
use ducktape_view_guest::wire;
use ducktape_view_guest::wire::Node;
use forge::{LineComment, Op, Side, Verdict};

/// The change tabs read a log or a whole diff on open, so → only moves and
/// Enter opens; the state filter is a radio group and checks on the arrow.
#[test]
fn the_change_tabs_move_on_an_arrow_and_open_on_enter() {
    let (mut cx, view) = change_screen("default", ChangeTab::Conversation);
    assert!(cx.interactivity("forge-change-tabs").focusable);
    cx.simulate_key_down("forge-change-tabs", "right");
    view.read(|forge| assert_eq!(forge.nav().change_tab, ChangeTab::Conversation));
    assert!(
        cx.interactivity("forge-change-tab-commits")
            .aria
            .active_descendant
    );
    cx.simulate_key_down("forge-change-tabs", "enter");
    cx.run_until_parked();
    view.read(|forge| assert_eq!(forge.nav().change_tab, ChangeTab::Commits));
}

#[test]
fn the_change_state_filter_checks_on_an_arrow() {
    let (mut cx, view) = opened("default");
    cx.simulate_click("forge-tab-changes");
    cx.run_until_parked();
    use crate::state::Filter;
    view.read(|forge| assert_eq!(forge.filter, Filter::Open));
    assert!(!cx.interactivity("forge-filter-merged").focusable);
    cx.simulate_focus("forge-filter-states");
    cx.simulate_key_down("forge-filter-states", "right");
    cx.run_until_parked();
    view.read(|forge| assert_eq!(forge.filter, Filter::Merged));
    let merged = cx.interactivity("forge-filter-merged");
    assert_eq!(
        merged.aria.toggled,
        Some(ducktape_view_guest::Toggled::True)
    );
    assert!(merged.aria.active_descendant);
}

#[test]
fn the_change_list_shows_the_plans_row_and_its_filters() {
    let (mut cx, view) = opened("default");
    cx.simulate_click("forge-tab-changes");
    cx.run_until_parked();
    use crate::state::Filter;
    for filter in [
        Filter::Open,
        Filter::Merged,
        Filter::Closed,
        Filter::Judgment,
        Filter::Authored,
        Filter::Involves,
    ] {
        assert!(
            cx.find(&format!("forge-filter-{}", filter.slug()))
                .is_some(),
            "{} filter",
            filter.label()
        );
    }
    assert!(cx.has_text("Review this change"), "{:?}", cx.texts());
    assert!(cx.find("forge-change-state-1").is_some(), "the state dot");
    assert!(
        cx.has_text("feature → main · Ada"),
        "the author key resolves: {:?}",
        cx.texts()
    );
    cx.simulate_click("forge-filter-closed");
    cx.run_until_parked();
    assert!(cx.has_text("Close this change"), "{:?}", cx.texts());
    cx.simulate_input("forge-changes-search", "nothing like this");
    cx.run_until_parked();
    assert!(cx.has_text("No changes here"));
    view.read(|forge| assert_eq!(forge.filter, crate::state::Filter::Closed));
}

#[test]
fn the_change_header_carries_its_endpoints_and_a_merge_the_program_allows() {
    let (cx, view) = change_screen_as("default", ChangeTab::Conversation, 9);
    view.read(|forge| assert_eq!(forge.nav().change, Some(1)));
    assert!(cx.has_text("Review this change"), "{:?}", cx.texts());
    assert!(cx.has_text("#1") && cx.has_text("open"));
    assert!(cx.has_text("Ada wants feature → main · head 26607f52…a84d"));
    // `compare` says FastForward, so the merge is offered.
    let Some(ducktape_view_guest::wire::Node::Container(node)) = cx.find("forge-merge") else {
        panic!("the merge button is a native container");
    };
    assert!(node.interactivity.on_click.is_some());
    assert_ne!(node.interactivity.aria.disabled, Some(true));
    // The reader is not the author, so editing is closed to them.
    let Some(ducktape_view_guest::wire::Node::Container(edit)) = cx.find("forge-edit-change")
    else {
        panic!("the edit button stays visible")
    };
    assert_eq!(edit.interactivity.aria.disabled, Some(true));
}

#[test]
fn a_diverged_comparison_says_why_it_cannot_merge() {
    let (cx, _view) = change_screen("diverged", ChangeTab::Conversation);
    assert!(
        cx.has_text(
            "The endpoints diverged. Merge with git and push the result, then Merge turns on."
        ),
        "{:?}",
        cx.texts()
    );
    let Some(ducktape_view_guest::wire::Node::Container(node)) = cx.find("forge-merge") else {
        panic!("the merge button stays visible");
    };
    assert_eq!(node.interactivity.aria.disabled, Some(true));
    assert!(node.interactivity.on_click.is_none());
}

#[test]
fn merging_submits_the_client_computed_fast_forward() {
    let (mut cx, _) = change_screen_as("default", ChangeTab::Conversation, 9);
    cx.simulate_click("forge-merge");
    cx.run_until_parked();
    assert!(
        cx.host()
            .requests::<SubmitForge>()
            .iter()
            .any(|op| matches!(
                op,
                Op::Merge {
                    repo,
                    expected_into,
                    expected_from,
                    result,
                    change: Some(1),
                    ..
                } if repo == "project"
                    && expected_into == "ebfb8b62a50d6e5f7d10062af7cc5d15fd224e16"
                    && expected_from == "26607f522099476177a45a8058a93108fba5a84d"
                    && result == expected_from
            ))
    );
    assert!(cx.has_text("Merging this change"));
}

#[test]
fn an_operation_shows_its_submission_then_a_refusal_reverts_it_with_the_reason() {
    let (mut cx, _) = change_screen_as("default", ChangeTab::Conversation, 9);
    cx.host()
        .refuse::<SubmitForge>("unauthorized", "this key may not close that change");
    cx.simulate_click("forge-close-change");
    cx.run_until_parked();
    assert!(
        cx.has_text("Close for good") && !cx.has_text("Closing this change"),
        "the first press only asks"
    );
    cx.simulate_click("forge-close-change");
    cx.run_until_parked();
    assert!(
        cx.has_text("Refused: this key may not close that change"),
        "{:?}",
        cx.texts()
    );
    assert!(cx.has_text("Closing this change"), "the row stays in place");
}

#[test]
fn a_repository_filter_does_not_carry_into_its_change_search() {
    let (mut cx, _view) = booted("default");
    cx.simulate_input("forge-repos-search", "proj");
    cx.run_until_parked();
    cx.simulate_click("forge-repo-project-open");
    cx.run_until_parked();
    cx.simulate_click("forge-tab-changes");
    cx.run_until_parked();
    assert!(cx.has_text("Review this change"), "{:?}", cx.texts());
}

#[test]
fn a_merged_change_wears_its_state_and_offers_nothing_more() {
    let (cx, _view) = change_screen("merged", ChangeTab::Conversation);
    assert!(cx.has_text("merged"), "{:?}", cx.texts());
    assert!(
        cx.texts()
            .iter()
            .any(|text| text.starts_with("merged into main as ")),
        "{:?}",
        cx.texts()
    );
    assert!(
        cx.texts()
            .windows(2)
            .any(|w| w[0] == "Ada" && w[1].starts_with("merged into main as ")),
        "the merger is named from the record: {:?}",
        cx.texts()
    );
    assert!(
        cx.has_text("This change is no longer open"),
        "{:?}",
        cx.texts()
    );
    for button in ["forge-edit-change", "forge-close-change", "forge-merge"] {
        assert!(cx.find(button).is_none(), "{button} on an ended change");
    }
}

#[test]
fn a_closed_change_names_who_closed_it_and_offers_no_edit_or_close() {
    let (cx, _view) = change_screen("closed", ChangeTab::Conversation);
    assert!(
        cx.texts()
            .windows(2)
            .any(|w| w[0] == "Ada" && w[1] == "closed this change"),
        "the closer is named from the record: {:?}",
        cx.texts()
    );
    for button in ["forge-edit-change", "forge-close-change", "forge-merge"] {
        assert!(cx.find(button).is_none(), "{button} on an ended change");
    }
}

#[test]
fn the_conversation_is_the_hidden_chat_channel_and_the_forge_body() {
    let (mut cx, view) = change_screen("reviewed", ChangeTab::Conversation);
    assert!(
        cx.find("forge-change-body-text").is_some(),
        "the body is markdown"
    );
    // forge's own lines read from its records, never from their text
    assert!(cx.has_text("Ada") && cx.has_text("opened this change"));
    assert!(!cx.has_text("raw forge text"), "{:?}", cx.texts());
    assert!(!cx.texts().iter().any(|text| text.contains("module:")));
    assert!(cx.has_text("Reading it now"));
    assert!(cx.has_text("Rae"), "a chat handle resolves to a name");
    // Three reviews across two pages of the Change reply.
    view.read(|forge| {
        let (_, _, _, reviews) = forge.change().expect("the change landed");
        assert_eq!(reviews.items.len(), 3, "the review cursor was followed");
    });
    assert!(cx.has_text("approved") && cx.has_text("requested changes"));
    assert!(
        cx.has_text("src/lib.rs:2 — Context is commentable"),
        "a line comment is quoted under its review: {:?}",
        cx.texts()
    );
    // the change is still open: forge's last line has no ending to name yet
    assert!(
        !cx.texts()
            .iter()
            .any(|text| text.starts_with("merged into"))
    );
    assert!(
        matches!(
            cx.find("forge-reply"),
            Some(ducktape_view_guest::wire::Node::Field {
                multiline: true,
                ..
            })
        ),
        "the reply is the host's multi-line editor"
    );
    // what the host's editor holds once the reply is typed
    cx.update(&view, |forge, _, cx| {
        forge.reply = ducktape_view_guest::TextField::new("looks right to me");
        cx.notify();
    });
    cx.run_until_parked();
    cx.simulate_click("forge-reply-send");
    cx.run_until_parked();
    assert!(
        cx.host()
            .requests::<Submit<::chat::Chat>>()
            .iter()
            .any(|op| matches!(
                op,
                chat::Op::PostMessage { channel_id, .. } if channel_id == "forge:project:1"
            ))
    );
    view.read(|forge| assert!(forge.reply.text.is_empty()));
}

#[test]
fn a_review_pinned_before_a_push_reads_as_outdated() {
    let (cx, _view) = change_screen("outdated", ChangeTab::Conversation);
    assert!(cx.has_text("outdated"), "{:?}", cx.texts());
}

#[test]
fn the_files_tab_marks_comments_and_viewed_files_and_can_show_one() {
    let (mut cx, view) = change_screen("reviewed", ChangeTab::Files);
    assert!(cx.has_text("src/lib.rs"), "{:?}", cx.texts());
    assert!(cx.has_text("+2") && cx.has_text("−1"));
    assert!(
        cx.find("forge-file-comments-src/lib.rs").is_some(),
        "published line comments mark the file"
    );
    cx.simulate_click("forge-viewed-src/lib.rs");
    cx.run_until_parked();
    view.read(|forge| assert!(forge.viewed.contains("project#1:src/lib.rs")));
    assert!(cx.has_text("✓"));
    let Some(wire::Node::Container(tick)) = cx.find("forge-viewed-src/lib.rs") else {
        panic!("the viewed tick");
    };
    assert_eq!(
        tick.interactivity.aria.toggled,
        Some(true.into()),
        "checked"
    );
    cx.simulate_click("forge-file-src/lib.rs-open");
    cx.run_until_parked();
    view.read(|forge| {
        assert_eq!(
            forge.nav().diff_path.as_deref(),
            Some(b"src/lib.rs".as_slice())
        )
    });
    // the row holds the tick beside its press; the shown file is current
    let row = super::control(&cx, "forge-file-src/lib.rs");
    assert_eq!(
        row.interactivity.role,
        Some(ducktape_view_guest::Role::ListItem)
    );
    assert!(!row.interactivity.focusable && row.interactivity.on_click.is_none());
    let open = super::control(&cx, "forge-file-src/lib.rs-open");
    assert_eq!(
        open.interactivity.role,
        Some(ducktape_view_guest::Role::Button)
    );
    assert_eq!(
        open.interactivity.aria.current,
        Some(ducktape_view_guest::accesskit::AriaCurrent::True)
    );
    assert!(!super::holds(
        &wire::Node::Container(open),
        "forge-viewed-src/lib.rs"
    ));
    cx.simulate_click("forge-files-all");
    cx.run_until_parked();
    view.read(|forge| assert!(forge.nav().diff_path.is_none()));
}

#[test]
fn the_diff_draws_typed_lines_and_believes_the_program_about_a_literal_plus_plus() {
    let (cx, _view) = change_screen("reviewed", ChangeTab::Files);
    assert!(cx.find("forge-diff").is_some());
    // A diff this small is drawn whole, so the screen carries every row of
    // the hunk and not only the one the list would have measured.
    assert!(
        cx.find("forge-diff-hunk-1").is_some(),
        "the hunk header is a row: {:?}",
        cx.texts()
    );
    for line in ["one", "keep", "old", "++ x", "end", "added"] {
        assert!(cx.has_text(line), "{line} is drawn: {:?}", cx.texts());
    }
    for gutter in [
        "forge-gutter-src/lib.rs-old-1",
        "forge-gutter-src/lib.rs-new-1",
        "forge-gutter-src/lib.rs-new-5",
    ] {
        assert!(cx.find(gutter).is_some(), "{gutter} carries its number");
    }
    assert!(
        cx.has_text("+") && cx.has_text("\u{2212}"),
        "an added and a removed marker"
    );
    let Some(ducktape_view_guest::wire::Node::Container(gutter)) =
        cx.find("forge-gutter-src/lib.rs-new-5")
    else {
        panic!("the gutter number is the comment button");
    };
    assert_eq!(
        gutter.interactivity.aria.label.as_deref(),
        Some("Comment on this line")
    );
    assert!(gutter.interactivity.on_click.is_some());
}

/// Under a review the diff is one grid: ↓ moves to the next line with a
/// gutter (scrolled into view), ← → between its old and new gutters, and
/// Enter comments at the active gutter.
#[test]
fn the_diff_gutters_are_a_grid_the_arrows_walk() {
    let (mut cx, view) = change_screen("reviewed", ChangeTab::Files);
    cx.simulate_click("forge-start-review");
    cx.run_until_parked();
    let grid = cx.interactivity("forge-diff-lines");
    assert_eq!(grid.role, Some(ducktape_view_guest::Role::Grid));
    assert!(grid.focusable && grid.tab_stop == Some(true));
    let active_gutter = |cx: &ducktape_view_guest::testing::TestAppContext| -> String {
        fn find(node: &ducktape_view_guest::wire::Node) -> Option<String> {
            if node
                .interactivity()
                .is_some_and(|i| i.aria.active_descendant)
            {
                return node.key().map(str::to_owned);
            }
            node.children().iter().find_map(find)
        }
        find(cx.find("forge-diff-lines").expect("the grid")).expect("a gutter claims")
    };
    let first = active_gutter(&cx);
    assert!(first.starts_with("forge-gutter-src/lib.rs-old-"), "{first}");
    assert!(!cx.interactivity(&first).focusable);
    assert_eq!(
        cx.interactivity(&format!("{first}-cell")).role,
        Some(ducktape_view_guest::Role::GridCell)
    );
    cx.simulate_focus("forge-diff-lines");
    cx.simulate_key_down("forge-diff-lines", "right");
    let second = active_gutter(&cx);
    assert!(
        second.starts_with("forge-gutter-src/lib.rs-new-"),
        "{second}"
    );
    cx.simulate_key_down("forge-diff-lines", "ctrl-end");
    cx.simulate_key_down("forge-diff-lines", "end");
    let last = active_gutter(&cx);
    assert_ne!(last, second);
    let (side, number) = last
        .trim_start_matches("forge-gutter-src/lib.rs-")
        .split_once('-')
        .expect("side-number");
    cx.simulate_key_down("forge-diff-lines", "enter");
    cx.run_until_parked();
    view.read(|forge| {
        let open = forge
            .review()
            .unwrap()
            .open
            .as_ref()
            .expect("an open anchor");
        assert_eq!(open.line, number.parse::<u64>().unwrap());
        assert_eq!(open.new_side, side == "new");
    });
}

#[test]
fn the_gutter_of_a_drawn_line_is_the_comment_button() {
    let (mut cx, view) = change_screen("reviewed", ChangeTab::Files);
    cx.simulate_click("forge-start-review");
    cx.run_until_parked();
    assert!(
        cx.has_text("pinned at 26607f52…a84d · 0 line comments pending"),
        "{:?}",
        cx.texts()
    );
    cx.simulate_click("forge-gutter-src/lib.rs-new-5");
    cx.run_until_parked();
    view.read(|forge| {
        let review = forge.review().expect("a review session");
        let open = review.open.as_ref().expect("an open anchor");
        assert_eq!(open.line, 5);
        assert!(open.new_side);
    });
    assert!(cx.has_text("src/lib.rs:5 (new)"));
}

/// The verdicts are a radio group: one Tab stop, ↓ checks the next one.
#[test]
fn the_verdicts_check_on_an_arrow() {
    let (mut cx, view) = change_screen("reviewed", ChangeTab::Files);
    cx.simulate_click("forge-start-review");
    cx.run_until_parked();
    cx.simulate_click("forge-finish-review");
    cx.run_until_parked();
    let group = cx.interactivity("forge-verdicts");
    assert_eq!(group.role, Some(ducktape_view_guest::Role::RadioGroup));
    assert!(group.focusable && group.tab_stop == Some(true));
    assert!(!cx.interactivity("forge-verdict-approve").focusable);
    cx.simulate_focus("forge-verdicts");
    cx.simulate_key_down("forge-verdicts", "down");
    cx.run_until_parked();
    view.read(|forge| {
        assert_eq!(forge.review().unwrap().verdict, Some(Verdict::Approve));
    });
    let approve = cx.interactivity("forge-verdict-approve");
    assert_eq!(
        approve.aria.toggled,
        Some(ducktape_view_guest::Toggled::True)
    );
    assert!(approve.aria.active_descendant);
    assert!(
        !cx.interactivity("forge-verdict-comment")
            .aria
            .active_descendant
    );
}

#[test]
fn a_review_batches_every_anchor_into_exactly_one_operation() {
    let (mut cx, view) = change_screen("reviewed", ChangeTab::Files);
    cx.simulate_click("forge-start-review");
    cx.run_until_parked();

    // Two anchors, one of them re-staged: one draft per anchor.
    for (path, new_side, line, body) in [
        (b"src/lib.rs".as_slice(), true, 3u64, "first thought"),
        (b"src/lib.rs".as_slice(), true, 3, "second thought"),
        (b"src/lib.rs".as_slice(), false, 1, "the old side too"),
    ] {
        cx.update(&view, |forge, _, cx| {
            forge.open_comment(path.to_vec(), new_side, line, cx)
        });
        cx.run_until_parked();
        cx.simulate_input("forge-comment-body", body);
        cx.simulate_click("forge-comment-save");
        cx.run_until_parked();
    }
    view.read(|forge| {
        let review = forge.review().expect("a review session");
        assert_eq!(review.comments.len(), 2, "re-staging replaces its anchor");
    });

    cx.simulate_click("forge-finish-review");
    cx.run_until_parked();
    assert!(
        matches!(
            cx.find("forge-review-body"),
            Some(ducktape_view_guest::wire::Node::Field {
                multiline: true,
                ..
            })
        ),
        "the review body is the host's multi-line editor"
    );
    cx.update(&view, |forge, _, cx| {
        forge.review_mut().unwrap().body = ducktape_view_guest::TextField::new("one batch, one op");
        cx.notify();
    });
    cx.run_until_parked();
    cx.simulate_click("forge-verdict-request-changes");
    cx.run_until_parked();
    cx.simulate_click("forge-submit-review");
    cx.run_until_parked();

    let submitted = cx.host().requests::<SubmitForge>();
    let reviews: Vec<&Op> = submitted
        .iter()
        .filter(|op| matches!(op, Op::ReviewSubmit { .. }))
        .collect();
    assert_eq!(reviews.len(), 1, "one review is one operation");
    let Op::ReviewSubmit { repo, n, review } = reviews[0] else {
        unreachable!()
    };
    assert_eq!((repo.as_str(), *n), ("project", 1));
    assert_eq!(review.verdict, Verdict::RequestChanges);
    assert_eq!(review.body, "one batch, one op");
    assert_eq!(
        review.commit_oid, "26607f522099476177a45a8058a93108fba5a84d",
        "the pin is the head the reader read"
    );
    assert_eq!(
        review.base_oid.as_deref(),
        Some("ebfb8b62a50d6e5f7d10062af7cc5d15fd224e16"),
        "the old side addresses the merge base"
    );
    let mut anchors = review.comments.clone();
    anchors.sort_by_key(|comment| (comment.side, comment.line));
    assert_eq!(
        anchors,
        vec![
            LineComment {
                path: b"src/lib.rs".to_vec(),
                side: Side::Old,
                line: 1,
                body: "the old side too".into(),
            },
            LineComment {
                path: b"src/lib.rs".to_vec(),
                side: Side::New,
                line: 3,
                body: "second thought".into(),
            },
        ]
    );
    assert!(cx.has_text("Submitting this review") || cx.has_text("Waiting for the next block"));
    view.read(|forge| {
        assert!(
            forge.review().is_none(),
            "a landed review ends its session: no second Finish at the old pin"
        )
    });
}

#[test]
fn a_refused_review_keeps_every_draft() {
    let (mut cx, view) = change_screen("reviewed", ChangeTab::Files);
    cx.simulate_click("forge-start-review");
    cx.run_until_parked();
    cx.update(&view, |forge, _, cx| {
        forge.open_comment(b"src/lib.rs".to_vec(), true, 3, cx)
    });
    cx.run_until_parked();
    cx.simulate_input("forge-comment-body", "keep me");
    cx.simulate_click("forge-comment-save");
    cx.run_until_parked();
    cx.host()
        .refuse::<SubmitForge>("capacity", "this operation exceeds its bound");
    cx.simulate_click("forge-finish-review");
    cx.run_until_parked();
    cx.simulate_click("forge-verdict-approve");
    cx.run_until_parked();
    cx.simulate_click("forge-submit-review");
    cx.run_until_parked();
    assert!(
        cx.has_text("Refused: this operation exceeds its bound"),
        "{:?}",
        cx.texts()
    );
    view.read(|forge| {
        let review = forge.review().expect("the session survives a refusal");
        assert_eq!(review.comments.len(), 1);
        assert_eq!(review.comments[0].body.text, "keep me");
    });
}

#[test]
fn an_empty_comment_verdict_is_refused_before_it_reaches_the_program() {
    let (mut cx, view) = change_screen("reviewed", ChangeTab::Files);
    cx.simulate_click("forge-start-review");
    cx.run_until_parked();
    let before = cx.host().requests::<SubmitForge>().len();
    cx.simulate_click("forge-finish-review");
    cx.run_until_parked();
    cx.simulate_click("forge-verdict-comment");
    cx.run_until_parked();
    cx.simulate_click("forge-submit-review");
    cx.run_until_parked();
    assert_eq!(cx.host().requests::<SubmitForge>().len(), before);
    assert!(
        cx.has_text("A comment review needs a body or a line comment"),
        "{:?}",
        cx.texts()
    );
    view.read(|forge| assert!(forge.review().is_some()));
}

/// Wide, a change's details stand beside its conversation: its reviews,
/// its merge status, every line comment (each opening its file in Files)
/// and its channel. Narrow, one "Details" toggle lays them over the screen.
#[test]
fn the_details_sidebar_holds_reviews_merge_status_and_line_comments() {
    let (mut cx, view) = change_screen("reviewed", ChangeTab::Conversation);
    for section in [
        "forge-reviews",
        "forge-merge-status",
        "forge-comments",
        "forge-channel",
    ] {
        assert!(cx.find(section).is_some(), "{section}: {:?}", cx.texts());
    }
    assert!(cx.has_text("fast-forward"), "{:?}", cx.texts());
    assert!(cx.has_text("advisory"));
    assert!(cx.has_text("src/lib.rs:2 · Rae"), "{:?}", cx.texts());
    cx.simulate_click("forge-comment-1-src/lib.rs-2");
    cx.run_until_parked();
    view.read(|forge| {
        assert_eq!(forge.nav().change_tab, ChangeTab::Files);
        assert_eq!(
            forge.nav().diff_path.as_deref(),
            Some(b"src/lib.rs".as_slice())
        )
    });
    // the Files tab keeps its width for the diff
    assert!(cx.find("forge-dock").is_none());
    cx.update(&view, |forge, _, cx| {
        forge.open_change_tab(ChangeTab::Conversation, cx);
        forge.measured(720., 760., cx);
    });
    cx.run_until_parked();
    assert!(cx.find("forge-dock").is_none(), "narrow folds the details");
    cx.simulate_click("forge-toggle-dock");
    cx.run_until_parked();
    assert!(cx.find("forge-details-over").is_some());
    assert!(cx.find("forge-merge-status").is_some());
}

/// The edit form's body is a multi-line editor: a body of paragraphs comes
/// back from the form with its breaks, not as one line.
#[test]
fn editing_a_change_keeps_the_paragraphs_of_its_body() {
    let (mut cx, view) = change_screen("default", ChangeTab::Conversation);
    let body = "What: a body.\n\nWhy: it reads.\n\nTest: this one.";
    cx.update(&view, |forge, _, cx| {
        forge.start_edit(cx);
        // what the host's editor holds once the paragraphs are typed
        forge.form.as_mut().unwrap().body = ducktape_view_guest::TextField::new(body);
    });
    cx.run_until_parked();
    assert!(
        matches!(
            cx.find("forge-change-body"),
            Some(wire::Node::Field {
                multiline: true,
                ..
            })
        ),
        "the body field is the host's multi-line editor"
    );
    cx.simulate_click("forge-change-submit");
    cx.run_until_parked();
    assert!(
        cx.host().requests::<SubmitForge>().iter().any(
            |op| matches!(op, Op::ChangeEdit { n: 1, body: Some(saved), .. } if saved == body)
        ),
    );
}

/// A change's Commits tab lists the change's own commits: the log of its
/// source less what its target reaches, not the target's whole history.
#[test]
fn a_change_lists_its_own_commits() {
    let (cx, view) = change_screen("default", ChangeTab::Commits);
    let into = view.read(|forge| forge.change().map(|(change, ..)| change.into.clone()));
    let into = into.expect("the change landed");
    let asked = cx.host().requests::<crate::api::Ask>();
    assert!(
        asked.iter().any(|query| matches!(
            query,
            forge::Query::Log { exclude: Some(forge::Revision::Ref(target)), .. } if *target == into
        )),
        "{asked:?}"
    );
}

/// A merged change's Commits read between the two heads the merge joined,
/// not its refs, which move on after the merge.
#[test]
fn a_merged_change_lists_the_commits_it_merged() {
    let (cx, view) = change_screen("merged", ChangeTab::Commits);
    let heads = view.read(|forge| {
        forge
            .change()
            .and_then(|(change, ..)| change.merged_heads.clone())
    });
    let heads = heads.expect("the merge recorded its heads");
    let asked = cx.host().requests::<crate::api::Ask>();
    assert!(
        asked.iter().any(|query| matches!(
            query,
            forge::Query::Log {
                from: forge::Revision::Oid(source),
                exclude: Some(forge::Revision::Oid(target)),
                ..
            } if *source == heads.source && *target == heads.target
        )),
        "{asked:?}"
    );
}

fn focused(cx: &TestAppContext) -> Option<String> {
    cx.focused().and_then(Node::key).map(str::to_owned)
}

/// The finish form is a dialog: Tab from its last stop comes round to its
/// first, and Shift-Tab back, instead of walking out into the diff.
#[test]
fn tab_stays_inside_the_finish_form() {
    let (mut cx, _) = change_screen("reviewed", ChangeTab::Files);
    cx.simulate_click("forge-start-review");
    cx.run_until_parked();
    cx.simulate_click("forge-finish-review");
    cx.run_until_parked();
    cx.simulate_focus("forge-submit-review");
    cx.simulate_tab(true);
    assert_eq!(focused(&cx).as_deref(), Some("forge-review-body"));
    cx.simulate_tab(false);
    assert_eq!(focused(&cx).as_deref(), Some("forge-submit-review"));
}

/// Opening the finish form puts the keys in it, and Escape folds it.
#[test]
fn the_finish_form_takes_the_keys_and_escape_folds_it() {
    let (mut cx, view) = change_screen("reviewed", ChangeTab::Files);
    cx.simulate_click("forge-start-review");
    cx.run_until_parked();
    cx.simulate_click("forge-finish-review");
    cx.run_until_parked();
    assert_eq!(focused(&cx).as_deref(), Some("forge-review-body"));
    cx.simulate_dismiss("forge-finish");
    cx.run_until_parked();
    view.read(|forge| assert!(!forge.review().unwrap().finishing));
}

/// A reader who cannot write (the session dropped) sees the verdicts, and
/// picks none.
#[test]
fn a_reader_without_write_picks_no_verdict() {
    let (mut cx, view) = change_screen("default", ChangeTab::Files);
    cx.update(&view, |forge, _, cx| {
        forge.start_review(cx);
        forge.finishing(true, cx);
        forge.session.connected = false;
    });
    cx.run_until_parked();
    assert!(super::disabled(&cx, "forge-verdict-approve"));
}

/// A reader who neither owns nor writes the repository sees Close and Merge
/// off: forge refuses both to them.
#[test]
fn a_reader_without_write_gets_no_close_or_merge() {
    let (cx, _) = change_screen("default", ChangeTab::Conversation);
    for id in ["forge-close-change", "forge-merge"] {
        let Some(ducktape_view_guest::wire::Node::Container(node)) = cx.find(id) else {
            panic!("{id} is a native container");
        };
        assert_eq!(node.interactivity.aria.disabled, Some(true), "{id}");
    }
}

/// A gutter's comment button is a 24 px press target, the door's AX-017
/// floor: its box is at least 24 px tall, the number centred down it, in
/// a gutter column wider than that.
#[test]
fn a_gutter_comment_button_is_at_least_24_px_each_way() {
    use ducktape_view_guest::px;
    let (cx, _view) = change_screen("reviewed", ChangeTab::Files);
    for key in [
        "forge-gutter-src/lib.rs-old-1",
        "forge-gutter-src/lib.rs-new-1",
        "forge-gutter-src/lib.rs-new-5",
    ] {
        assert_eq!(cx.style(key).min_size.height, Some(px(24.).into()), "{key}");
        let column = cx.style(&format!("{key}-cell")).size.width;
        assert_eq!(column, Some(px(44.).into()), "{key}");
    }
}
