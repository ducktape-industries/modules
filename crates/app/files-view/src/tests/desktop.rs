//! The desktop's pages over the seeded network: what each page shows, a
//! refused read, and each write committed through the real program, as dev.
use std::cell::{Cell, RefCell};
use std::rc::Rc;

use files::{BlobId, DiffKind, Node};
use view::methods::{FilePick, FileRead, Picked, Query as Ask, Submit, Submitted};
use view::prelude::Error;
use view::wire;

use super::*;
use crate::state::{Holds, SHOW_MOST};
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
    cx.simulate_click(&format!("files/begin/{write}"));
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
    cx.simulate_click("files/crumb:/home/dev");
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
    assert!(cx.has_text(words::diff_kind(DiffKind::Added)));
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
    assert!(cx.find("files/begin/new-folder").is_none());
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
    cx.simulate_click("files/crumb:/home/dev");
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
        "the form stays open with the program's words for why"
    );
}

#[test]
fn the_arrow_keys_on_the_tabs_open_the_history() {
    let net = seeded();
    let mut cx = desktop(&net);
    cx.simulate_focus("files-tabs");
    cx.simulate_key_down("files-tabs", "right");
    cx.run_until_parked();
    assert!(cx.has_text("v1"), "the pins show: {:?}", cx.texts());
    cx.simulate_key_down("files-tabs", "left");
    cx.run_until_parked();
    assert!(cx.has_text(words::EVERYTHING));
}

#[test]
fn a_pin_opens_the_snapshot_it_names() {
    let net = seeded();
    let mut cx = desktop(&net);
    cx.simulate_click("files/tab/history");
    cx.run_until_parked();
    cx.simulate_click("files/pin/v1");
    cx.run_until_parked();
    assert!(cx.has_text("Start my notes"), "{:?}", cx.texts());
    assert!(cx.has_text(words::FIRST_SNAPSHOT));
}

#[test]
fn every_page_says_why_files_refused_its_read() {
    let net = seeded();
    let mut cx = desktop(&net);
    cx.host()
        .refuse::<Ask<files::Files>>("unavailable", "the node is away");
    cx.simulate_click("files/tab/history");
    cx.run_until_parked();
    assert!(
        cx.find("files-history-refused").is_some(),
        "{:?}",
        cx.texts()
    );
    cx.simulate_click("files/tab/places");
    cx.run_until_parked();
    cx.simulate_click("files/place/shared");
    cx.run_until_parked();
    assert!(
        cx.find("files-folder-refused").is_some(),
        "{:?}",
        cx.texts()
    );
    assert!(cx.has_text("the node is away"));
}

#[test]
fn a_refused_snapshot_is_asked_once_and_again_on_retry() {
    let net = seeded();
    let mut cx = desktop(&net);
    let asked = Rc::new(Cell::new(0));
    let counted = asked.clone();
    let chain = net.clone();
    cx.host()
        .handle::<Ask<files::Files>>(move |query| match query {
            files::Query::Snapshot { .. } => {
                counted.set(counted.get() + 1);
                Err(view::prelude::Error::new("unavailable", "the blob is gone"))
            }
            query => chain.query(files::PROGRAM, &query),
        });
    cx.simulate_click("files/tab/history");
    cx.run_until_parked();
    cx.simulate_click("files/snapshot/2");
    cx.run_until_parked();
    assert!(cx.has_text("the blob is gone"), "{:?}", cx.texts());
    assert_eq!(asked.get(), 1, "a refusal is not asked again by drawing it");
    cx.simulate_click("files-snapshot-retry");
    cx.run_until_parked();
    assert_eq!(asked.get(), 2);
}

#[test]
fn a_refused_commit_says_why_in_its_form() {
    let net = seeded();
    let mut cx = desktop(&net);
    cx.host()
        .refuse::<Submit<files::Files>>("refused", "the ledger is full");
    open_home(&mut cx);
    begin(&mut cx, "new-folder");
    cx.simulate_input("files/write/name", "drafts");
    commit_form(&mut cx);
    assert!(cx.find("files/write/refused").is_some(), "{:?}", cx.texts());
    assert!(cx.has_text("That didn’t go through: the ledger is full"));
}

#[test]
fn another_account_signing_in_closes_the_open_form() {
    let net = seeded();
    let mut cx = desktop(&net);
    open_home(&mut cx);
    begin(&mut cx, "new-folder");
    let ada = net.person("ada");
    cx.host().stream::<HostSession>().send(Session {
        account: Some(ada),
        ..net.session()
    });
    cx.run_until_parked();
    assert!(cx.find("files/write").is_none(), "{:?}", cx.texts());
}

#[test]
fn a_renamed_folder_stays_on_screen_at_its_new_path() {
    let net = seeded();
    let mut cx = desktop(&net);
    open_home(&mut cx);
    cx.simulate_click("files/entry/docs");
    cx.run_until_parked();
    begin(&mut cx, "rename");
    cx.simulate_input("files/write/name", "/home/dev/papers");
    commit_form(&mut cx);
    assert!(cx.find("files/crumb:/home/dev").is_some());
    assert!(cx.has_text("papers"), "{:?}", cx.texts());
    assert!(cx.has_text("plan.md"));
}

#[test]
fn a_removed_folder_returns_to_the_folder_it_was_in() {
    let net = seeded();
    let mut cx = desktop(&net);
    open_home(&mut cx);
    cx.simulate_click("files/entry/docs");
    cx.run_until_parked();
    begin(&mut cx, "remove");
    assert!(cx.has_text(&words::removes("/home/dev/docs", true)));
    commit_form(&mut cx);
    assert!(cx.has_text("notes.txt"), "{:?}", cx.texts());
    assert!(cx.find("files/entry/docs").is_none());
}

#[test]
fn a_wide_pane_lists_the_places_down_the_side_in_place_of_the_tabs() {
    let net = seeded();
    let mut cx = desktop_at(&net, 1000.);
    assert!(cx.find("files-side").is_some(), "{:?}", cx.texts());
    assert!(cx.find("files-tabs").is_none());
    assert!(
        cx.find("files/side/pin/v1").is_some(),
        "the pins are listed"
    );
    cx.simulate_click("files/side/home");
    cx.run_until_parked();
    assert!(cx.has_text(words::KIND), "a wide folder lists in columns");
    assert!(
        cx.has_text(words::kind(&file(), Holds::Text)),
        "{:?}",
        cx.texts()
    );
    cx.simulate_resize(700., 900.);
    cx.run_until_parked();
    assert!(cx.find("files-side").is_none());
    assert!(
        cx.find("files-tabs").is_some(),
        "narrowed, the tabs are back"
    );
}

#[test]
fn split_a_file_opens_beside_its_folder_and_a_folder_there_takes_its_place() {
    let net = seeded();
    let mut cx = desktop_at(&net, 1440.);
    cx.simulate_click("files/side/home");
    cx.run_until_parked();
    assert!(cx.has_text(words::PICK_A_FILE));
    cx.simulate_click("files/entry/notes.txt");
    cx.run_until_parked();
    assert!(cx.find("files-list").is_some() && cx.find("files-detail").is_some());
    assert!(cx.has_text(NOTES), "the file shows: {:?}", cx.texts());
    assert!(cx.has_text("docs"), "its folder stays listed beside it");
    cx.simulate_click("files/entry/docs");
    cx.run_until_parked();
    assert!(cx.has_text("plan.md"), "{:?}", cx.texts());
    assert!(
        cx.has_text(words::PICK_A_FILE),
        "the folder took the file's place"
    );
}

#[test]
fn split_a_snapshot_opens_beside_the_history() {
    let net = seeded();
    let mut cx = desktop_at(&net, 1440.);
    cx.simulate_click("files/side/pin/v1");
    cx.run_until_parked();
    assert!(cx.has_text(words::FIRST_SNAPSHOT), "{:?}", cx.texts());
    assert!(
        cx.has_text("Say more"),
        "the history stays listed beside it"
    );
}

#[test]
fn an_image_shows_as_its_picture() {
    let net = seeded();
    let mut cx = desktop(&net);
    cx.simulate_click("files/place/shared");
    cx.run_until_parked();
    cx.simulate_click("files/entry/dot.png");
    cx.run_until_parked();
    let under = format!(
        "{} · {}",
        words::kind(&file(), Holds::Picture),
        words::size(PNG.len() as u64)
    );
    assert!(shows(&cx, &under), "{:?}", cx.texts());
    assert!(
        matches!(cx.find("files-picture"), Some(wire::Node::Image { .. })),
        "{:?}",
        cx.texts()
    );
}

/// Whether the side's row for `key` is the one picked.
fn picked(cx: &TestAppContext, key: &str) -> bool {
    cx.interactivity(&format!("files/side/{key}")).aria.selected == Some(true)
}

#[test]
fn a_pin_made_while_a_folder_is_open_joins_the_side() {
    let net = seeded();
    let mut cx = desktop_at(&net, 1000.);
    cx.simulate_click("files/side/home");
    cx.run_until_parked();
    let snapshot = commit(
        &net,
        net.me(),
        "More notes",
        vec![put("/home/dev/more.txt", b"more")],
    );
    let pin = Op::Pin {
        name: "v2".into(),
        snapshot,
    };
    net.submit(net.me(), files::PROGRAM, &pin)
        .expect("a pin on a minted snapshot");
    cx.run_until_parked();
    assert!(cx.find("files/side/pin/v2").is_some(), "{:?}", cx.texts());
}

#[test]
fn a_restored_view_reads_what_landed_while_it_was_away() {
    let net = seeded();
    let mut cx = desktop_at(&net, 1000.);
    cx.simulate_click("files/side/home");
    cx.run_until_parked();
    let bytes = cx.snapshot().unwrap();
    let snapshot = commit(
        &net,
        net.me(),
        "Share more",
        vec![put("/shared/more.txt", b"more")],
    );
    let pin = Op::Pin {
        name: "v2".into(),
        snapshot,
    };
    net.submit(net.me(), files::PROGRAM, &pin)
        .expect("a pin on a minted snapshot");
    cx.restore::<Files>(&bytes).unwrap();
    cx.run_until_parked();
    assert!(cx.find("files/side/pin/v2").is_some(), "{:?}", cx.texts());
}

#[test]
fn the_side_picks_the_place_or_the_pin_the_page_is_under() {
    let net = seeded();
    let mut cx = desktop_at(&net, 1000.);
    cx.simulate_click("files/side/home");
    cx.run_until_parked();
    cx.simulate_click("files/entry/notes.txt");
    cx.run_until_parked();
    assert!(picked(&cx, "home") && !picked(&cx, "everything"));
    cx.simulate_click("files/side/shared");
    cx.run_until_parked();
    cx.simulate_click("files/entry/readme.txt");
    cx.run_until_parked();
    assert!(picked(&cx, "shared"));
    cx.simulate_click("files/side/pin/v1");
    cx.run_until_parked();
    assert!(picked(&cx, "pin/v1") && !picked(&cx, "history"));
    // one Tab stop: the arrows open the item over or under
    cx.simulate_focus("files-side-list");
    cx.simulate_key_down("files-side-list", "up");
    cx.run_until_parked();
    assert!(picked(&cx, "history"), "{:?}", cx.texts());
    assert!(cx.has_text("Say more"), "the history opened");
    cx.simulate_key_down("files-side-list", "down");
    cx.run_until_parked();
    assert!(picked(&cx, "pin/v1"));
}

/// The snapshot each `Read` a page asked was at.
type Reads = Rc<RefCell<Vec<Option<BlobId>>>>;

/// A file at `/shared/big.png` that begins as a PNG and is `size` bytes in
/// all, committed by dev; then its page, the `Read`s it asked kept.
fn picture_page(size: usize) -> (TestAppContext, Network, Reads) {
    let net = seeded();
    let mut bytes = PNG.to_vec();
    bytes.resize(size, 0);
    commit(
        &net,
        net.me(),
        "A big picture",
        vec![put("/shared/big.png", &bytes)],
    );
    let mut cx = desktop(&net);
    let reads = Reads::default();
    let kept = reads.clone();
    let chain = net.clone();
    cx.host().handle::<Ask<files::Files>>(move |query| {
        if let files::Query::Read { at, .. } = &query {
            kept.borrow_mut().push(*at);
        }
        chain.query(files::PROGRAM, &query)
    });
    cx.simulate_click("files/place/shared");
    cx.run_until_parked();
    cx.simulate_click("files/entry/big.png");
    cx.run_until_parked();
    (cx, net, reads)
}

#[test]
fn a_picture_larger_than_one_read_is_read_whole_at_one_snapshot() {
    let size = 2 * files::CHUNK_SIZE as usize + 8;
    let (cx, _, reads) = picture_page(size);
    // the first 64 KiB, then a chunk at a time
    let reads = reads.borrow();
    assert_eq!(reads.len(), 3);
    assert!(reads[0].is_some() && reads.iter().all(|at| *at == reads[0]));
    let Some(wire::Node::Image {
        data: Some(wire::ImageData::Encoded(bytes)),
        ..
    }) = cx.find("files-picture")
    else {
        panic!("no picture: {:?}", cx.texts());
    };
    assert_eq!(bytes.len(), size);
}

#[test]
fn a_picture_on_screen_is_not_read_again_by_a_commit_that_leaves_it() {
    let (mut cx, net, reads) = picture_page(2 * files::CHUNK_SIZE as usize + 8);
    assert_eq!(reads.borrow().len(), 3);
    commit(
        &net,
        net.me(),
        "Another file",
        vec![put("/shared/other.txt", b"other")],
    );
    cx.run_until_parked();
    assert_eq!(reads.borrow().len(), 4, "its first bytes again, no more");
    assert!(
        matches!(cx.find("files-picture"), Some(wire::Node::Image { .. })),
        "{:?}",
        cx.texts()
    );
}

#[test]
fn a_picture_read_short_of_its_size_says_so() {
    let net = seeded();
    let mut bytes = PNG.to_vec();
    bytes.resize(2 * files::CHUNK_SIZE as usize, 0);
    commit(
        &net,
        net.me(),
        "A big picture",
        vec![put("/shared/big.png", &bytes)],
    );
    let mut cx = desktop(&net);
    let chain = net.clone();
    // every read past the first brings nothing
    cx.host().handle::<Ask<files::Files>>(move |query| {
        let past = matches!(query, files::Query::Read { offset, .. } if offset > 0);
        match chain.query(files::PROGRAM, &query) {
            Ok(files::Reply::Contents(contents)) if past => {
                Ok(files::Reply::Contents(files::Contents {
                    bytes: Vec::new(),
                    eof: true,
                    ..contents
                }))
            }
            reply => reply,
        }
    });
    cx.simulate_click("files/place/shared");
    cx.run_until_parked();
    cx.simulate_click("files/entry/big.png");
    cx.run_until_parked();
    assert!(cx.find("files-picture").is_none());
    assert!(cx.find("files-file-refused").is_some(), "{:?}", cx.texts());
}

#[test]
fn a_picture_too_large_to_draw_says_its_size_and_is_not_read_on() {
    let (cx, _, reads) = picture_page(SHOW_MOST as usize + 1);
    assert_eq!(
        reads.borrow().len(),
        1,
        "only the first bytes, which say it is a picture"
    );
    assert!(cx.find("files-picture").is_none());
    assert!(
        shows(&cx, &words::too_big_to_show(SHOW_MOST + 1)),
        "{:?}",
        cx.texts()
    );
}

/// What files holds at `path` now, read whole.
fn held(net: &Network, path: &str) -> Vec<u8> {
    let read = files::Query::Read {
        path: path.into(),
        at: None,
        offset: 0,
        len: u64::MAX,
    };
    match net.query(files::PROGRAM, &read) {
        Ok(files::Reply::Contents(contents)) => contents.bytes,
        other => panic!("{path}: {other:?}"),
    }
}

/// `size` bytes no two chunks of which are alike.
fn film(size: usize) -> Vec<u8> {
    (0..size).map(|at| (at % 251) as u8).collect()
}

/// What `files` answers `op` with on `net`, as the node predicts it.
fn predicted(net: &Network, op: &files::Op) -> Result<Submitted, Error> {
    Ok(match net.submit(net.me(), files::PROGRAM, op) {
        Ok(output) => Submitted::Applied {
            frame: [0; 32],
            until: 0,
            output,
        },
        Err(refusal) => Submitted::Refused {
            frame: [0; 32],
            until: 0,
            refusal,
        },
    })
}

/// Counts every `Stage` the view sends; `answer` answers each, by its
/// count from 1.
fn stages(
    cx: &TestAppContext,
    answer: impl Fn(u32, &files::Op) -> Result<Submitted, Error> + 'static,
) -> Rc<Cell<u32>> {
    let stages = Rc::new(Cell::new(0));
    let counted = stages.clone();
    cx.host().handle::<Submit<files::Files>>(move |op| {
        if matches!(op, files::Op::Stage { .. }) {
            counted.set(counted.get() + 1);
        }
        answer(counted.get(), &op)
    });
    stages
}

#[test]
fn an_uploaded_picture_is_committed_and_opens_as_itself() {
    let net = seeded();
    let mut cx = desktop(&net);
    open_home(&mut cx);
    device(&cx, &[("dot.png", &PNG)]);
    begin(&mut cx, "upload");
    assert!(cx.has_text("dot.png"), "{:?}", cx.texts());
    commit_form(&mut cx);
    assert!(
        shows(&cx, &words::uploads(&["dot.png"])),
        "{:?}",
        cx.texts()
    );
    assert_eq!(held(&net, "/home/dev/dot.png"), PNG);
    cx.simulate_click("files/entry/dot.png");
    cx.run_until_parked();
    assert!(cx.find("files-picture").is_some(), "{:?}", cx.texts());
}

#[test]
fn a_large_upload_is_read_and_staged_a_chunk_at_a_time_then_committed() {
    let net = seeded();
    let mut cx = desktop(&net);
    open_home(&mut cx);
    let big = film(2 * files::CHUNK_SIZE as usize + 3);
    device(&cx, &[("film.mov", &big), ("note.txt", b"hi")]);
    let reads = Rc::new(RefCell::new(Vec::new()));
    let asked = reads.clone();
    let held_on_device = [big.clone(), b"hi".to_vec()];
    cx.host().handle::<FileRead>(move |ask| {
        asked.borrow_mut().push(ask.len);
        let bytes = &held_on_device[ask.file as usize - 1];
        let start = ask.offset as usize;
        Ok(bytes[start..start + ask.len as usize].to_vec())
    });
    let chain = net.clone();
    let stages = stages(&cx, move |_, op| predicted(&chain, op));
    begin(&mut cx, "upload");
    commit_form(&mut cx);
    assert_eq!(stages.get(), 3, "the big file's three chunks");
    assert_eq!(
        *reads.borrow(),
        [files::CHUNK_SIZE, files::CHUNK_SIZE, 3, 2],
        "a chunk read before each is staged; the small file whole"
    );
    assert!(
        shows(&cx, &words::uploads(&["film.mov", "note.txt"])),
        "{:?}",
        cx.texts()
    );
    assert_eq!(held(&net, "/home/dev/film.mov"), big);
    assert_eq!(held(&net, "/home/dev/note.txt"), b"hi");
}

#[test]
fn a_refused_chunk_stops_the_upload_and_commit_again_goes_on_from_it() {
    let net = seeded();
    let mut cx = desktop(&net);
    open_home(&mut cx);
    let big = film(2 * files::CHUNK_SIZE as usize + 3);
    device(&cx, &[("film.mov", &big)]);
    let chain = net.clone();
    let stages = stages(&cx, move |count, op| match count {
        2 => Err(Error::new("unavailable", "the node is away")),
        _ => predicted(&chain, op),
    });
    begin(&mut cx, "upload");
    commit_form(&mut cx);
    assert!(shows(&cx, "the node is away"), "{:?}", cx.texts());
    assert_eq!(stages.get(), 2);
    commit_form(&mut cx);
    assert_eq!(stages.get(), 4, "the second chunk again, then the third");
    assert_eq!(held(&net, "/home/dev/film.mov"), big);
}

#[test]
fn a_chunk_the_network_dropped_is_staged_anew() {
    let net = seeded();
    let mut cx = desktop(&net);
    open_home(&mut cx);
    let big = film(2 * files::CHUNK_SIZE as usize + 3);
    device(&cx, &[("film.mov", &big)]);
    // the second chunk is predicted, then dropped: a network that never
    // ran it answers what it would hold it as
    let elsewhere = Network::new();
    elsewhere.seat::<files::Files>(files::PROGRAM);
    let chain = net.clone();
    let stages = stages(&cx, move |count, op| match count {
        2 => predicted(&elsewhere, op),
        _ => predicted(&chain, op),
    });
    begin(&mut cx, "upload");
    commit_form(&mut cx);
    assert!(
        cx.find("files/write").is_some(),
        "the commit was refused: {:?}",
        cx.texts()
    );
    commit_form(&mut cx);
    assert_eq!(
        stages.get(),
        5,
        "the dropped chunk and every one after it, staged anew"
    );
    assert_eq!(held(&net, "/home/dev/film.mov"), big);
}

#[test]
fn a_file_too_big_is_left_out_and_said_why() {
    let net = seeded();
    let mut cx = desktop(&net);
    open_home(&mut cx);
    device(&cx, &[("notes.txt", b"new")]);
    let picked = vec![
        Picked {
            name: "huge.iso".into(),
            size: 1 << 32,
            file: 9,
        },
        Picked {
            name: "notes.txt".into(),
            size: 3,
            file: 1,
        },
    ];
    cx.host().handle::<FilePick>(move |_| Ok(picked.clone()));
    begin(&mut cx, "upload");
    assert!(
        shows(&cx, &words::too_big_to_upload(1 << 32)),
        "{:?}",
        cx.texts()
    );
    assert!(shows(&cx, words::REPLACES), "notes.txt is there already");
    commit_form(&mut cx);
    assert_eq!(held(&net, "/home/dev/notes.txt"), b"new");
}

#[test]
fn a_pick_with_nothing_to_send_offers_no_commit() {
    let net = seeded();
    let mut cx = desktop(&net);
    open_home(&mut cx);
    device(&cx, &[("docs", b"not a folder")]);
    begin(&mut cx, "upload");
    assert!(
        shows(
            &cx,
            &words::held_by(&Node::Dir {
                tree: BlobId::Sha256([0; 32]),
                entries: 1,
            })
        ),
        "{:?}",
        cx.texts()
    );
    assert!(cx.has_text(words::NOTHING_TO_UPLOAD));
    assert_eq!(
        cx.interactivity("files/write/submit").aria.disabled,
        Some(true)
    );
}

#[test]
fn a_cancel_opens_nothing_and_a_refused_pick_says_why() {
    let net = seeded();
    let mut cx = desktop(&net);
    open_home(&mut cx);
    device(&cx, &[]);
    begin(&mut cx, "upload");
    assert!(cx.find("files/write").is_none(), "cancelled: no form");
    cx.host()
        .refuse::<FilePick>("host_fault", "File picker not yet implemented for iOS");
    begin(&mut cx, "upload");
    assert!(
        shows(&cx, "File picker not yet implemented for iOS"),
        "{:?}",
        cx.texts()
    );
}
