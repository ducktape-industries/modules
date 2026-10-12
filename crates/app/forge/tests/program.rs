mod common;
use common::*;

#[test]
fn founding_requires_bounds_and_ops_require_a_signer() {
    let sandbox = MemorySandbox::default();
    let unfounded = Forge::init(&sandbox.exec(1), b"").unwrap_err();
    assert_eq!(unfounded.code, code::INVALID_INPUT);
    sandbox.chain.init("forge", &bounds()).unwrap();

    let create = Op::Create {
        repo: "r".into(),
        hash: HashKind::Sha1,
    };
    for origin in [Origin::Root, Origin::Module("chat".into())] {
        let refusal = sandbox
            .forge
            .refused(|| sandbox.frame(origin, 1, TIME, &create));
        assert_eq!(refusal.code, code::UNAUTHORIZED);
    }
}

/// Git's two queries are `answer`'s, raw; `query` has no `Reply` for them.
#[test]
fn a_git_wire_query_answers_raw_bytes_and_is_no_reply() {
    let sandbox = MemorySandbox::default();
    sandbox.chain.init("forge", &bounds()).unwrap();
    let advertise = Query::Advertise {
        repo: "r".into(),
        service: Service::UploadPack,
    };
    let refusal = Forge::query(&sandbox.reads(1), advertise.clone()).unwrap_err();
    assert_eq!(refusal.code, code::INVALID_INPUT);
    // `answer` serves it: refused for the repo, not for the shape
    let answered = Forge::answer(&sandbox.reads(1), advertise).unwrap_err();
    assert_eq!(answered.code, code::NOT_FOUND);
    let repos = Query::Repos {
        page: PageRequest::first(2),
    };
    let bytes = Forge::answer(&sandbox.reads(1), repos.clone()).unwrap();
    assert_eq!(
        abi::decode::<Reply>(&bytes).unwrap(),
        Forge::query(&sandbox.reads(1), repos).unwrap()
    );
}

#[test]
fn create_names_an_owner_and_refuses_bad_or_taken_names() {
    let sandbox = founded();
    create(&sandbox, "project", HashKind::Sha1);
    let again = act(
        &sandbox,
        WRITER,
        &Op::Create {
            repo: "project".into(),
            hash: HashKind::Sha1,
        },
    );
    assert_eq!(again.unwrap_err().code, code::ALREADY_EXISTS);
    for bad in ["", "a/b", ".hidden", "x.git", "sp ace"] {
        let refused = act(
            &sandbox,
            OWNER,
            &Op::Create {
                repo: bad.into(),
                hash: HashKind::Sha1,
            },
        );
        assert_eq!(refused.unwrap_err().code, code::INVALID_INPUT, "{bad:?}");
    }
    let listed: Reply = abi::decode(
        &ask(
            &sandbox,
            &Query::Repos {
                page: PageRequest::first(128),
            },
        )
        .unwrap(),
    )
    .unwrap();
    let Reply::Repos { page, .. } = listed else {
        panic!();
    };
    let repos = page.items;
    assert_eq!(repos.len(), 1);
    assert_eq!(repos[0].name, "project");
    assert_eq!(repos[0].repo.owner, person(OWNER));
    assert_eq!(repos[0].repo.settings, Settings::default());
}

#[test]
fn only_the_owner_and_granted_writers_push() {
    let sandbox = founded();
    create(&sandbox, "project", HashKind::Sha1);
    let mut source = MemoryObjects::new(Hash::Sha1);
    let tip = file_commit(&mut source, &[], 1, &[("a", b"1")]);
    let zero = Hash::Sha1.zero();
    let pack_bytes = pack_of(&source, &all_ids(&source));
    let commands = [(zero, tip, "refs/heads/main")];

    let stranger = push(&sandbox, STRANGER, "project", &commands, &pack_bytes);
    assert_eq!(stranger.unwrap_err().code, code::UNAUTHORIZED);
    assert_eq!(sandbox.blob_count(), 0);

    let grant_by_stranger = act(
        &sandbox,
        STRANGER,
        &Op::Grant {
            repo: "project".into(),
            principal: person(WRITER),
        },
    );
    assert_eq!(grant_by_stranger.unwrap_err().code, code::UNAUTHORIZED);

    act(
        &sandbox,
        OWNER,
        &Op::Grant {
            repo: "project".into(),
            principal: person(WRITER),
        },
    )
    .unwrap();
    let writer = push(&sandbox, WRITER, "project", &commands, &pack_bytes).unwrap();
    assert_eq!(writer, ["unpack ok", "ok refs/heads/main"]);

    act(
        &sandbox,
        OWNER,
        &Op::Revoke {
            repo: "project".into(),
            principal: person(WRITER),
        },
    )
    .unwrap();
    let revoked = push(&sandbox, WRITER, "project", &commands, &pack_bytes);
    assert_eq!(revoked.unwrap_err().code, code::UNAUTHORIZED);
}

#[test]
fn a_non_fast_forward_is_reported_and_not_applied_unless_allowed() {
    let sandbox = founded();
    create(&sandbox, "project", HashKind::Sha1);
    let mut source = MemoryObjects::new(Hash::Sha1);
    let root = file_commit(&mut source, &[], 1, &[("a", b"1")]);
    let left = file_commit(&mut source, &[root], 2, &[("a", b"left")]);
    let right = file_commit(&mut source, &[root], 3, &[("a", b"right")]);
    let zero = Hash::Sha1.zero();
    let everything = pack_of(&source, &all_ids(&source));

    push(
        &sandbox,
        OWNER,
        "project",
        &[(zero, left, "refs/heads/main")],
        &everything,
    )
    .unwrap();
    let rewound = push(
        &sandbox,
        OWNER,
        "project",
        &[(left, right, "refs/heads/main")],
        b"",
    )
    .unwrap();
    assert_eq!(
        rewound,
        ["unpack ok", "ng refs/heads/main non-fast-forward"]
    );
    assert_eq!(
        refs_of(&sandbox, "project")["refs/heads/main"],
        left.to_hex()
    );

    let deleted = push(
        &sandbox,
        OWNER,
        "project",
        &[(left, zero, "refs/heads/main")],
        b"",
    )
    .unwrap();
    assert_eq!(
        deleted,
        ["unpack ok", "ng refs/heads/main deletion prohibited"]
    );

    act(
        &sandbox,
        OWNER,
        &Op::Configure {
            repo: "project".into(),
            settings: Settings {
                head: b"refs/heads/main".to_vec(),
                allow_force: true,
                allow_delete: true,
            },
        },
    )
    .unwrap();
    let forced = push(
        &sandbox,
        OWNER,
        "project",
        &[(left, right, "refs/heads/main")],
        b"",
    )
    .unwrap();
    assert_eq!(forced, ["unpack ok", "ok refs/heads/main"]);
    assert_eq!(
        refs_of(&sandbox, "project")["refs/heads/main"],
        right.to_hex()
    );
    let deleted = push(
        &sandbox,
        OWNER,
        "project",
        &[(right, zero, "refs/heads/main")],
        b"",
    )
    .unwrap();
    assert_eq!(deleted, ["unpack ok", "ok refs/heads/main"]);
    assert!(refs_of(&sandbox, "project").is_empty());
}

#[test]
fn an_import_arrives_as_fast_forward_steps_and_an_open_pack_is_refused_whole() {
    let sandbox = founded();
    create(&sandbox, "project", HashKind::Sha1);
    let mut source = MemoryObjects::new(Hash::Sha1);
    let first = file_commit(&mut source, &[], 1, &[("a", b"1"), ("b", b"1")]);
    let after_first: BTreeSet<Oid> = source.ids().copied().collect();
    let second = file_commit(&mut source, &[first], 2, &[("a", b"2"), ("b", b"1")]);
    let third = file_commit(&mut source, &[second], 3, &[("a", b"3"), ("b", b"1")]);
    let later: Vec<Oid> = source
        .ids()
        .copied()
        .filter(|id| !after_first.contains(id))
        .collect();
    let zero = Hash::Sha1.zero();

    let open = push(
        &sandbox,
        OWNER,
        "project",
        &[(zero, third, "refs/heads/main")],
        &pack_of(&source, &later),
    )
    .unwrap();
    let missing = open[0]
        .strip_prefix("unpack missing object ")
        .map(|hex| Oid::from_hex(Hash::Sha1, hex).unwrap())
        .unwrap();
    assert!(after_first.contains(&missing), "{}", open[0]);
    assert_eq!(open[1], "ng refs/heads/main n/a (unpacker error)");
    assert_eq!(sandbox.blob_count(), 0);
    assert!(refs_of(&sandbox, "project").is_empty());

    let step_one = push(
        &sandbox,
        OWNER,
        "project",
        &[(zero, first, "refs/heads/main")],
        &pack_of(&source, &after_first.iter().copied().collect::<Vec<_>>()),
    )
    .unwrap();
    assert_eq!(step_one, ["unpack ok", "ok refs/heads/main"]);
    let step_two = push(
        &sandbox,
        OWNER,
        "project",
        &[(first, third, "refs/heads/main")],
        &pack_of(&source, &later),
    )
    .unwrap();
    assert_eq!(step_two, ["unpack ok", "ok refs/heads/main"]);
    assert_eq!(sandbox.blob_count(), source.count());
    assert_eq!(
        refs_of(&sandbox, "project")["refs/heads/main"],
        third.to_hex()
    );
}

#[test]
fn a_walk_past_the_bound_asks_for_smaller_steps() {
    let sandbox = MemorySandbox::default();
    Forge::init(
        &sandbox.exec(1),
        &abi::encode(&Bounds {
            push_walk: 2,
            ..bounds()
        }),
    )
    .unwrap();
    create(&sandbox, "project", HashKind::Sha1);
    let mut source = MemoryObjects::new(Hash::Sha1);
    let mut tip = file_commit(&mut source, &[], 1, &[("a", b"0")]);
    let root = tip;
    for step in 1..5u8 {
        tip = file_commit(&mut source, &[tip], 1 + step as i64, &[("a", &[step])]);
    }
    let zero = Hash::Sha1.zero();
    let everything = pack_of(&source, &all_ids(&source));
    push(
        &sandbox,
        OWNER,
        "project",
        &[(zero, root, "refs/heads/main")],
        &everything,
    )
    .unwrap();
    let too_far = push(
        &sandbox,
        OWNER,
        "project",
        &[(root, tip, "refs/heads/main")],
        b"",
    )
    .unwrap();
    assert_eq!(
        too_far,
        [
            "unpack ok",
            "ng refs/heads/main history too long to verify; push in smaller steps"
        ]
    );
}

/// A walk outlives the blocks that wrote nothing to forge, so a long log
/// finishes while the chain moves; forge writing mid-walk restarts it.
#[test]
fn a_cursor_outlives_empty_blocks_and_a_forge_write_restarts_it() {
    use common::story::*;
    let mut rig = Rig::start(bounds(), HashKind::Sha1);
    let story = Story::pushed(&mut rig);
    let log = |after| Query::Log {
        repo: REPO.into(),
        from: forge::Revision::Oid(story.feature.clone()),
        exclude: None,
        page: PageRequest {
            after,
            limit: Some(1),
        },
    };
    let page = |rig: &Rig, after| match abi::decode(&rig.query(&log(after)).unwrap()).unwrap() {
        Reply::Log { page, .. } => page,
        _ => panic!("a log"),
    };
    let first = page(&rig, None);
    assert_eq!(first.items[0].oid, story.feature);
    rig.advance();
    rig.advance();
    let second = page(&rig, first.next.clone());
    assert_eq!(second.items[0].oid, story.root);
    rig.sandbox.hold(b"ninth", 9);
    rig.execute(&Op::Grant {
        repo: REPO.into(),
        principal: Principal::Account(9),
    })
    .unwrap();
    assert_eq!(rig.query(&log(first.next)).unwrap_err().code, code::STALE);
}

/// A change's commits are its source's history less its target's.
#[test]
fn a_log_leaves_out_what_its_exclude_reaches() {
    use common::story::*;
    let mut rig = Rig::start(bounds(), HashKind::Sha1);
    let story = Story::pushed(&mut rig);
    let bytes = rig
        .query(&Query::Log {
            repo: REPO.into(),
            from: reference("feature"),
            exclude: Some(reference("main")),
            page: PageRequest::first(64),
        })
        .unwrap();
    let Reply::Log { page, .. } = abi::decode(&bytes).unwrap() else {
        panic!("a log");
    };
    let oids: Vec<_> = page.items.into_iter().map(|c| c.oid).collect();
    assert_eq!(oids, [story.feature]);
}

/// The app reads at the preconfirmed layer, whose height is the one its
/// next ops run at: a push in the block that answered the cursor rewrites
/// the log at the same height, and the cursor still tells.
#[test]
fn a_forge_write_at_the_answering_height_restarts_the_walk() {
    use common::story::*;
    let mut rig = Rig::start(bounds(), HashKind::Sha1);
    let story = Story::pushed(&mut rig);
    let log = |after| Query::Log {
        repo: REPO.into(),
        from: reference("feature"),
        exclude: None,
        page: PageRequest {
            after,
            limit: Some(1),
        },
    };
    let Reply::Log { page, .. } = abi::decode(&rig.query(&log(None)).unwrap()).unwrap() else {
        panic!("a log");
    };
    assert_eq!(page.items[0].oid, story.feature);
    // no advance: the same height the page was answered at
    rig.sandbox.hold(b"ninth", 9);
    let (actor, height) = (rig.actor.clone(), rig.height);
    signed_op(
        &rig.sandbox,
        &actor,
        height,
        &Op::Grant {
            repo: REPO.into(),
            principal: Principal::Account(9),
        },
    )
    .unwrap();
    assert_eq!(rig.query(&log(page.next)).unwrap_err().code, code::STALE);
}

/// What `exclude` reaches walks under its own budget: a target with more
/// history than the change's own commits does not refuse the listing.
#[test]
fn a_log_exclude_spends_its_own_walk_budget() {
    use common::story::*;
    let mut rig = Rig::start(
        forge::Bounds {
            log_walk: 2,
            ..bounds()
        },
        HashKind::Sha1,
    );
    let story = Story::pushed(&mut rig);
    let bytes = rig
        .query(&Query::Log {
            repo: REPO.into(),
            from: reference("feature"),
            exclude: Some(reference("unrelated")),
            page: PageRequest::first(64),
        })
        .unwrap();
    let Reply::Log { page, .. } = abi::decode(&bytes).unwrap() else {
        panic!("a log");
    };
    let oids: Vec<_> = page.items.into_iter().map(|c| c.oid).collect();
    assert_eq!(oids, [story.feature, story.root]);
}

fn stage(sandbox: &MemorySandbox, actor: &[u8], pack: Vec<u8>) -> Result<Vec<u8>, guest::Error> {
    act(
        sandbox,
        actor,
        &Op::Stage {
            repo: "project".into(),
            pack,
        },
    )
}

/// The ids under `commit` that `source` gained for it: its tree and blobs,
/// the commit itself last.
fn objects_of(source: &MemoryObjects, commit: Oid) -> (Vec<Oid>, Oid) {
    let object = source.get(&commit).unwrap().unwrap();
    let parsed = Commit::parse(&object.body, Hash::Sha1).unwrap();
    let tree = source.get(&parsed.tree).unwrap().unwrap();
    let mut ids = vec![parsed.tree];
    ids.extend(
        Tree::parse(&tree.body, Hash::Sha1)
            .unwrap()
            .entries
            .iter()
            .map(|entry| entry.id),
    );
    (ids, commit)
}

#[test]
fn a_stage_stores_objects_and_moves_no_ref() {
    let sandbox = founded();
    create(&sandbox, "project", HashKind::Sha1);
    let mut source = MemoryObjects::new(Hash::Sha1);
    let tip = file_commit(&mut source, &[], 1, &[("a", b"1"), ("b", b"2")]);
    let (below, _) = objects_of(&source, tip);

    // a commit whose tree is not held, alone: refused, nothing stored
    let open = stage(&sandbox, OWNER, pack_of(&source, &[tip]));
    assert_eq!(open.unwrap_err().code, code::NOT_FOUND);
    assert_eq!(sandbox.blob_count(), 0);

    stage(&sandbox, OWNER, pack_of(&source, &below)).unwrap();
    assert_eq!(sandbox.blob_count(), below.len());
    stage(&sandbox, OWNER, pack_of(&source, &[tip])).unwrap();
    assert_eq!(sandbox.blob_count(), source.count());
    assert!(refs_of(&sandbox, "project").is_empty());

    // a pack that is no pack is refused as one
    let garbage = stage(&sandbox, OWNER, b"PACK not really".to_vec());
    assert_eq!(garbage.unwrap_err().code, code::INVALID_INPUT);
}

#[test]
fn only_the_owner_and_granted_writers_stage() {
    let sandbox = founded();
    create(&sandbox, "project", HashKind::Sha1);
    let mut source = MemoryObjects::new(Hash::Sha1);
    file_commit(&mut source, &[], 1, &[("a", b"1")]);
    let everything = pack_of(&source, &all_ids(&source));

    let stranger = stage(&sandbox, STRANGER, everything.clone());
    assert_eq!(stranger.unwrap_err().code, code::UNAUTHORIZED);
    assert_eq!(sandbox.blob_count(), 0);

    act(
        &sandbox,
        OWNER,
        &Op::Grant {
            repo: "project".into(),
            principal: person(WRITER),
        },
    )
    .unwrap();
    stage(&sandbox, WRITER, everything).unwrap();
    assert_eq!(sandbox.blob_count(), source.count());
}

/// Every object a stage stored had what it names held when it landed, so
/// the push that moves the ref brings only what is left: the commit alone
/// (thin, its tree staged), or nothing at all.
#[test]
fn a_push_moves_a_ref_onto_staged_objects() {
    let sandbox = founded();
    create(&sandbox, "project", HashKind::Sha1);
    let mut source = MemoryObjects::new(Hash::Sha1);
    let first = file_commit(&mut source, &[], 1, &[("a", b"1")]);
    let second = file_commit(&mut source, &[first], 2, &[("a", b"2")]);
    let zero = Hash::Sha1.zero();

    let (below_first, _) = objects_of(&source, first);
    stage(&sandbox, OWNER, pack_of(&source, &below_first)).unwrap();
    let thin = push(
        &sandbox,
        OWNER,
        "project",
        &[(zero, first, "refs/heads/main")],
        &pack_of(&source, &[first]),
    )
    .unwrap();
    assert_eq!(thin, ["unpack ok", "ok refs/heads/main"]);

    let (mut below_second, _) = objects_of(&source, second);
    below_second.retain(|id| !below_first.contains(id));
    below_second.push(second);
    stage(&sandbox, OWNER, pack_of(&source, &below_second)).unwrap();
    let empty = push(
        &sandbox,
        OWNER,
        "project",
        &[(first, second, "refs/heads/main")],
        b"",
    )
    .unwrap();
    assert_eq!(empty, ["unpack ok", "ok refs/heads/main"]);
    assert_eq!(
        refs_of(&sandbox, "project")["refs/heads/main"],
        second.to_hex()
    );
    assert_eq!(sandbox.blob_count(), source.count());
}

/// A push whose ref is refused keeps the objects it unpacked, as git's
/// receive-pack does: a push again after the refusal brings none of them.
#[test]
fn a_refused_ref_keeps_the_objects_its_pack_brought() {
    let sandbox = founded();
    create(&sandbox, "project", HashKind::Sha1);
    let mut source = MemoryObjects::new(Hash::Sha1);
    let tip = file_commit(&mut source, &[], 1, &[("a", b"1")]);
    let zero = Hash::Sha1.zero();
    let stale = push(
        &sandbox,
        OWNER,
        "project",
        &[(tip, tip, "refs/heads/main")],
        &pack_of(&source, &all_ids(&source)),
    )
    .unwrap();
    assert_eq!(stale, ["unpack ok", "ng refs/heads/main stale info"]);
    assert_eq!(sandbox.blob_count(), source.count());
    let again = push(
        &sandbox,
        OWNER,
        "project",
        &[(zero, tip, "refs/heads/main")],
        b"",
    )
    .unwrap();
    assert_eq!(again, ["unpack ok", "ok refs/heads/main"]);
}

/// Incompressible bytes.
fn noise(len: usize, seed: u64) -> Vec<u8> {
    let mut state = seed | 1;
    (0..len)
        .map(|_| {
            state ^= state << 13;
            state ^= state >> 7;
            state ^= state << 17;
            state as u8
        })
        .collect()
}

/// A commit like the font bundles in ducktape's history: its own objects
/// take more than a block carries (16 MiB, core's `node::BLOCK_BYTES`), so
/// no one push holds it. Staged a blob at a time, it lands, and the push
/// that moves the ref holds the tree and the commit alone.
#[test]
fn a_commit_bigger_than_a_frame_lands_in_parts() {
    const FRAME: usize = 16 << 20;
    let sandbox = MemorySandbox::default();
    Forge::init(
        &sandbox.exec(1),
        &abi::encode(&Bounds {
            max_object_size: 8 << 20,
            ..bounds()
        }),
    )
    .unwrap();
    create(&sandbox, "project", HashKind::Sha1);
    let mut source = MemoryObjects::new(Hash::Sha1);
    let fonts: Vec<(String, Vec<u8>)> = (0..3)
        .map(|n| (format!("font-{n}.ttf"), noise(6 << 20, n * 2 + 1)))
        .collect();
    let files: Vec<(&str, &[u8])> = fonts
        .iter()
        .map(|(name, bytes)| (name.as_str(), bytes.as_slice()))
        .collect();
    let tip = file_commit(&mut source, &[], 1, &files);
    let (below, _) = objects_of(&source, tip);
    let (tree, blobs) = below.split_first().unwrap();
    assert!(pack_of(&source, &all_ids(&source)).len() > FRAME);

    for blob in blobs {
        let part = pack_of(&source, &[*blob]);
        assert!(part.len() < FRAME);
        stage(&sandbox, OWNER, part).unwrap();
    }
    let last = pack_of(&source, &[*tree, tip]);
    assert!(last.len() < FRAME);
    let moved = push(
        &sandbox,
        OWNER,
        "project",
        &[(Hash::Sha1.zero(), tip, "refs/heads/main")],
        &last,
    )
    .unwrap();
    assert_eq!(moved, ["unpack ok", "ok refs/heads/main"]);
    assert_eq!(sandbox.blob_count(), source.count());
}

/// A forge founded with `bounds` holding `project` with a history: c1 ← c2 ←
/// c3 ← c4, a side commit merged at c4, each with a file of its own; and the
/// source the pack came from.
fn history(bounds: Bounds) -> (MemorySandbox, MemoryObjects, [Oid; 5]) {
    let sandbox = MemorySandbox::default();
    Forge::init(&sandbox.exec(1), &abi::encode(&bounds)).unwrap();
    create(&sandbox, "project", HashKind::Sha1);
    let mut source = MemoryObjects::new(Hash::Sha1);
    let c1 = file_commit(&mut source, &[], 1, &[("one", b"1")]);
    let c2 = file_commit(&mut source, &[c1], 2, &[("two", b"2")]);
    let side = file_commit(&mut source, &[c1], 3, &[("side", b"s")]);
    let c3 = file_commit(&mut source, &[c2], 4, &[("three", b"3")]);
    let c4 = file_commit(&mut source, &[c3, side], 5, &[("four", b"4")]);
    let moved = push(
        &sandbox,
        OWNER,
        "project",
        &[(Hash::Sha1.zero(), c4, "refs/heads/main")],
        &pack_of(&source, &all_ids(&source)),
    )
    .unwrap();
    assert_eq!(moved, ["unpack ok", "ok refs/heads/main"]);
    (sandbox, source, [c1, c2, side, c3, c4])
}

fn objects(sandbox: &MemorySandbox, ids: &[Oid], ancestors: bool) -> (Vec<Oid>, Vec<String>) {
    let query = Query::Objects {
        repo: "project".into(),
        ids: ids.iter().map(|id| id.to_hex()).collect(),
        ancestors,
    };
    let Reply::Objects {
        objects, missing, ..
    } = abi::decode(&ask(sandbox, &query).unwrap()).unwrap()
    else {
        panic!("objects");
    };
    let ids = objects
        .iter()
        .map(|object| Oid::from_hex(Hash::Sha1, &object.oid).unwrap())
        .collect();
    (ids, missing)
}

#[test]
fn objects_come_by_id_as_git_stores_them_and_the_unheld_are_named() {
    let (sandbox, source, [c1, _, _, _, c4]) = history(bounds());
    let tree = Commit::parse(&source.get(&c4).unwrap().unwrap().body, Hash::Sha1)
        .unwrap()
        .tree;
    let absent = Oid::from_hex(Hash::Sha1, "ab".repeat(20)).unwrap();
    let query = Query::Objects {
        repo: "project".into(),
        ids: [tree, absent, c1].iter().map(|id| id.to_hex()).collect(),
        ancestors: false,
    };
    let Reply::Objects {
        objects, missing, ..
    } = abi::decode(&ask(&sandbox, &query).unwrap()).unwrap()
    else {
        panic!("objects");
    };
    assert_eq!(missing, [absent.to_hex()]);
    let held: Vec<_> = objects
        .iter()
        .map(|object| (object.oid.clone(), object.kind, object.body.clone()))
        .collect();
    let body = |id: &Oid| source.get(id).unwrap().unwrap().body;
    assert_eq!(
        held,
        [
            (tree.to_hex(), forge::ObjectKind::Tree, body(&tree)),
            (c1.to_hex(), forge::ObjectKind::Commit, body(&c1)),
        ]
    );
}

/// The budget is a query's read budget: `tree_walk` reads, `diff_bytes`
/// bytes. An id past it is in neither list, to ask again; the first held
/// object comes whatever its size, so no object is out of reach.
#[test]
fn the_read_budget_cuts_the_ids_and_the_first_held_comes_past_it() {
    let (sandbox, _, [c1, c2, _, c3, c4]) = history(Bounds {
        tree_walk: 3,
        ..bounds()
    });
    assert_eq!(objects(&sandbox, &[c4, c3, c2, c1], false).0, [c4, c3, c2]);
    let (sandbox, source, [c1, c2, ..]) = history(Bounds {
        diff_bytes: 10,
        blob_bytes: 10,
        ..bounds()
    });
    assert!(source.get(&c1).unwrap().unwrap().body.len() > 10);
    assert_eq!(objects(&sandbox, &[c1, c2], false), (vec![c1], vec![]));
}

/// With `ancestors`, what the budget leaves brings the commits the named
/// ones descend from, breadth first: only commits, each once.
#[test]
fn ancestors_come_breadth_first_while_the_budget_lasts() {
    let (sandbox, _, [c1, c2, side, c3, c4]) = history(bounds());
    assert_eq!(objects(&sandbox, &[c4], true).0, [c4, c3, side, c2, c1]);
    assert_eq!(objects(&sandbox, &[c4], false).0, [c4]);
    let (sandbox, _, [_, _, side, c3, c4]) = history(Bounds {
        tree_walk: 3,
        ..bounds()
    });
    assert_eq!(objects(&sandbox, &[c4], true).0, [c4, c3, side]);
}
