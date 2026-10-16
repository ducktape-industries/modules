//! Forge under a finger (`ui::phone`): each phone screen in the states the
//! Expo app draws it (full, empty, being read, refused) and each of its
//! actions; the width sweep at 360, 390 and 430 (SPEC §8.1d-1); and the
//! desktop kept under a pointer at every width.
use super::*;
use view::testing;

/// Forge on a 390 wide phone, seated as Ada (account 1: `project`'s owner
/// and change 1's author), once `mode`'s answers are set.
fn phone(mode: &'static str) -> TestAppContext {
    let mut cx = TestAppContext::new();
    configure(&mut cx, mode);
    seat_phone(cx)
}

/// Forge on a phone whose reads `answer` answers.
fn phone_answered(
    answer: impl Fn(&Query) -> Result<Reply, view::host::Error> + 'static,
) -> TestAppContext {
    let mut cx = TestAppContext::new();
    configure(&mut cx, "default");
    cx.host().handle::<Ask>(move |query| answer(&query));
    seat_phone(cx)
}

fn seat_phone(mut cx: TestAppContext) -> TestAppContext {
    let props = cx.host().stream::<HostSession>();
    cx.open::<Forge>();
    cx.simulate_touch(true);
    cx.simulate_resize(390., 844.);
    cx.run_until_parked();
    props.send(seat(b"owner", Some(1)));
    cx.run_until_parked();
    cx
}

fn press(cx: &mut TestAppContext, id: &str) {
    cx.simulate_click(id);
    cx.run_until_parked();
}

/// `project` open on the phone.
fn repository(mode: &'static str) -> TestAppContext {
    let mut cx = phone(mode);
    press(&mut cx, "forge-repo-project");
    cx
}

/// Change 1 of `project` open on the phone.
fn change(mode: &'static str) -> TestAppContext {
    let mut cx = repository(mode);
    press(&mut cx, "forge-tab-changes");
    press(&mut cx, "forge-change-project-1");
    cx
}

/// `src/lib.rs` of change 1 open on its own page.
fn file(mode: &'static str) -> TestAppContext {
    let mut cx = change(mode);
    press(&mut cx, "forge-change-tab-files");
    press(&mut cx, "forge-file-src/lib.rs");
    cx
}

fn submitted(cx: &TestAppContext) -> Vec<forge::Op> {
    cx.host().requests::<SubmitForge>()
}

// ------------------------------------------------------------------ Forge

#[test]
fn the_root_lists_what_needs_my_judgment_then_every_repository() {
    let cx = phone("default");
    assert!(cx.find("forge-page-repos").is_some());
    assert_eq!(
        cx.back_handler(),
        None,
        "the root has nothing to go back to"
    );
    assert!(cx.has_text("Forge"));
    assert!(cx.has_text("testnet · Ada"), "{:?}", cx.texts());
    assert!(cx.has_text("Needs my judgment"));
    assert!(cx.find("forge-judgment-list").is_some());
    assert!(cx.has_text("Repositories"));
    assert!(cx.find("forge-repo-project").is_some());
}

#[test]
fn a_network_without_repositories_says_how_one_begins() {
    let cx = phone("empty");
    assert!(cx.has_text("No repositories yet"));
    assert!(cx.has_text("Create one with New repository, then push to it."));
}

#[test]
fn a_filter_that_keeps_no_repository_says_nothing_matches() {
    let mut cx = phone("default");
    cx.simulate_input("forge-repos-search", "nothing-by-this-name");
    cx.run_until_parked();
    assert!(cx.has_text("Nothing matches"));
    assert!(cx.find("forge-repo-project").is_none());
}

#[test]
fn the_repositories_say_so_while_they_are_read() {
    let mut cx = TestAppContext::new();
    configure(&mut cx, "default");
    cx.host().never::<Ask>();
    let cx = seat_phone(cx);
    assert!(cx.has_text("Reading the repositories…"));
}

#[test]
fn a_refused_repository_list_says_why_and_retries() {
    let mut cx = phone_answered(|_| Err(refusal("refused-object-not-held")));
    let sentence = "object ffffffffffffffffffffffffffffffffffffffff is not held by this node";
    assert!(cx.has_text(sentence), "{:?}", cx.texts());
    let asked = cx.host().requests::<Ask>().len();
    press(&mut cx, "forge-repos-list-retry");
    assert!(
        cx.host().requests::<Ask>().len() > asked,
        "Retry asks again"
    );
}

#[test]
fn new_repository_opens_a_sheet_that_refuses_a_bad_name_then_creates() {
    let mut cx = phone("default");
    press(&mut cx, "forge-new-repo");
    assert!(cx.find("forge-new-repo-sheet").is_some());
    assert!(cx.back_handler().is_some(), "Back closes the sheet");
    cx.simulate_input("forge-new-repo-name", "bad name");
    press(&mut cx, "forge-new-repo-create");
    let refusal = "A repository name is letters, digits, dot, dash or underscore";
    assert!(cx.has_text(refusal));
    let view::wire::Node::Field { options, .. } = cx.node("forge-new-repo-name") else {
        panic!("the name is a field");
    };
    assert_eq!(options.invalid, Some(view::accesskit::Invalid::True));
    assert_eq!(
        options.description.as_deref(),
        Some(refusal),
        "a screen reader hears why"
    );
    cx.simulate_input("forge-new-repo-name", "garden");
    press(&mut cx, "forge-new-repo-create");
    assert!(cx.find("forge-new-repo-sheet").is_none());
    assert!(
        submitted(&cx)
            .iter()
            .any(|op| matches!(op, forge::Op::Create { repo, .. } if repo == "garden"))
    );
}

// ------------------------------------------------------------------ a repository

#[test]
fn a_repository_names_its_owner_its_ref_and_six_tabs() {
    let cx = repository("default");
    assert!(cx.find("forge-page-repo").is_some());
    assert!(cx.find("forge-page-repo-back").is_some());
    assert!(cx.back_handler().is_some());
    assert!(cx.find("forge-ref-picker").is_some(), "the ref button");
    for tab in ["readme", "code", "commits", "changes", "refs", "settings"] {
        assert!(cx.find(&format!("forge-tab-{tab}")).is_some(), "{tab}");
    }
    assert!(
        cx.find("forge-readme-body").is_some(),
        "it opens on its README"
    );
}

#[test]
fn about_opens_as_a_sheet_and_back_closes_it_first() {
    let mut cx = repository("default");
    press(&mut cx, "forge-dock-about");
    assert!(cx.find("forge-about-sheet").is_some());
    assert!(cx.has_text("Default head"));
    cx.simulate_back();
    cx.run_until_parked();
    assert!(cx.find("forge-about-sheet").is_none());
    assert!(cx.find("forge-page-repo").is_some(), "the page stays");
}

#[test]
fn back_closes_what_is_drawn_not_an_about_left_open_under_a_change() {
    let mut cx = repository("default");
    press(&mut cx, "forge-dock-about");
    press(&mut cx, "forge-tab-changes");
    press(&mut cx, "forge-change-project-1");
    assert!(
        cx.find("forge-about-sheet").is_none(),
        "a change has no About"
    );
    cx.simulate_back();
    cx.run_until_parked();
    assert!(
        cx.find("forge-page-change").is_none(),
        "Back pops the change"
    );
    assert!(cx.find("forge-page-repo").is_some());
}

#[test]
fn copy_puts_the_address_on_the_clipboard_and_its_glyph_says_so() {
    let mut cx = repository("default");
    press(&mut cx, "forge-repo-copy");
    assert_eq!(
        cx.interactivity("forge-repo-copy").aria.label.as_deref(),
        Some("Address copied")
    );
}

#[test]
fn the_ref_button_opens_the_ref_sheet_and_a_pick_closes_it() {
    let mut cx = repository("default");
    press(&mut cx, "forge-ref-picker");
    assert!(cx.find("forge-ref-picker-sheet").is_some());
    assert!(cx.has_text("Pick a ref"));
    press(&mut cx, "forge-ref-refs/heads/clean");
    assert!(cx.find("forge-ref-picker-sheet").is_none());
    assert!(cx.has_text("clean"));
}

#[test]
fn every_tab_draws_its_body() {
    let mut cx = repository("default");
    for (tab, body) in [
        ("code", "forge-tree-list"),
        ("commits", "forge-log-list"),
        ("changes", "forge-change-list"),
        ("refs", "forge-refs-list"),
        ("settings", "forge-settings"),
        ("readme", "forge-readme-body"),
    ] {
        press(&mut cx, &format!("forge-tab-{tab}"));
        assert!(cx.find(body).is_some(), "{tab} draws {body}");
    }
}

// ------------------------------------------------------------------ a change

#[test]
fn a_change_shows_its_state_who_wants_what_and_four_tabs() {
    let cx = change("default");
    assert!(cx.find("forge-page-change").is_some());
    assert!(cx.find("forge-change-state").is_some());
    assert!(
        cx.texts().iter().any(|text| text.contains(" wants ")),
        "{:?}",
        cx.texts()
    );
    for tab in ["conversation", "commits", "files", "details"] {
        assert!(
            cx.find(&format!("forge-change-tab-{tab}")).is_some(),
            "{tab}"
        );
    }
    assert!(cx.find("forge-merge").is_some());
    assert!(cx.find("forge-start-review").is_some());
    assert!(cx.find("forge-reply").is_some());
}

/// The strip says each tab's count whichever tab is open, as Expo's
/// does: the change's conversation and its diff are read with it.
#[test]
fn the_strip_counts_every_tab_whichever_is_open() {
    let mut cx = change("reviewed");
    let said = |cx: &TestAppContext, tab: &str| -> Vec<String> {
        let tab = cx.node(&format!("forge-change-tab-{tab}"));
        marked(tab).into_iter().map(|(text, _)| text).collect()
    };
    assert_eq!(said(&cx, "files").len(), 2, "{:?}", said(&cx, "files"));
    press(&mut cx, "forge-change-tab-files");
    let conversation = said(&cx, "conversation");
    assert_eq!(conversation.len(), 2, "{conversation:?}");
}

#[test]
fn the_conversation_lists_the_reviews_as_advisory_and_the_replies() {
    let cx = change("reviewed");
    assert!(cx.has_text("Reviews"));
    assert!(cx.has_text("advisory"));
    assert!(cx.has_text("Replies"));
    assert!(!cx.has_text("Nobody has reviewed this change yet."));
}

#[test]
fn a_change_nobody_reviewed_says_so() {
    let cx = change("default");
    assert!(
        cx.has_text("Nobody has reviewed this change yet."),
        "{:?}",
        cx.texts()
    );
}

#[test]
fn merge_says_why_it_is_off_over_the_bar() {
    let cx = change("diverged");
    assert!(cx.has_text(
        "The endpoints diverged. Merge with git and push the result, then Merge turns on."
    ));
    assert!(disabled(&cx, "forge-merge"));
}

#[test]
fn a_refused_change_says_why_in_its_place() {
    let cx = {
        let mut cx = phone_answered(|query| match query {
            Query::Change { .. } => Err(refusal("refused-object-not-held")),
            other => Ok(answer(other, "default")),
        });
        press(&mut cx, "forge-repo-project");
        press(&mut cx, "forge-tab-changes");
        press(&mut cx, "forge-change-project-1");
        cx
    };
    assert!(
        cx.find("forge-change-refused").is_some(),
        "{:?}",
        cx.texts()
    );
    assert!(cx.find("forge-change-retry").is_some());
}

#[test]
fn start_review_pins_a_review_and_opens_the_files() {
    let mut cx = change("default");
    press(&mut cx, "forge-start-review");
    assert!(cx.find("forge-files").is_some(), "the Files tab");
    assert!(cx.has_text("Tick a file once you have read it."));
}

#[test]
fn more_offers_edit_close_and_copy_link_and_close_asks_again() {
    let mut cx = change("default");
    press(&mut cx, "forge-change-more");
    assert!(cx.find("forge-change-actions").is_some());
    assert!(cx.find("forge-change-edit").is_some());
    assert!(cx.find("forge-change-copy-link").is_some());
    press(&mut cx, "forge-change-close");
    assert!(cx.has_text("Closing is final: a closed change cannot be reopened."));
    assert!(submitted(&cx).is_empty(), "the first press only asks");
    press(&mut cx, "forge-close-confirm");
    assert!(
        submitted(&cx)
            .iter()
            .any(|op| matches!(op, forge::Op::ChangeClose { n: 1, .. }))
    );
    assert!(cx.find("forge-change-actions").is_none());
}

#[test]
fn the_files_tab_lists_each_file_and_a_tick_marks_it_read() {
    let mut cx = change("default");
    press(&mut cx, "forge-change-tab-files");
    assert!(cx.find("forge-file-src/lib.rs").is_some());
    assert!(cx.find("forge-all-files").is_some());
    press(&mut cx, "forge-viewed-src/lib.rs");
    assert!(
        cx.find("forge-page-change").is_some(),
        "the tick does not open the file"
    );
    assert_eq!(
        cx.interactivity("forge-viewed-src/lib.rs").aria.toggled,
        Some(view::Toggled::True)
    );
}

#[test]
fn details_name_the_reviews_the_merge_status_the_line_comments_and_the_channel() {
    let mut cx = change("reviewed");
    press(&mut cx, "forge-change-tab-details");
    assert!(cx.find("forge-details").is_some());
    for said in [
        "Merge status",
        "Mergeability",
        "Source head",
        "Line comments",
        "Channel",
    ] {
        assert!(cx.has_text(said), "{said}");
    }
}

// ------------------------------------------------------------------ one file of a change

#[test]
fn a_file_page_walks_its_files_and_says_how_to_comment() {
    let cx = file("default");
    assert!(cx.find("forge-page-file").is_some());
    assert!(cx.has_text("lib.rs"));
    assert!(cx.find("forge-file-previous").is_some());
    assert!(cx.find("forge-file-next").is_some());
    assert!(cx.has_text("Comment on any line by pressing its number."));
}

#[test]
fn pressing_a_line_number_opens_the_composer_and_stage_keeps_the_comment() {
    let mut cx = file("default");
    let first = (0..64)
        .map(|index| format!("forge-diff-gutter-{index}"))
        .find(|id| cx.find(id).is_some())
        .unwrap_or_else(|| panic!("no line number to press"));
    press(&mut cx, &first);
    assert!(cx.find("forge-comment-body").is_some(), "the composer");
    cx.simulate_input("forge-comment-body", "trim first");
    press(&mut cx, "forge-comment-stage");
    assert!(cx.has_text("Reviewing · 1 staged"));
    assert!(cx.has_text("Your pending comment"));
}

#[test]
fn a_published_comment_sits_under_its_line() {
    let cx = file("reviewed");
    assert!(
        cx.texts()
            .iter()
            .any(|text| text.starts_with("line ") && text.contains(" · block ")),
        "{:?}",
        cx.texts()
    );
}

// ------------------------------------------------------------------ Finish your review

#[test]
fn finish_review_opens_the_sheet_and_submit_sends_one_review() {
    let mut cx = change("default");
    press(&mut cx, "forge-start-review");
    press(&mut cx, "forge-change-tab-conversation");
    press(&mut cx, "forge-finish-review");
    assert!(cx.find("forge-finish-sheet").is_some());
    assert!(cx.has_text("Finish your review"));
    assert!(
        cx.texts()
            .iter()
            .any(|text| text.starts_with("0 line comments staged against "))
    );
    press(&mut cx, "forge-verdict-approve");
    press(&mut cx, "forge-submit-review");
    let sent = submitted(&cx);
    assert!(sent.iter().any(|op| matches!(op, forge::Op::ReviewSubmit { review, .. } if review.verdict == forge::Verdict::Approve)));
}

#[test]
fn discard_drops_the_review() {
    let mut cx = change("default");
    press(&mut cx, "forge-start-review");
    press(&mut cx, "forge-change-tab-conversation");
    press(&mut cx, "forge-finish-review");
    press(&mut cx, "forge-discard-review-sheet");
    assert!(cx.find("forge-finish-sheet").is_none());
    assert!(
        cx.find("forge-start-review").is_some(),
        "no review under way"
    );
}

// ------------------------------------------------------------------ a commit

#[test]
fn a_commit_shows_its_facts_its_files_and_their_diff() {
    let mut cx = repository("default");
    press(&mut cx, "forge-tab-commits");
    press(
        &mut cx,
        "forge-commit-26607f522099476177a45a8058a93108fba5a84d",
    );
    assert!(cx.find("forge-page-commit").is_some());
    for said in ["Commit", "Tree", "Parents", "Author", "Files"] {
        assert!(cx.has_text(said), "{said}: {:?}", cx.texts());
    }
    assert!(cx.find("forge-commit-diff").is_some());
}

// ------------------------------------------------------------------ a file of the Code tab

#[test]
fn a_file_shows_its_lines_and_wrap_flips() {
    let mut cx = repository("default");
    press(&mut cx, "forge-tab-code");
    press(&mut cx, "forge-tree-empty.txt");
    assert!(cx.find("forge-page-blob").is_some());
    assert!(cx.find("forge-blob-lines").is_some(), "{:?}", cx.texts());
    press(&mut cx, "forge-wrap");
    assert!(cx.find("forge-blob-lines").is_some());
}

#[test]
fn a_markdown_file_reads_as_its_document_until_source() {
    let mut cx = repository("default");
    press(&mut cx, "forge-tab-code");
    press(&mut cx, "forge-tree-README.md");
    assert!(cx.find("forge-blob-document").is_some());
    press(&mut cx, "forge-source");
    assert!(cx.find("forge-blob-lines").is_some());
    assert!(
        cx.find("forge-wrap").is_some(),
        "Wrap once the source shows"
    );
}

// ------------------------------------------------------------------ New change

#[test]
fn edit_opens_the_form_filled_and_save_sends_the_edit() {
    let mut cx = change("default");
    press(&mut cx, "forge-change-more");
    press(&mut cx, "forge-change-edit");
    assert!(cx.find("forge-page-change-form").is_some());
    assert!(cx.has_text("Edit change #1"));
    press(&mut cx, "forge-change-send");
    assert!(
        submitted(&cx)
            .iter()
            .any(|op| matches!(op, forge::Op::ChangeEdit { n: 1, .. }))
    );
}

#[test]
fn compare_opens_a_new_change_that_refuses_a_blank_title() {
    let mut cx = repository("default");
    press(&mut cx, "forge-tab-refs");
    press(&mut cx, "forge-compare-clean");
    assert!(cx.has_text("New change"));
    press(&mut cx, "forge-change-send");
    assert!(cx.has_text("A change needs a title"));
    cx.simulate_input("forge-change-title", "Clean up");
    assert!(
        !cx.has_text("A change needs a title"),
        "typing takes the refusal away"
    );
}

/// New change from the Changes tab (Expo 38): no source yet; Source opens
/// the ref sheet titled "Source", a pick fills it and the endpoints
/// compare; Open sends the change from it into the default head.
#[test]
fn new_change_from_the_changes_tab_picks_its_source_on_the_ref_sheet() {
    let mut cx = repository("default");
    press(&mut cx, "forge-tab-changes");
    press(&mut cx, "forge-new-change");
    assert!(cx.has_text("Pick a source"));
    assert!(!cx.texts().iter().any(|text| text.contains(" ahead · ")));
    press(&mut cx, "forge-change-from");
    assert!(cx.find("forge-change-from-sheet").is_some());
    assert!(cx.has_text("Source"));
    press(&mut cx, "forge-ref-refs/heads/clean");
    assert!(
        cx.find("forge-change-from-sheet").is_none(),
        "a pick closes it"
    );
    assert!(cx.has_text("clean"));
    assert!(
        cx.texts().iter().any(|text| text.contains(" ahead · ")),
        "{:?}",
        cx.texts()
    );
    cx.simulate_input("forge-change-title", "Clean up");
    press(&mut cx, "forge-change-send");
    let opened = submitted(&cx).into_iter().find_map(|op| match op {
        forge::Op::ChangeOpen { from, into, .. } => Some((from, into)),
        _ => None,
    });
    assert_eq!(
        opened,
        Some((
            forge::Revision::Ref(b"refs/heads/clean".to_vec()),
            b"refs/heads/main".to_vec()
        ))
    );
}

// ------------------------------------------------------------------ the sweep

/// Every phone screen fits each phone width under the touch ladder, and
/// Back is set exactly while there is somewhere to go back to (rule 13).
#[test]
fn forge_fits_every_phone() {
    let mut cx = phone("reviewed");
    testing::sweep(&mut cx, |cx| {
        assert_eq!(cx.back_handler(), None, "nothing to go back to");
        press(cx, "forge-new-repo");
        cx.simulate_back();
        press(cx, "forge-repo-project");
        press(cx, "forge-dock-about");
        cx.simulate_back();
        press(cx, "forge-ref-picker");
        cx.simulate_dismiss("forge-ref-picker-sheet");
        for tab in ["code", "commits", "changes", "refs", "settings"] {
            press(cx, &format!("forge-tab-{tab}"));
        }
        press(cx, "forge-tab-commits");
        press(cx, "forge-commit-26607f522099476177a45a8058a93108fba5a84d");
        cx.simulate_back();
        cx.run_until_parked();
        press(cx, "forge-tab-code");
        press(cx, "forge-tree-README.md");
        cx.simulate_back();
        cx.run_until_parked();
        press(cx, "forge-tab-changes");
        press(cx, "forge-new-change");
        press(cx, "forge-change-from");
        cx.simulate_dismiss("forge-change-from-sheet");
        cx.simulate_back();
        cx.run_until_parked();
        press(cx, "forge-change-project-1");
        press(cx, "forge-change-more");
        cx.simulate_dismiss("forge-change-actions");
        for tab in ["commits", "details", "files"] {
            press(cx, &format!("forge-change-tab-{tab}"));
        }
        press(cx, "forge-file-src/lib.rs");
        press(cx, "forge-start-review");
        press(cx, "forge-finish-review");
        cx.simulate_dismiss("forge-finish-sheet");
        press(cx, "forge-discard-review");
        let mut popped = 0;
        while cx.back_handler().is_some() {
            cx.simulate_back();
            cx.run_until_parked();
            popped += 1;
            assert!(popped < 8, "Back never reaches the root");
        }
        assert!(cx.find("forge-page-repos").is_some());
    });
}

/// Under a pointer the desktop draws at every width, never a phone page.
#[test]
fn a_pointer_keeps_the_desktop_at_every_width() {
    let (mut cx, _) = opened("default");
    for width in [360., 640., 1180.] {
        cx.simulate_resize(width, 760.);
        cx.run_until_parked();
        assert!(cx.find("forge-columns").is_some(), "the desktop at {width}");
        assert!(
            cx.find("forge-page-repo").is_none(),
            "no phone page at {width}"
        );
    }
}
