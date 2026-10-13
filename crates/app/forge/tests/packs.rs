//! forge keeps git's packs: a stage or a push stores its pack as one blob,
//! deltas and all, and indexes each object at its entry; a delta against
//! what forge held before lands whole; old loose objects still read.

mod common;
use common::*;

use forge::tables::{Location, PACKED, PACKS};
use gitcore::pack::Payload;

macro_rules! git_fixture {
    ($name:literal) => {
        include_bytes!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../lib/gitcore/tests/fixtures/",
            $name
        )) as &[u8]
    };
}

/// git's packs of one repository (gitcore's fixtures, `make.sh`): the base
/// commit's objects, then the two commits after it with their deltas
/// against each other (`ofs`, `ref`) or against the base too (`thin`).
const BASE: &[u8] = git_fixture!("base.pack");
const OFS: &[u8] = git_fixture!("ofs.pack");
const REF: &[u8] = git_fixture!("ref.pack");
const THIN: &[u8] = git_fixture!("thin.pack");

fn objects_in(pack_bytes: &[u8], base: &MemoryObjects) -> MemoryObjects {
    let mut objects = MemoryObjects::new(Hash::Sha1);
    for (_, object) in pack::read(pack_bytes, Hash::Sha1, &Limits::generous(), |id| {
        base.get(id)
    })
    .unwrap()
    {
        objects.put(object.kind, &object.body).unwrap();
    }
    objects
}

fn stage(sandbox: &MemorySandbox, pack: &[u8]) -> Result<Vec<u8>, guest::Error> {
    act(
        sandbox,
        OWNER,
        &Op::Stage {
            repo: "project".into(),
            pack: pack.to_vec(),
        },
    )
}

fn location(sandbox: &MemorySandbox, id: &Oid) -> Location {
    PACKED
        .get(&sandbox.reads(1), &id.as_bytes().to_vec())
        .unwrap()
        .unwrap_or_else(|| panic!("{id} is not packed"))
}

/// The entry `id` is indexed at, as its pack holds it.
fn entry(sandbox: &MemorySandbox, id: &Oid) -> Payload {
    let at = location(sandbox, id);
    let reads = sandbox.reads(1);
    let blob = PACKS.get(&reads, &at.pack).unwrap().unwrap();
    let bytes = reads.blob_get(blob).unwrap().body;
    pack::entry_at(&bytes, at.offset as usize, Hash::Sha1, usize::MAX)
        .unwrap()
        .0
}

fn founded_project() -> MemorySandbox {
    let sandbox = founded();
    create(&sandbox, "project", HashKind::Sha1);
    sandbox
}

#[test]
fn a_stage_keeps_git_s_pack_whole_deltas_and_all() {
    let sandbox = founded_project();
    let base = objects_in(BASE, &MemoryObjects::new(Hash::Sha1));
    let later = objects_in(OFS, &base);
    stage(&sandbox, BASE).unwrap();
    assert_eq!(sandbox.blob_count(), 1);
    stage(&sandbox, OFS).unwrap();
    assert_eq!(sandbox.blob_count(), 2);
    holds_all(&sandbox, "project", &base);
    holds_all(&sandbox, "project", &later);
    let deltas = later
        .ids()
        .filter(|id| matches!(entry(&sandbox, id), Payload::OfsDelta { .. }))
        .count();
    assert!(deltas > 0, "git's deltas are kept");
    // a delta's row spans its chain, base before it, and no more of the pack
    for id in later.ids() {
        let at = location(&sandbox, id);
        assert!(at.start <= at.offset && at.offset < at.end, "{id}");
        if let Payload::OfsDelta { base_offset, .. } = entry(&sandbox, id) {
            assert!(at.start as usize <= base_offset, "{id}");
        }
        assert!(((at.end - at.start) as usize) < OFS.len());
    }
    assert!(later.ids().all(|id| location(&sandbox, id).pack == 1));
}

/// A delta against an object of the same pack, named by id, reads inside it.
#[test]
fn a_ref_delta_inside_its_pack_stays_a_delta() {
    let sandbox = founded_project();
    let base = objects_in(BASE, &MemoryObjects::new(Hash::Sha1));
    let later = objects_in(REF, &base);
    stage(&sandbox, BASE).unwrap();
    stage(&sandbox, REF).unwrap();
    assert_eq!(sandbox.blob_count(), 2);
    holds_all(&sandbox, "project", &later);
    assert!(
        later
            .ids()
            .any(|id| matches!(entry(&sandbox, id), Payload::RefDelta { .. }))
    );
}

/// The deltas of `pack`'s objects that are against an object outside it,
/// each with that base.
fn thin_deltas(sandbox: &MemorySandbox, objects: &MemoryObjects) -> Vec<(Oid, Oid)> {
    objects
        .ids()
        .filter_map(|id| match entry(sandbox, id) {
            Payload::RefDelta { base, .. } if objects.get(&base).unwrap().is_none() => {
                Some((*id, base))
            }
            _ => None,
        })
        .collect()
}

/// git's thin pack (what a push sends) deltifies against what forge holds:
/// a split push's earlier part, or an earlier push. Those deltas stay
/// deltas, and a read goes on into the pack that holds the base.
#[test]
fn a_thin_pack_keeps_its_deltas_against_held_objects() {
    let sandbox = founded_project();
    let base = objects_in(BASE, &MemoryObjects::new(Hash::Sha1));
    let later = objects_in(THIN, &base);
    stage(&sandbox, BASE).unwrap();
    assert!(pack::read(THIN, Hash::Sha1, &Limits::generous(), |_| Ok(None)).is_err());
    stage(&sandbox, THIN).unwrap();
    assert_eq!(sandbox.blob_count(), 2);
    holds_all(&sandbox, "project", &base);
    holds_all(&sandbox, "project", &later);
    let thin = thin_deltas(&sandbox, &later);
    assert!(!thin.is_empty());
    for (id, base_id) in thin {
        assert_eq!(
            (
                location(&sandbox, &id).pack,
                location(&sandbox, &base_id).pack
            ),
            (1, 0)
        );
        assert!(location(&sandbox, &id).depth > location(&sandbox, &base_id).depth);
    }
}

/// Objects forge stored before packs, a blob each, keep reading, and a
/// pack's delta against one stays a delta and resolves.
#[test]
fn a_delta_against_a_loose_object_resolves() {
    let sandbox = founded_project();
    let base = objects_in(BASE, &MemoryObjects::new(Hash::Sha1));
    let later = objects_in(THIN, &base);
    let ctx = sandbox.exec(1);
    for id in base.ids() {
        let object = base.get(id).unwrap().unwrap();
        ctx.blob_put(HashKind::Sha1, object.kind.as_str(), object.body)
            .unwrap();
        assert!(is_loose(&sandbox, id));
    }
    let loose = sandbox.blob_count();
    stage(&sandbox, THIN).unwrap();
    assert_eq!(sandbox.blob_count(), loose + 1);
    for id in base.ids() {
        assert_eq!(object_of(&sandbox, "project", id), base.get(id).unwrap());
    }
    holds_all(&sandbox, "project", &later);
    let thin = thin_deltas(&sandbox, &later);
    assert!(!thin.is_empty());
    assert!(thin.iter().all(|(id, _)| location(&sandbox, id).depth == 1));
}

/// A pack whose objects forge holds already adds nothing: not stored, not
/// indexed again (the first pack that brought an object keeps it).
#[test]
fn a_pack_of_held_objects_is_not_stored_again() {
    let sandbox = founded_project();
    let base = objects_in(BASE, &MemoryObjects::new(Hash::Sha1));
    stage(&sandbox, BASE).unwrap();
    stage(&sandbox, BASE).unwrap();
    assert_eq!(sandbox.blob_count(), 1);
    holds_all(&sandbox, "project", &base);
}

/// Pushes to a forge founded with `bounds` that each change one file a
/// little, as git sends them (thin, the file a delta against its version
/// before); every version reads back. The file's blob of each version, or
/// none without git.
fn one_file_pushed(bounds: Bounds, versions: usize) -> Option<(MemorySandbox, Vec<Oid>)> {
    let git = std::process::Command::new("git")
        .arg("--version")
        .output()
        .ok()?;
    assert!(git.status.success());
    let dir = std::env::temp_dir().join(format!(
        "forge-packs-{}-{:?}",
        std::process::id(),
        std::thread::current().id()
    ));
    std::fs::remove_dir_all(&dir).ok();
    std::fs::create_dir_all(&dir).unwrap();
    let run = |args: &[&str], input: Option<&[u8]>| -> Vec<u8> {
        let mut command = std::process::Command::new("git");
        command
            .args(args)
            .current_dir(&dir)
            .env("GIT_CONFIG_GLOBAL", "/dev/null")
            .env("GIT_CONFIG_SYSTEM", "/dev/null")
            .env("GIT_AUTHOR_NAME", "Ada")
            .env("GIT_AUTHOR_EMAIL", "ada@example.com")
            .env("GIT_COMMITTER_NAME", "Ada")
            .env("GIT_COMMITTER_EMAIL", "ada@example.com")
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped());
        let mut child = command.spawn().unwrap();
        std::io::Write::write_all(child.stdin.as_mut().unwrap(), input.unwrap_or_default())
            .unwrap();
        let output = child.wait_with_output().unwrap();
        assert!(output.status.success(), "git {args:?}");
        output.stdout
    };
    run(&["init", "-q", "-b", "main"], None);
    let lines: Vec<String> = (0..400)
        .map(|n| format!("line {n} of a lock file\n"))
        .collect();
    let sandbox = MemorySandbox::default();
    sandbox.chain.init("forge", &bounds).unwrap();
    create(&sandbox, "project", HashKind::Sha1);
    let mut old = Hash::Sha1.zero();
    let mut commits = Vec::new();
    for version in 0..versions {
        let mut text = lines.clone();
        text[version * 40] = format!("line changed in version {version}\n");
        std::fs::write(dir.join("Cargo.lock"), text.concat()).unwrap();
        run(&["add", "."], None);
        run(&["commit", "-q", "-m", &format!("v{version}")], None);
        let tip = String::from_utf8(run(&["rev-parse", "HEAD"], None)).unwrap();
        let tip = Oid::from_hex(Hash::Sha1, tip.trim()).unwrap();
        let revs = match old.is_zero() {
            true => format!("{tip}\n"),
            false => format!("{tip}\n^{old}\n"),
        };
        let thin = run(
            &["pack-objects", "--revs", "--thin", "--stdout"],
            Some(revs.as_bytes()),
        );
        let alone = pack::read(&thin, Hash::Sha1, &Limits::generous(), |_| Ok(None));
        assert_eq!(
            alone.is_err(),
            version > 0,
            "v{version}: git's pack is thin"
        );
        let report = push(
            &sandbox,
            OWNER,
            "project",
            &[(old, tip, "refs/heads/main")],
            &thin,
        )
        .unwrap();
        assert_eq!(report, ["unpack ok", "ok refs/heads/main"]);
        old = tip;
        commits.push(tip);
    }
    std::fs::remove_dir_all(&dir).ok();
    let listed = run_list(&sandbox, &commits);
    assert_eq!(listed.len(), commits.len());
    let mut locks = Vec::new();
    for (version, commit) in commits.iter().enumerate() {
        let tree = Commit::parse(
            &object_of(&sandbox, "project", commit).unwrap().body,
            Hash::Sha1,
        )
        .unwrap()
        .tree;
        let tree = Tree::parse(
            &object_of(&sandbox, "project", &tree).unwrap().body,
            Hash::Sha1,
        )
        .unwrap();
        let lock = tree.entries[0].id;
        let body = object_of(&sandbox, "project", &lock).unwrap().body;
        let expected = format!("line changed in version {version}\n");
        assert!(String::from_utf8(body).unwrap().contains(&expected));
        locks.push(lock);
    }
    Some((sandbox, locks))
}

/// Each version after the first stays git's delta against the one before,
/// its chain going back pack by pack.
#[test]
fn a_file_changed_push_after_push_keeps_its_deltas() {
    let Some((sandbox, locks)) = one_file_pushed(bounds(), 8) else {
        return;
    };
    for (version, lock) in locks.iter().enumerate() {
        let at = location(&sandbox, lock);
        assert_eq!(usize::from(at.depth), version, "v{version}");
        assert_eq!(
            matches!(entry(&sandbox, lock), Payload::Full(..)),
            version == 0
        );
    }
}

/// A chain that would apply more deltas than the founded `max_delta_depth`
/// starts again: that version lands whole, and the next deltas against it.
#[test]
fn a_chain_past_the_founded_depth_starts_again_whole() {
    let shallow = Bounds {
        max_delta_depth: 3,
        ..bounds()
    };
    let Some((sandbox, locks)) = one_file_pushed(shallow, 9) else {
        return;
    };
    let depths: Vec<u8> = locks
        .iter()
        .map(|lock| location(&sandbox, lock).depth)
        .collect();
    assert_eq!(depths, [0, 1, 2, 3, 0, 1, 2, 3, 0]);
    for (lock, depth) in locks.iter().zip(depths) {
        assert_eq!(
            matches!(entry(&sandbox, lock), Payload::Full(..)),
            depth == 0
        );
    }
}

/// The history forge lists from the tip: every commit pushed.
fn run_list(sandbox: &MemorySandbox, commits: &[Oid]) -> Vec<String> {
    let reply: Reply = abi::decode(
        &ask(
            sandbox,
            &Query::Log {
                repo: "project".into(),
                from: forge::Revision::Ref(b"refs/heads/main".to_vec()),
                exclude: None,
                page: PageRequest::first(64),
            },
        )
        .unwrap(),
    )
    .unwrap();
    let Reply::Log { page, .. } = reply else {
        panic!("a log");
    };
    let listed: Vec<String> = page.items.into_iter().map(|commit| commit.oid).collect();
    let pushed: BTreeSet<String> = commits.iter().map(Oid::to_hex).collect();
    assert_eq!(listed.iter().cloned().collect::<BTreeSet<_>>(), pushed);
    listed
}

/// A history pushed a commit at a time, each push its own pack: a walk
/// down it loads a pack a commit.
fn pushed_a_commit_at_a_time(count: usize) -> (MemorySandbox, Vec<Oid>) {
    let sandbox = founded_project();
    let mut source = MemoryObjects::new(Hash::Sha1);
    let mut commits: Vec<Oid> = Vec::new();
    let zero = Hash::Sha1.zero();
    for n in 0..count {
        let held: BTreeSet<Oid> = source.ids().copied().collect();
        let parents: Vec<Oid> = commits.last().copied().into_iter().collect();
        let content = format!("version {n}");
        let tip = file_commit(
            &mut source,
            &parents,
            n as i64,
            &[("file", content.as_bytes())],
        );
        let new: Vec<Oid> = source
            .ids()
            .copied()
            .filter(|id| !held.contains(id))
            .collect();
        let old = commits.last().copied().unwrap_or(zero);
        let report = push(
            &sandbox,
            OWNER,
            "project",
            &[(old, tip, "refs/heads/main")],
            &pack_of(&source, &new),
        )
        .unwrap();
        assert_eq!(report, ["unpack ok", "ok refs/heads/main"]);
        commits.push(tip);
    }
    assert_eq!(sandbox.blob_count(), count);
    (sandbox, commits)
}

/// A read takes the bytes its chain spans, never a whole pack, so a walk
/// across a history pushed a pack a commit reads it all, and `Objects`
/// brings every ancestor.
#[test]
fn a_history_in_many_packs_reads_whole() {
    let (sandbox, commits) = pushed_a_commit_at_a_time(40);
    run_list(&sandbox, &commits);
    let query = Query::Objects {
        repo: "project".into(),
        ids: vec![commits[39].to_hex()],
        ancestors: true,
    };
    let Reply::Objects {
        objects, missing, ..
    } = abi::decode(&ask(&sandbox, &query).unwrap()).unwrap()
    else {
        panic!("objects");
    };
    assert!(missing.is_empty());
    let answered: Vec<String> = objects.into_iter().map(|object| object.oid).collect();
    let newest_first: Vec<String> = commits.iter().rev().map(Oid::to_hex).collect();
    assert_eq!(answered, newest_first);
}

/// A chain deeper than the founded `max_delta_depth` is refused at the
/// stage and stores nothing; one the index would loop through (a corrupt
/// row) is refused at the read, past the same depth.
#[test]
fn a_delta_chain_past_the_founded_depth_is_refused() {
    let sandbox = MemorySandbox::default();
    sandbox
        .chain
        .init(
            "forge",
            &Bounds {
                max_delta_depth: 0,
                ..bounds()
            },
        )
        .unwrap();
    create(&sandbox, "project", HashKind::Sha1);
    stage(&sandbox, BASE).unwrap();
    let deep = sandbox.forge.refused(|| {
        signed_op(
            &sandbox,
            OWNER,
            1,
            &Op::Stage {
                repo: "project".into(),
                pack: OFS.to_vec(),
            },
        )
    });
    assert_eq!(deep.code, code::INVALID_INPUT);
    assert_eq!(sandbox.blob_count(), 1);

    let sandbox = founded_project();
    let base = objects_in(BASE, &MemoryObjects::new(Hash::Sha1));
    let later = objects_in(REF, &base);
    stage(&sandbox, BASE).unwrap();
    stage(&sandbox, REF).unwrap();
    let (delta, base_id) = later
        .ids()
        .find_map(|id| match entry(&sandbox, id) {
            Payload::RefDelta { base, .. } => Some((*id, base)),
            _ => None,
        })
        .unwrap();
    // the base's row now names the delta's own entry: a loop
    PACKED.put(
        &sandbox.exec(1),
        &base_id.as_bytes().to_vec(),
        &location(&sandbox, &delta),
    );
    let looped = Query::Objects {
        repo: "project".into(),
        ids: vec![delta.to_hex()],
        ancestors: false,
    };
    assert_eq!(
        ask(&sandbox, &looped).unwrap_err().code,
        code::INVALID_INPUT
    );
}
