use super::*;

fn far_past_the_window() -> TestAppContext {
    let mut cx = TestAppContext::new();
    node(&mut cx, Rc::new(RefCell::new(WINDOW as u64 + 100)));
    cx.open::<Explorer>();
    cx.run_until_parked();
    cx.simulate_click("explorer-tab-transactions");
    cx.run_until_parked();
    cx
}

#[test]
fn the_transactions_page_lists_every_frame_activity_recorded_not_the_window() {
    let cx = far_past_the_window();
    assert!(cx.has_text("3 transactions"), "{:?}", cx.texts());
    assert_eq!(
        name(&cx, "explorer-tx-12-0").as_deref(),
        Some("mystery, Rejected, block 12 · 0")
    );
    assert_eq!(
        name(&cx, "explorer-tx-11-1").as_deref(),
        Some("chat, Accepted, block 11 · 1")
    );
    assert!(cx.find("explorer-tx-11-0").is_some());
    assert_eq!(
        frames_asked(&cx),
        [None],
        "the first page, which holds them all"
    );
    assert!(
        cx.host().requests::<ChainTx>().is_empty(),
        "no transaction is read to list it"
    );
    assert!(cx.host().requests::<ChainBlock>().is_empty());
}

#[test]
fn a_frame_past_the_window_opens_by_its_place_and_reads_its_payload_alone() {
    let mut cx = far_past_the_window();
    cx.simulate_click("explorer-tx-11-0");
    cx.run_until_parked();
    assert!(
        cx.has_text("In block 11") && cx.has_text("hello there"),
        "{:?}",
        cx.texts()
    );
    assert_eq!(
        cx.host().requests::<ChainTx>(),
        [TxAsk {
            tx: TxRef::At {
                height: 11,
                position: 0
            },
            most: u32::MAX
        }]
    );
    assert!(cx.host().requests::<ChainBlock>().is_empty());
}

#[test]
fn a_frame_in_the_window_opens_from_the_window_and_reads_only_its_payload() {
    let (mut cx, _) = ready();
    cx.simulate_click("explorer-tab-transactions");
    cx.run_until_parked();
    cx.simulate_click("explorer-tx-11-1");
    cx.run_until_parked();
    assert!(cx.has_text("Direct message"), "{:?}", cx.texts());
    assert_eq!(
        cx.host().requests::<ChainTx>(),
        [TxAsk {
            tx: TxRef::At {
                height: 11,
                position: 1
            },
            most: u32::MAX
        }]
    );
    assert!(cx.host().requests::<ChainBlock>().is_empty());
}

#[test]
fn a_write_to_activity_reads_its_first_page_again() {
    let mut cx = TestAppContext::new();
    node(&mut cx, Rc::new(RefCell::new(12)));
    let recorded = Rc::new(RefCell::new(chain(12)));
    let held = recorded.clone();
    index(&cx, move || held.borrow().clone());
    let writes = cx.host().stream::<Changes<Activity>>();
    cx.open::<Explorer>();
    cx.run_until_parked();
    cx.simulate_click("explorer-tab-transactions");
    cx.run_until_parked();
    assert!(cx.has_text("3 transactions"), "{:?}", cx.texts());
    assert_eq!(frames_asked(&cx), [None]);
    let pages = cx.host().requests::<ChainBlocks>().len();
    let later = tx(0xd7, ADA, "chat", post("design", "later"));
    recorded.borrow_mut()[12].txs.push(later);
    writes.send(Some(view::methods::Change {
        height: 12,
        keys: Vec::new(),
    }));
    cx.run_until_parked();
    assert!(cx.has_text("4 transactions"), "{:?}", cx.texts());
    assert_eq!(frames_asked(&cx), [None, None]);
    assert_eq!(
        name(&cx, "explorer-tx-12-1").as_deref(),
        Some("chat, Accepted, block 12 · 1")
    );
    assert_eq!(
        cx.host().requests::<ChainBlocks>().len(),
        pages,
        "the list follows activity, not the window"
    );
}

#[test]
fn a_place_that_holds_no_transaction_says_so() {
    let mut cx = TestAppContext::new();
    node(&mut cx, Rc::new(RefCell::new(12)));
    let routes = cx.host().stream::<HostRoute>();
    cx.open::<Explorer>();
    cx.run_until_parked();
    routes.send("tx/12/5".into());
    cx.run_until_parked();
    assert!(
        cx.has_text("Transaction not found")
            && cx.has_text("This node keeps no transaction at position 5 of block 12."),
        "{:?}",
        cx.texts()
    );
}

#[test]
fn without_the_activity_program_the_transactions_say_why() {
    let mut cx = TestAppContext::new();
    node(&mut cx, Rc::new(RefCell::new(12)));
    cx.host()
        .refuse::<Query<Activity>>("not_found", "no program is called activity");
    cx.open::<Explorer>();
    cx.run_until_parked();
    assert!(
        cx.has_text("no program is called activity") && cx.has_text("Latest activity"),
        "the overview's blocks stay: {:?}",
        cx.texts()
    );
    cx.simulate_click("explorer-tab-transactions");
    cx.run_until_parked();
    assert!(
        cx.has_text("no program is called activity"),
        "{:?}",
        cx.texts()
    );
}

/// Where each page of every transaction asked starts, by the offset its
/// cursor carries (`None`, the first page); each asks a page.
fn frames_asked(cx: &TestAppContext) -> Vec<Option<u64>> {
    let asked = cx.host().requests::<Query<Activity>>().into_iter();
    asked
        .filter_map(|query| match query {
            activity::Query::Frames { page } => {
                assert_eq!(page.limit, Some(crate::queries::PAGE), "a page");
                Some(page.after.map(offset))
            }
            _ => None,
        })
        .collect()
}

fn offset(cursor: Vec<u8>) -> u64 {
    u64::from_be_bytes(cursor.try_into().expect("an offset"))
}

/// An activity of `frames` transactions, one a block from block 1, each
/// Ada's, sent where `to` says by its height; the window is the short
/// chain's.
fn long_activity(frames: u64, to: fn(u64) -> &'static str) -> TestAppContext {
    let mut cx = TestAppContext::new();
    node(&mut cx, Rc::new(RefCell::new(12)));
    let recorded: Vec<Block> = (1..=frames)
        .map(|height| Block {
            height,
            id: [0; 32],
            parent: [0; 32],
            time: T0 + height * 1000,
            epoch: 0,
            proposer: None,
            txs: vec![tx(height as u8, ADA, to(height), Vec::new())],
        })
        .collect();
    index(&cx, move || recorded.clone());
    cx
}

/// The row of the transaction in block `height`.
fn frame(height: u64) -> String {
    format!("explorer-tx-{height}-0")
}

#[test]
fn opening_the_explorer_reads_the_first_page_of_activity_not_all_of_it() {
    let mut cx = long_activity(5_000, |_| "chat");
    cx.open::<Explorer>();
    cx.run_until_parked();
    assert!(cx.find(&frame(5_000)).is_some(), "{:?}", cx.texts());
    assert_eq!(frames_asked(&cx), [None]);
    cx.simulate_click("explorer-tab-transactions");
    cx.run_until_parked();
    assert_eq!(frames_asked(&cx), [None], "the tab lists the page read");
    assert!(cx.find(&frame(5_000)).is_some(), "{:?}", cx.texts());
    assert!(
        !cx.has_text("5,000 transactions") && !cx.has_text("50 transactions"),
        "no count of a list read in part: {:?}",
        cx.texts()
    );
}

/// The list reaching the rows held reads the next page; a write reads
/// the first page again only while that is all the listing holds, and a
/// listing read on starts over at its first page when the reader comes
/// back to it.
#[test]
fn scrolling_the_transactions_reads_the_next_page() {
    let mut cx = long_activity(5_000, |_| "chat");
    let writes = cx.host().stream::<Changes<Activity>>();
    let write = |cx: &mut TestAppContext| {
        writes.send(Some(view::methods::Change {
            height: 5_000,
            keys: Vec::new(),
        }));
        cx.run_until_parked();
    };
    cx.open::<Explorer>();
    cx.run_until_parked();
    cx.simulate_click("explorer-tab-transactions");
    cx.run_until_parked();
    write(&mut cx);
    assert_eq!(frames_asked(&cx), [None, None], "the first page again");
    assert!(cx.find("explorer-transactions-list-more").is_none());
    cx.simulate_range("explorer-transactions-list", 40..51);
    cx.run_until_parked();
    assert_eq!(frames_asked(&cx), [None, None, Some(50)], "the next page");
    assert!(cx.find(&frame(4_950)).is_some(), "{:?}", cx.texts());
    let read = frames_asked(&cx).len();
    write(&mut cx);
    assert_eq!(
        frames_asked(&cx).len(),
        read,
        "a listing read on is left as read"
    );
    cx.simulate_click("explorer-tab-overview");
    cx.run_until_parked();
    assert_eq!(
        frames_asked(&cx)[read..],
        [None],
        "it starts over at its first page"
    );
    write(&mut cx);
    assert_eq!(frames_asked(&cx)[read..], [None, None]);
}

/// Activity lists no transaction by program: a program's are the rows read
/// that went to it, and pages are read on until they fill the list or
/// activity ends. Then the count is a total.
#[test]
fn a_programs_transactions_read_on_to_where_activity_ends() {
    let mut cx = long_activity(5_000, |height| match height % 1_000 {
        0 => "forge",
        _ => "chat",
    });
    let routes = cx.host().stream::<HostRoute>();
    cx.open::<Explorer>();
    cx.run_until_parked();
    routes.send("txs/forge".into());
    cx.run_until_parked();
    assert_eq!(frames_asked(&cx).len(), 100, "every page, one at a time");
    for height in [5_000, 4_000, 3_000, 2_000, 1_000] {
        assert!(cx.find(&frame(height)).is_some(), "{:?}", cx.texts());
    }
    assert!(cx.has_text("5 transactions"), "{:?}", cx.texts());
    assert!(!cx.has_text("Reading older transactions…"));
}

/// While pages remain, a program's list says how far back it has looked
/// and that it reads on, never that nothing was recorded.
#[test]
fn a_programs_transactions_say_how_far_back_they_looked() {
    let mut cx = long_activity(5_000, |height| match height {
        10 => "forge",
        _ => "chat",
    });
    let routes = cx.host().stream::<HostRoute>();
    cx.open::<Explorer>();
    cx.run_until_parked();
    // the node stops answering after the first page
    cx.host().never::<Query<Activity>>();
    routes.send("txs/forge".into());
    cx.run_until_parked();
    assert!(
        cx.has_text("0 of the newest 50 transactions")
            && cx.has_text("Reading older transactions…"),
        "{:?}",
        cx.texts()
    );
    assert!(!cx.has_text("No transactions recorded."));
}

/// A program whose rows fill the list stops reading on: the pages held
/// are as far back as the list reaches.
#[test]
fn a_programs_transactions_stop_reading_once_they_fill_the_list() {
    let mut cx = long_activity(5_000, |height| match height % 2 {
        0 => "forge",
        _ => "chat",
    });
    let routes = cx.host().stream::<HostRoute>();
    cx.open::<Explorer>();
    cx.run_until_parked();
    routes.send("txs/forge".into());
    cx.run_until_parked();
    let pages = frames_asked(&cx).len();
    assert!((2..=3).contains(&pages), "{pages} pages");
    let read = pages as u64 * crate::queries::PAGE;
    let caption = format!("{} of the newest {read} transactions", read / 2);
    assert!(cx.has_text(&caption), "{caption}: {:?}", cx.texts());
}

/// An account's page asks activity for its newest page alone; Show older
/// reads the one past it.
#[test]
fn an_account_page_asks_for_its_newest_page() {
    let mut cx = long_activity(5_000, |_| "chat");
    let routes = cx.host().stream::<HostRoute>();
    cx.open::<Explorer>();
    cx.run_until_parked();
    routes.send("account/3".into());
    cx.run_until_parked();
    let acted = |cx: &TestAppContext| {
        cx.host()
            .requests::<Query<Activity>>()
            .into_iter()
            .filter_map(|query| match query {
                activity::Query::Acted { account, page } => {
                    assert_eq!((account, page.limit), (3, Some(crate::queries::PAGE)));
                    Some(page.after.map(offset))
                }
                _ => None,
            })
            .collect::<Vec<_>>()
    };
    assert_eq!(acted(&cx), [None]);
    assert!(
        cx.has_text("5,000 transactions recorded"),
        "{:?}",
        cx.texts()
    );
    assert!(cx.find(&frame(4_951)).is_some() && cx.find(&frame(4_950)).is_none());
    cx.simulate_click("explorer-activity-older");
    cx.run_until_parked();
    assert_eq!(acted(&cx), [None, Some(50)]);
    assert!(cx.find(&frame(4_901)).is_some(), "{:?}", cx.texts());
}
