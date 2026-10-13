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
        hooks: Vec::new(),
    };
    Modules::init(
        &store.exec(registry_env(0, Origin::Root)),
        &abi::encode(&genesis),
    )
    .unwrap();
    let kind = registry::CODE_KIND;
    let code = guest::blob_id(guest::HashKind::Sha256, kind, b"scheduled code").unwrap();
    let blob = guest::Blob {
        kind: kind.into(),
        body: b"scheduled code".to_vec(),
    };
    store.borrow_mut().blobs.insert(code, blob);
    let publish = registry::Op::Publish { code };
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

/// A change lands at its height with no op on the registry after it: no
/// registry head comes, the chain's head reaching it reads the roster
/// again, and the registry, which keeps the change in its schedule until
/// an op folds it, answers it as running.
#[test]
fn a_change_reached_by_the_head_is_running_not_scheduled() {
    let mut cx = TestAppContext::new();
    let heads = cx.host().stream::<ChainHeads>();
    let tip = Rc::new(RefCell::new(12u64));
    node(&mut cx, tip.clone());
    let chess = registry::Pending {
        height: 13,
        change: registry::Change::Set(entry("chess", 0x11)),
        by: None,
    };
    let at = tip.clone();
    cx.host().handle::<Query<Modules>>(move |query| {
        let height = *at.borrow();
        Ok(match query {
            registry::Query::Roster => {
                let mut programs = vec![entry("chat", 0xab), entry("identity", 0xcd)];
                if height >= chess.height {
                    programs.push(entry("chess", 0x11));
                }
                registry::Reply::Roster {
                    height,
                    programs,
                    views: Vec::new(),
                }
            }
            registry::Query::Scheduled { .. } => {
                registry::Reply::Scheduled(registry::PageResponse {
                    height,
                    items: vec![chess.clone()],
                    next: None,
                })
            }
            registry::Query::Hooks(_) => registry::Reply::Hooks(Vec::new()),
            registry::Query::Grants { .. } => registry::Reply::Grants(registry::PageResponse {
                height,
                items: Vec::new(),
                next: None,
            }),
            registry::Query::Roles => registry::Reply::Roles(abi::Roles {
                registry: registry::MODULE.into(),
                validators: valset::MODULE.into(),
                identity: identity::MODULE.into(),
            }),
            other => panic!("unexpected query: {other:?}"),
        })
    });
    cx.open::<Explorer>();
    cx.run_until_parked();
    cx.simulate_click("explorer-tab-programs");
    cx.run_until_parked();
    assert!(cx.has_text("2 programs") && cx.has_text("at 13"));
    *tip.borrow_mut() = 13;
    heads.send(Head {
        height: 13,
        time: T0 + 13_000,
        id: [113; 32],
    });
    cx.run_until_parked();
    assert!(
        cx.has_text("3 programs") && cx.has_text("chess"),
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

#[test]
fn an_accounts_activity_past_the_window_is_read_by_place_and_follows_the_index() {
    let mut cx = TestAppContext::new();
    let tip = Rc::new(RefCell::new(WINDOW as u64 + 100));
    node(&mut cx, tip.clone());
    let heads = cx.host().stream::<Changes<Activity>>();
    cx.open::<Explorer>();
    cx.run_until_parked();
    cx.simulate_click("explorer-tab-accounts");
    cx.run_until_parked();
    assert!(cx.has_text("3 · tx recorded"), "{:?}", cx.texts());
    assert!(cx.has_text("2 tx"), "Ada's two, long out of the window");
    cx.simulate_click("explorer-account-3");
    cx.run_until_parked();
    for text in [
        "2 transactions recorded",
        "Post in #design",
        "Direct message",
    ] {
        assert!(cx.has_text(text), "{text}: {:?}", cx.texts());
    }
    let at = |position| TxAsk {
        tx: TxRef::At {
            height: 11,
            position,
        },
        most: 4 << 10,
    };
    assert_eq!(cx.host().requests::<ChainTx>(), [at(1), at(0)]);
    assert!(cx.host().requests::<ChainBlock>().is_empty());
    let asked = cx.host().requests::<Query<Activity>>().len();
    heads.send(Some(ducktape_view_guest::methods::Change {
        height: *tip.borrow() + 1,
        keys: Vec::new(),
    }));
    cx.run_until_parked();
    assert_eq!(
        cx.host().requests::<Query<Activity>>().len(),
        asked + 2,
        "the counts and the open account are read again"
    );
    assert_eq!(
        cx.host().requests::<ChainTx>().len(),
        2,
        "a row already read is not read again"
    );
    cx.simulate_click("explorer-tx-11-0");
    cx.run_until_parked();
    assert!(
        cx.has_text("In block 11") && cx.has_text("hello there"),
        "a row of the account's activity opens though the window has no such hash: {:?}",
        cx.texts()
    );
}

#[test]
fn without_the_activity_program_the_accounts_list_and_the_account_says_why() {
    let mut cx = TestAppContext::new();
    node(&mut cx, Rc::new(RefCell::new(12)));
    cx.host()
        .refuse::<Query<Activity>>("not_found", "no program is called activity");
    cx.open::<Explorer>();
    cx.run_until_parked();
    cx.simulate_click("explorer-tab-accounts");
    cx.run_until_parked();
    assert!(cx.has_text("Ada") && cx.has_text("–"), "{:?}", cx.texts());
    cx.simulate_click("explorer-account-3");
    cx.run_until_parked();
    assert!(
        cx.has_text("no program is called activity") && cx.has_text("laptop"),
        "{:?}",
        cx.texts()
    );
    assert!(!cx.has_text("Programs used"));
}
