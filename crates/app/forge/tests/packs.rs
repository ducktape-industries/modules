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

/// git's thin pack (what a push sends) deltifies against what forge holds:
/// a split push's earlier part, or an earlier push. Those objects land
/// whole in one pack forge writes, so no read follows a chain from pack to
/// pack and a chain does not grow with every push.
#[test]
fn a_thin_pack_s_deltas_against_held_objects_land_whole() {
    let sandbox = founded_project();
    let base = objects_in(BASE, &MemoryObjects::new(Hash::Sha1));
    let later = objects_in(THIN, &base);
    stage(&sandbox, BASE).unwrap();
    let thin = pack::read(THIN, Hash::Sha1, &Limits::generous(), |id| base.get(id));
    assert!(thin.is_ok());
    assert!(pack::read(THIN, Hash::Sha1, &Limits::generous(), |_| Ok(None)).is_err());
    stage(&sandbox, THIN).unwrap();
    holds_all(&sandbox, "project", &base);
    holds_all(&sandbox, "project", &later);
    let packs: BTreeSet<u32> = later.ids().map(|id| location(&sandbox, id).pack).collect();
    assert_eq!(sandbox.blob_count(), 1 + packs.len());
    let whole_in_forge_s = later
        .ids()
        .filter(|id| location(&sandbox, id).pack == 2)
        .inspect(|id| assert!(matches!(entry(&sandbox, id), Payload::Full(..)), "{id}"))
        .count();
    assert!(whole_in_forge_s > 0);
}

/// Objects forge stored before packs, a blob each, keep reading, and a
/// pack's delta against one resolves.
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
    for id in base.ids() {
        assert_eq!(object_of(&sandbox, "project", id), base.get(id).unwrap());
    }
    holds_all(&sandbox, "project", &later);
    assert!(sandbox.blob_count() > loose);
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

/// Pushes that each change one file a little, as git sends them (thin, the
/// file a delta against its version before): every version reads back,
/// each whole where it is indexed, however many pushes came before.
#[test]
fn a_file_changed_push_after_push_reads_back_every_version() {
    let Ok(git) = std::process::Command::new("git").arg("--version").output() else {
        return;
    };
    assert!(git.status.success());
    let dir = std::env::temp_dir().join(format!("forge-packs-{}", std::process::id()));
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
    let sandbox = founded_project();
    let mut old = Hash::Sha1.zero();
    let mut commits = Vec::new();
    for version in 0..8 {
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
        assert!(
            matches!(entry(&sandbox, &lock), Payload::Full(..)),
            "v{version}"
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

/// One query loads at most `MAX_PACKS` (16) packs: a walk that needs more
/// is refused as past its bound, and `Objects` ends its batch there instead.
#[test]
fn a_read_past_sixteen_packs_is_refused_and_a_batch_ends_there() {
    let (sandbox, commits) = pushed_a_commit_at_a_time(16);
    run_list(&sandbox, &commits);
    let (sandbox, commits) = pushed_a_commit_at_a_time(17);
    let log = Query::Log {
        repo: "project".into(),
        from: forge::Revision::Ref(b"refs/heads/main".to_vec()),
        exclude: None,
        page: PageRequest::first(64),
    };
    assert_eq!(ask(&sandbox, &log).unwrap_err().code, code::CAPACITY);
    let query = Query::Objects {
        repo: "project".into(),
        ids: vec![commits[16].to_hex()],
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
    let newest: Vec<String> = commits.iter().rev().take(16).map(Oid::to_hex).collect();
    assert_eq!(answered, newest);
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
