//! The desktop's pages over the seeded network: what each page shows, and
//! each write committed through the real program, as dev.
use super::*;
use crate::words;

/// The place in the texts on screen of the first text containing `part`.
fn place_of(cx: &TestAppContext, part: &str) -> usize {
    cx.texts()
        .iter()
        .position(|text| text.contains(part))
        .unwrap_or_else(|| panic!("{part:?} is not on screen: {:?}", cx.texts()))
}

fn open_home(cx: &mut TestAppContext) {
    cx.simulate_click("files/place/home");
    cx.run_until_parked();
}

fn open_notes(cx: &mut TestAppContext) {
    open_home(cx);
    cx.simulate_click("files/entry/notes.txt");
    cx.run_until_parked();
}

/// Opens `write`'s form from the page on screen.
fn begin(cx: &mut TestAppContext, write: &str) {
    cx.simulate_click(&format!("files/write/{write}"));
    cx.run_until_parked();
}

fn commit_form(cx: &mut TestAppContext) {
    cx.simulate_click("files/write/submit");
    cx.run_until_parked();
}

#[test]
fn the_places_name_the_readers_home_shared_and_the_newest_snapshots() {
    let net = seeded();
    let cx = desktop(&net);
    assert!(cx.has_text("/home/dev"), "{:?}", cx.texts());
    assert!(cx.has_text(words::SHARED));
    assert!(cx.has_text(words::EVERYTHING));
    assert!(place_of(&cx, "Say more") < place_of(&cx, "Start my notes"));
}

#[test]
fn a_home_lists_its_folders_first() {
    let net = seeded();
    let mut cx = desktop(&net);
    open_home(&mut cx);
    assert!(place_of(&cx, "docs") < place_of(&cx, "notes.txt"));
    assert!(
        cx.has_text("1 item"),
        "a folder says how many entries it holds"
    );
}

#[test]
fn a_text_file_opens_with_its_text() {
    let net = seeded();
    let mut cx = desktop(&net);
    open_notes(&mut cx);
    assert!(cx.has_text(NOTES), "{:?}", cx.texts());
    assert!(shows(&cx, "Text · 14 bytes"));
}

#[test]
fn a_file_that_is_not_text_says_how_big_it_is() {
    let net = seeded();
    let mut cx = desktop(&net);
    cx.simulate_click("files/place/shared");
    cx.run_until_parked();
    cx.simulate_click("files/entry/blob.bin");
    cx.run_until_parked();
    assert!(cx.has_text(&words::not_text(4)), "{:?}", cx.texts());
}

#[test]
fn a_link_says_where_it_points() {
    let net = seeded();
    let mut cx = desktop(&net);
    cx.simulate_click("files/place/shared");
    cx.run_until_parked();
    cx.simulate_click("files/entry/latest");
    cx.run_until_parked();
    assert!(cx.has_text(words::TARGET));
    assert!(cx.has_text("/shared/readme.txt"), "{:?}", cx.texts());
}

#[test]
fn a_crumb_opens_the_folder_it_names() {
    let net = seeded();
    let mut cx = desktop(&net);
    open_home(&mut cx);
    cx.simulate_click("files/entry/docs");
    cx.run_until_parked();
    assert!(cx.has_text("plan.md"));
    // Everything / home / dev / docs: the third crumb is dev's home
    cx.simulate_click("files/crumb/2");
    cx.run_until_parked();
    assert!(cx.has_text("notes.txt"), "{:?}", cx.texts());
}

#[test]
fn the_history_lists_snapshots_newest_first_their_authors_and_the_pins() {
    let net = seeded();
    let mut cx = desktop(&net);
    cx.simulate_click("files/tab/history");
    cx.run_until_parked();
    assert!(place_of(&cx, "Say more") < place_of(&cx, "Share the readme"));
    assert!(shows(&cx, "ada · "), "an author is named: {:?}", cx.texts());
    assert!(cx.has_text("v1"));
    assert!(
        shows(&cx, &words::snapshot(1)),
        "the pin names its snapshot"
    );
}

#[test]
fn a_snapshot_lists_what_it_changed() {
    let net = seeded();
    let mut cx = desktop(&net);
    cx.simulate_click("files/tab/history");
    cx.run_until_parked();
    cx.simulate_click("files/snapshot/2");
    cx.run_until_parked();
    assert!(cx.has_text("/shared/readme.txt"), "{:?}", cx.texts());
    assert!(cx.has_text("added"));
    assert!(
        !cx.has_text("/home/dev/notes.txt"),
        "dev's notes did not change"
    );
}

#[test]
fn the_first_snapshot_says_everything_in_it_is_new() {
    let net = seeded();
    let mut cx = desktop(&net);
    cx.simulate_click("files/tab/history");
    cx.run_until_parked();
    cx.simulate_click("files/snapshot/1");
    cx.run_until_parked();
    assert!(cx.has_text(words::FIRST_SNAPSHOT), "{:?}", cx.texts());
}

#[test]
fn another_accounts_home_offers_no_writes() {
    let net = seeded();
    let mut cx = desktop(&net);
    cx.simulate_click("files/place/everything");
    cx.run_until_parked();
    cx.simulate_click("files/entry/home");
    cx.run_until_parked();
    cx.simulate_click("files/entry/ada");
    cx.run_until_parked();
    assert!(cx.has_text("diary.txt"));
    assert!(cx.find("files/write/new-folder").is_none());
}

#[test]
fn a_new_folder_is_committed_and_listed() {
    let net = seeded();
    let mut cx = desktop(&net);
    open_home(&mut cx);
    begin(&mut cx, "new-folder");
    cx.simulate_input("files/write/name", "drafts");
    commit_form(&mut cx);
    assert!(cx.has_text("drafts"), "{:?}", cx.texts());
    assert!(cx.has_text("Committed: Make drafts"));
    assert!(cx.find("files/write").is_none(), "the form closes");
}

#[test]
fn a_new_text_file_is_committed_with_the_typed_message() {
    let net = seeded();
    let mut cx = desktop(&net);
    open_home(&mut cx);
    begin(&mut cx, "new-file");
    cx.simulate_input("files/write/name", "todo.txt");
    cx.simulate_input("files/write/text", "buy milk");
    cx.simulate_input("files/write/message", "Add a todo");
    commit_form(&mut cx);
    assert!(cx.has_text("Committed: Add a todo"));
    cx.simulate_click("files/entry/todo.txt");
    cx.run_until_parked();
    assert!(cx.has_text("buy milk"), "{:?}", cx.texts());
}

#[test]
fn an_edit_writes_the_new_text_over_the_old() {
    let net = seeded();
    let mut cx = desktop(&net);
    open_notes(&mut cx);
    begin(&mut cx, "edit");
    assert!(
        cx.has_text(NOTES),
        "the field starts from the text as it is"
    );
    cx.simulate_input("files/write/text", "rewritten");
    commit_form(&mut cx);
    assert!(cx.has_text("rewritten"), "{:?}", cx.texts());
    assert!(!cx.has_text(NOTES));
}

#[test]
fn a_rename_moves_the_page_to_the_new_path() {
    let net = seeded();
    let mut cx = desktop(&net);
    open_notes(&mut cx);
    begin(&mut cx, "rename");
    cx.simulate_input("files/write/name", "/home/dev/docs/notes.txt");
    commit_form(&mut cx);
    assert!(cx.has_text(NOTES), "{:?}", cx.texts());
    assert!(cx.has_text("docs"), "the crumbs name its new folder");
    cx.simulate_click("files/crumb/2");
    cx.run_until_parked();
    assert!(!cx.has_text("notes.txt"), "{:?}", cx.texts());
}

#[test]
fn a_removed_file_leaves_its_folder_on_screen() {
    let net = seeded();
    let mut cx = desktop(&net);
    open_notes(&mut cx);
    begin(&mut cx, "remove");
    assert!(cx.has_text(&words::removes("/home/dev/notes.txt", false)));
    commit_form(&mut cx);
    assert!(cx.has_text("docs"), "{:?}", cx.texts());
    assert!(!cx.has_text("notes.txt"));
    assert!(cx.has_text("Committed: Remove notes.txt"));
}

#[test]
fn a_name_files_would_refuse_is_said_on_its_field() {
    let net = seeded();
    let mut cx = desktop(&net);
    open_home(&mut cx);
    begin(&mut cx, "new-folder");
    cx.simulate_input("files/write/name", "a/b");
    commit_form(&mut cx);
    assert!(cx.has_text(words::ONE_NAME), "{:?}", cx.texts());
    assert!(cx.find("files/write").is_some(), "the form stays open");
}

#[test]
fn a_commit_refused_as_stale_says_why_in_its_form() {
    let net = seeded();
    let mut cx = desktop(&net);
    open_home(&mut cx);
    begin(&mut cx, "new-file");
    // dev writes the same path from another device meanwhile
    commit(
        &net,
        net.me(),
        "Elsewhere",
        vec![put("/home/dev/x.txt", b"first")],
    );
    cx.run_until_parked();
    cx.simulate_input("files/write/name", "x.txt");
    commit_form(&mut cx);
    assert!(
        shows(&cx, "changed since the base snapshot"),
        "{:?}",
        cx.texts()
    );
    assert!(
        cx.find("files/write").is_some(),
        "the form stays open to retry"
    );
}
