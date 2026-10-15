//! The transaction lists come from the index, not from the blocks: the
//! latest transactions however far below the head they sit, each row's
//! block read when the row is drawn, its payload when it is opened.
use super::*;

/// Blocks 0..=5,012, a transaction last at 12: five thousand empty blocks
/// between the head and the latest transaction.
const QUIET_TIP: u64 = 5_012;

fn quiet() -> (TestAppContext, Rc<RefCell<u64>>) {
    let mut cx = TestAppContext::new();
    let tip = Rc::new(RefCell::new(QUIET_TIP));
    node(&mut cx, tip.clone());
    cx.open::<Explorer>();
    cx.run_until_parked();
    (cx, tip)
}

#[test]
fn the_latest_transactions_are_listed_however_far_below_the_head_they_sit() {
    let (mut cx, _) = quiet();
    // the Overview: the head's page of blocks, folded, and the three
    // transactions five thousand blocks below it
    assert!(
        cx.has_text("4,913–5,012 · 100 empty blocks"),
        "{:?}",
        cx.texts()
    );
    for text in ["Post in #design", "Direct message", "mystery · 4 bytes"] {
        assert!(cx.has_text(text), "{text}: {:?}", cx.texts());
    }
    assert_eq!(
        cx.host().requests::<ChainBlocks>(),
        [BlockPage {
            before: None,
            limit: u32::MAX
        }],
        "one page of blocks at the head, and none below it"
    );
    assert_eq!(
        cx.host().requests::<ChainBlock>(),
        [BlockRef::Height(12), BlockRef::Height(11)],
        "the blocks the rows drawn sit in, each once"
    );
    assert!(cx.host().requests::<ChainTx>().is_empty());
    cx.simulate_click("explorer-tab-transactions");
    cx.run_until_parked();
    assert!(
        cx.has_text("3 transactions since block 11"),
        "{:?}",
        cx.texts()
    );
    for id in ["explorer-tx-12-0", "explorer-tx-11-1", "explorer-tx-11-0"] {
        assert!(cx.find(id).is_some(), "{id}: {:?}", cx.texts());
    }
    assert!(cx.has_text("Post in #design") && cx.has_text("02020202…0202"));
    assert_eq!(
        cx.host().requests::<ChainBlocks>().len(),
        1,
        "the Transactions tab reads no blocks"
    );
    assert_eq!(cx.host().requests::<ChainBlock>().len(), 2);
    // the row opens from the block read for it
    cx.simulate_click("explorer-tx-11-0");
    cx.run_until_parked();
    assert!(cx.has_text("In block 11") && cx.has_text("hello there"));
    assert!(cx.host().requests::<ChainTx>().is_empty());
}

#[test]
fn a_programs_transactions_are_the_indexs_too() {
    let (mut cx, _) = quiet();
    cx.simulate_input("explorer-search", "chat");
    cx.simulate_submit("explorer-search");
    cx.run_until_parked();
    assert!(cx.has_text("Transactions · chat") && cx.has_text("2 transactions since block 11"));
    assert!(cx.has_text("Post in #design") && !cx.has_text("mystery · 4 bytes"));
    let asked = cx.host().requests::<Query<Activity>>();
    assert!(
        asked.iter().any(|query| matches!(
            query,
            activity::Query::Recent { program: Some(program), .. } if program == "chat"
        )),
        "{asked:?}"
    );
    assert_eq!(cx.host().requests::<ChainBlocks>().len(), 1);
}

/// A frame too long for the node to list with its block (a 1 MB push) is
/// drawn by its size from the index and the block, and its payload is read
/// only when its row is opened.
#[test]
fn an_accounts_long_payload_is_read_only_when_its_row_is_opened() {
    let mut cx = TestAppContext::new();
    let tip = Rc::new(RefCell::new(QUIET_TIP));
    node(&mut cx, tip);
    let pushed = || {
        let mut blocks = chain(QUIET_TIP);
        let push = forge::Op::Push {
            repo: "app".into(),
            request: vec![0x50; 1 << 20],
            pack: None,
        };
        blocks[12].txs = vec![tx(0xfe, ADA, forge::PROGRAM, borsh::to_vec(&push).unwrap())];
        blocks
    };
    index(&cx, pushed);
    serve(&cx, pushed);
    cx.open::<Explorer>();
    cx.run_until_parked();
    cx.simulate_input("explorer-search", "ada");
    cx.simulate_submit("explorer-search");
    cx.run_until_parked();
    assert!(cx.has_text("3 transactions recorded"), "{:?}", cx.texts());
    assert!(
        cx.has_text("forge · 1,048,589 bytes"),
        "listed by its size: {:?}",
        cx.texts()
    );
    assert_eq!(
        cx.host().requests::<ChainBlock>(),
        [BlockRef::Height(12), BlockRef::Height(11)]
    );
    assert!(
        cx.host().requests::<ChainTx>().is_empty(),
        "no payload is read for a list"
    );
    cx.simulate_click("explorer-tx-12-0");
    cx.run_until_parked();
    assert!(cx.has_text("Push · app"), "{:?}", cx.texts());
    assert_eq!(
        cx.host().requests::<ChainTx>(),
        [TxAsk {
            tx: TxRef::At {
                height: 12,
                position: 0
            },
            most: u32::MAX
        }],
        "the opened row alone reads its payload"
    );
}

/// The Blocks tab reads older pages as its last line is drawn, down to
/// `WINDOW` blocks, and no further: a quiet chain folds into one line, so
/// the last line is always drawn.
#[test]
fn the_blocks_tab_reads_down_to_the_window_as_it_is_drawn_and_no_further() {
    let (mut cx, _) = quiet();
    cx.simulate_click("explorer-tab-blocks");
    cx.run_until_parked();
    let asked: Vec<Option<u64>> = cx
        .host()
        .requests::<ChainBlocks>()
        .iter()
        .map(|ask| ask.before)
        .collect();
    let mut expected = vec![None];
    expected.extend(
        (1..WINDOW as u64 / u64::from(NODE_PAGE))
            .map(|page| Some(QUIET_TIP + 1 - page * u64::from(NODE_PAGE))),
    );
    assert_eq!(asked, expected, "{asked:?}");
    assert!(
        cx.has_text("4,013–5,012 · 1,000 empty blocks"),
        "{:?}",
        cx.texts()
    );
    assert!(cx.has_text("the last 1,000 blocks"), "{:?}", cx.texts());
    // the blocks read stay held: the tab opens again without a read
    cx.simulate_click("explorer-tab-overview");
    cx.run_until_parked();
    cx.simulate_click("explorer-tab-blocks");
    cx.run_until_parked();
    assert_eq!(cx.host().requests::<ChainBlocks>().len(), expected.len());
}
