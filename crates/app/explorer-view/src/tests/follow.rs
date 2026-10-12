use super::*;

/// A module's account comes from the kernel's `RegisterModule`, which no
/// transaction targets: identity's own live heads, not the window's
/// transactions, re-read the accounts.
#[test]
fn an_identity_head_re_reads_the_accounts() {
    let mut cx = TestAppContext::new();
    node(&mut cx, Rc::new(RefCell::new(12)));
    let heads = cx.host().stream::<Changes<Identity>>();
    cx.open::<Explorer>();
    cx.run_until_parked();
    cx.simulate_click("explorer-tab-accounts");
    cx.run_until_parked();
    assert!(!cx.has_text("Module · chess"));
    let asked = cx.host().requests::<Query<Identity>>().len();
    let chess = account(
        8,
        "chess",
        identity::Control::Module {
            module: "chess".into(),
        },
    );
    cx.host().handle::<Query<Identity>>(move |_| {
        Ok(identity::Reply::Accounts(identity::PageResponse {
            height: 13,
            items: vec![ada(), scout(), forge(), chess.clone()],
            next: None,
        }))
    });
    heads.send(Some(ducktape_view_guest::methods::Change {
        height: 13,
        keys: Vec::new(),
    }));
    cx.run_until_parked();
    assert!(cx.has_text("Module · chess"), "{:?}", cx.texts());
    assert!(cx.has_text("Agent · managed by Ada · suspended"), "kept");
    assert_eq!(cx.host().requests::<Query<Identity>>().len(), asked + 1);
}

#[test]
fn a_registry_head_re_reads_the_programs() {
    let mut cx = TestAppContext::new();
    node(&mut cx, Rc::new(RefCell::new(12)));
    let heads = cx.host().stream::<Changes<Modules>>();
    cx.open::<Explorer>();
    cx.run_until_parked();
    cx.simulate_click("explorer-tab-programs");
    cx.run_until_parked();
    assert!(cx.has_text("2 programs"));
    let programs = vec![
        entry("chat", 0xab),
        entry("identity", 0xcd),
        entry("chess", 0x11),
    ];
    registry_with(&cx, programs, Vec::new(), Vec::new());
    heads.send(Some(ducktape_view_guest::methods::Change {
        height: 13,
        keys: Vec::new(),
    }));
    cx.run_until_parked();
    assert!(
        cx.has_text("3 programs") && cx.has_text("chess"),
        "{:?}",
        cx.texts()
    );
    assert!(cx.has_text("Nothing is scheduled against the registry."));
}

fn registry_env(height: u64, origin: guest::Origin) -> guest::Env {
    guest::Env {
        height,
        origin,
        sender: None,
        ..guest::MockHost::env(registry::MODULE)
    }
}

fn registry_swapping(program: &str, at: u64) -> (guest::MockHost, BlobId, BlobId) {
    use guest::{MockHost, Module as _, Origin};
    let store = MockHost::default();
    let entry = |code| registry::Entry {
        program: program.into(),
        code,
        params: Vec::new(),
    };
    let running = BlobId::Sha256([0xab; 32]);
    let genesis = registry::Genesis {
        programs: vec![entry(running)],
        views: Vec::new(),
    };
    Modules::init(
        &store.exec(registry_env(0, Origin::Root)),
        &abi::encode(&genesis),
    )
    .unwrap();
    let publish = registry::Op::Publish {
        body: b"scheduled code".to_vec(),
    };
    Modules::execute(
        &store.exec(registry_env(1, Origin::Signed(ADA.to_vec()))),
        publish,
    )
    .unwrap();
    let scheduled: BlobId = abi::decode(&store.take_output()).unwrap();
    let swap = registry::Op::Schedule(registry::Scheduled {
        height: at,
        change: registry::Change::Set(entry(scheduled)),
    });
    let validators = Origin::Module(MockHost::roles().validators);
    Modules::execute(&store.exec(registry_env(1, validators)), swap).unwrap();
    (store, running, scheduled)
}

#[test]
fn a_scheduled_swap_shows_once_a_head_reaches_its_height() {
    use guest::Module as _;
    let mut cx = TestAppContext::new();
    let tip = Rc::new(RefCell::new(12));
    node(&mut cx, tip.clone());
    let heads = cx.host().stream::<ChainHeads>();
    let (store, running, scheduled) = registry_swapping("chat", 14);
    let asked_at = tip.clone();
    cx.host().handle::<Query<Modules>>(move |query| {
        let env = registry_env(*asked_at.borrow() + 1, guest::Origin::Root);
        Ok(Modules::query(&store.query(env), query).unwrap())
    });
    cx.open::<Explorer>();
    cx.run_until_parked();
    cx.simulate_click("explorer-tab-programs");
    cx.run_until_parked();
    let running = crate::decode::short(running.digest());
    let scheduled = crate::decode::short(scheduled.digest());
    assert!(
        cx.has_text(&running) && cx.has_text("at 14"),
        "{:?}",
        cx.texts()
    );
    let mut reach = |height: u64| {
        *tip.borrow_mut() = height;
        heads.send(Head {
            height,
            time: T0 + height * 1000,
            id: [(height as u8).wrapping_add(100); 32],
        });
        cx.run_until_parked();
        cx.has_text(&running) && cx.has_text("at 14")
    };
    assert!(reach(13), "before its height");
    assert!(!reach(14), "at its height");
    assert!(
        cx.has_text(&scheduled) && !cx.has_text(&running),
        "{:?}",
        cx.texts()
    );
    assert!(cx.has_text("Nothing is scheduled against the registry."));
}

#[test]
fn an_empty_registry_says_so() {
    let mut cx = TestAppContext::new();
    node(&mut cx, Rc::new(RefCell::new(12)));
    registry_with(&cx, Vec::new(), Vec::new(), Vec::new());
    cx.open::<Explorer>();
    cx.run_until_parked();
    cx.simulate_click("explorer-tab-programs");
    cx.run_until_parked();
    assert!(cx.has_text("No programs"), "{:?}", cx.texts());
}

#[test]
fn refused_accounts_say_why_and_retry_reads_again() {
    let mut cx = TestAppContext::new();
    node(&mut cx, Rc::new(RefCell::new(12)));
    cx.host()
        .refuse::<Query<Identity>>("unavailable", "identity is not running here");
    cx.open::<Explorer>();
    cx.run_until_parked();
    cx.simulate_click("explorer-tab-accounts");
    cx.run_until_parked();
    assert!(cx.has_text("identity is not running here"));
    node(&mut cx, Rc::new(RefCell::new(12)));
    cx.simulate_click("explorer-retry");
    cx.run_until_parked();
    assert!(cx.has_text("Module · forge"), "{:?}", cx.texts());
}

/// A pushed head asks the node for the blocks since the top, so one new
/// block costs one block, not a page of 20 (`chain.blocks { before: None,
/// limit: 20 }` per head, 2.3-2.6 KB carried for 1 new block on this
/// fixture chain, before).
#[test]
fn a_pushed_head_asks_for_the_blocks_since_the_top() {
    let mut cx = TestAppContext::new();
    let heads = cx.host().stream::<ChainHeads>();
    let tip = Rc::new(RefCell::new(12u64));
    node(&mut cx, tip.clone());
    let bytes = Rc::new(RefCell::new(0usize));
    {
        let (tip, bytes) = (tip.clone(), bytes.clone());
        cx.host().handle::<ChainBlocks>(move |ask| {
            let page = page(&chain(*tip.borrow()), &ask);
            *bytes.borrow_mut() += borsh::to_vec(&page).unwrap().len();
            Ok(page)
        });
    }
    cx.open::<Explorer>();
    cx.run_until_parked();
    cx.simulate_click("explorer-tab-transactions");
    cx.run_until_parked();
    for head in 13..=15u64 {
        let asked = cx.host().requests::<ChainBlocks>().len();
        let bytes_before = *bytes.borrow();
        *tip.borrow_mut() = head;
        heads.send(Head {
            height: head,
            time: T0 + head * 1000,
            id: [(head as u8).wrapping_add(100); 32],
        });
        cx.run_until_parked();
        let new: Vec<_> = cx.host().requests::<ChainBlocks>()[asked..]
            .iter()
            .map(|ask| (ask.before, ask.limit))
            .collect();
        eprintln!(
            "AUDIT head {head}: {} chain.blocks asks {new:?}, {} block bytes carried for 1 new block",
            new.len(),
            *bytes.borrow() - bytes_before
        );
        assert_eq!(new, [(None, 1)]);
    }
    // a head three blocks on asks for the three
    *tip.borrow_mut() = 18;
    let asked = cx.host().requests::<ChainBlocks>().len();
    heads.send(Head {
        height: 18,
        time: T0 + 18_000,
        id: [118; 32],
    });
    cx.run_until_parked();
    let new: Vec<_> = cx.host().requests::<ChainBlocks>()[asked..]
        .iter()
        .map(|ask| (ask.before, ask.limit))
        .collect();
    assert_eq!(new, [(None, 3)]);
    assert!(
        cx.has_text("3 transactions in the last 19 blocks"),
        "{:?}",
        cx.texts()
    );
}
