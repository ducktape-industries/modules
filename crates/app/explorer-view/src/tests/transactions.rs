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
    let every = activity::Query::Frames {
        page: activity::PageRequest {
            after: None,
            limit: None,
        },
    };
    assert!(
        cx.host().requests::<Query<Activity>>().contains(&every),
        "{:?}",
        cx.host().requests::<Query<Activity>>()
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
fn a_write_to_activity_reads_every_frame_again() {
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
    let pages = cx.host().requests::<ChainBlocks>().len();
    let later = tx(0xd7, ADA, "chat", post("design", "later"));
    recorded.borrow_mut()[12].txs.push(later);
    writes.send(Some(view::methods::Change {
        height: 12,
        keys: Vec::new(),
    }));
    cx.run_until_parked();
    assert!(cx.has_text("4 transactions"), "{:?}", cx.texts());
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
