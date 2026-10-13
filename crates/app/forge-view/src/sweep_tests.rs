//! What the forge-qa sweep found on the real screen, each held by the
//! outcome a reader sees.
use super::{booted, opened};
use ducktape_view_guest::Role;
use ducktape_view_guest::testing::TestAppContext;
use ducktape_view_guest::wire::Node;

/// The keys of the `role` nodes under `root` drawn with a background of
/// their own: the rows a reader sees as "selected".
fn lit_rows(cx: &TestAppContext, root: &str, role: Role) -> Vec<String> {
    fn walk(cx: &TestAppContext, node: &Node, role: Role, out: &mut Vec<String>) {
        if let Some(key) = node.key()
            && node.interactivity().is_some_and(|it| it.role == Some(role))
            && node.style().is_some()
            && cx.style(key).background.is_some()
        {
            out.push(key.to_owned());
        }
        for child in node.children() {
            walk(cx, child, role, out);
        }
    }
    let mut out = Vec::new();
    walk(cx, cx.node(root), role, &mut out);
    out
}

/// At rest no row of a list is lit: the first is claimed for the keys,
/// but a reader who has not touched the arrows sees no "selected" row.
/// The arrows light the row they walk to.
#[test]
fn no_list_row_is_lit_until_the_arrows_walk_it() {
    let (cx, _) = booted("default");
    assert_eq!(
        lit_rows(&cx, "forge-repos-list", Role::Row),
        Vec::<String>::new(),
        "the repositories at rest"
    );
    let (mut cx, _) = opened("default");
    cx.simulate_click("forge-tab-commits");
    cx.run_until_parked();
    let rows = |cx: &TestAppContext| lit_rows(cx, "forge-log-list", Role::ListBoxOption);
    assert_eq!(rows(&cx), Vec::<String>::new(), "the commits at rest");
    cx.simulate_focus("forge-log-list");
    cx.simulate_key_down("forge-log-list", "down");
    let lit = rows(&cx);
    assert_eq!(lit.len(), 1, "{lit:?}");
}

/// The file tree the same: a press on a folder lights nothing, the keys do.
#[test]
fn no_tree_row_is_lit_until_the_arrows_walk_it() {
    let (mut cx, _) = opened("default");
    cx.simulate_click("forge-tab-code");
    cx.run_until_parked();
    let lit = |cx: &TestAppContext| lit_rows(cx, "forge-tree-rows", Role::TreeItem);
    assert_eq!(lit(&cx), Vec::<String>::new(), "at rest");
    cx.simulate_click("forge-tree-src");
    cx.run_until_parked();
    assert_eq!(lit(&cx), Vec::<String>::new(), "after a press on a folder");
    cx.simulate_focus("forge-tree-rows");
    cx.simulate_key_down("forge-tree-rows", "down");
    assert_eq!(lit(&cx).len(), 1, "the arrows lit the row they walked to");
}

/// An op's receipt can land after the block that carries it (a fast
/// node): the row must not wait for a next block that never comes.
#[test]
fn an_op_whose_block_lands_before_its_receipt_leaves_the_screen() {
    use super::{Changes, block, change_screen_as};
    use crate::state::ChangeTab;
    let (mut cx, view) = change_screen_as("default", ChangeTab::Conversation, 9);
    let heads = cx.host().stream::<Changes<forge::Forge>>();
    // the op goes out, and the block carrying it is heard before the
    // node's receipt lands
    cx.update(&view, |forge, _, cx| forge.merge(cx));
    heads.send(block(200, Vec::new()));
    cx.run_until_parked();
    assert!(
        !cx.has_text("Merging this change"),
        "the block retired the row: {:?}",
        cx.texts()
    );
}

/// A commit touching many files: the file strip is capped and scrolls, so
/// the diff under it keeps its height instead of being pushed off screen.
#[test]
fn a_commit_with_many_files_keeps_its_diff_under_a_scrolling_strip() {
    let (mut cx, _) = opened("default");
    cx.simulate_click("forge-tab-commits");
    cx.run_until_parked();
    cx.simulate_click("forge-commit-26607f522099476177a45a8058a93108fba5a84d");
    cx.run_until_parked();
    let strip = cx.style("forge-commit-diff-files");
    assert!(strip.max_size.height.is_some(), "the strip is capped");
    assert!(strip.overflow.y.is_some(), "and scrolls past its cap");
}

/// A new-change draft begun in Refs stays in Refs and Changes: a change's
/// own page does not wear it.
#[test]
fn a_new_change_draft_does_not_show_inside_another_change() {
    let (mut cx, _) = super::opened_as("default", 9);
    cx.simulate_click("forge-tab-refs");
    cx.run_until_parked();
    cx.simulate_click("forge-compare-clean");
    cx.run_until_parked();
    assert!(cx.find("forge-change-form").is_some(), "{:?}", cx.texts());
    cx.simulate_click("forge-tab-changes");
    cx.run_until_parked();
    cx.simulate_click("forge-change-1");
    cx.run_until_parked();
    assert!(
        cx.find("forge-change-form").is_none(),
        "the draft is not this change's: {:?}",
        cx.texts()
    );
}

/// An unborn repository's Commits tab says "No commits yet" with the
/// repository's own address, not the program's refusal.
#[test]
fn an_unborn_repository_has_no_history_and_says_so_with_its_address() {
    let (mut cx, _) = opened("unborn");
    cx.simulate_click("forge-tab-commits");
    cx.run_until_parked();
    assert!(cx.has_text("No commits yet"), "{:?}", cx.texts());
    assert!(cx.find("forge-log-loading").is_none() && !cx.has_text("Retry"));
    assert!(
        cx.texts()
            .iter()
            .any(|text| text.contains("git push duck://testnet-0a1b2c3d/forge/project main")),
        "{:?}",
        cx.texts()
    );
}

/// The tab bar counts the open changes on every tab, not only once the
/// Changes tab was visited.
#[test]
fn the_changes_tab_counts_on_every_tab() {
    let (cx, _) = opened("default");
    let tab = cx.node("forge-tab-changes");
    let texts: Vec<String> = tab
        .children()
        .iter()
        .flat_map(|child| child.children().iter().chain(std::iter::once(child)))
        .filter_map(|node| node.text().map(str::to_owned))
        .collect();
    assert!(
        texts.iter().any(|text| text.parse::<u64>().is_ok()),
        "the README tab shows the count: {texts:?}"
    );
}
