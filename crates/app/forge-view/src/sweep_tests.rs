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
