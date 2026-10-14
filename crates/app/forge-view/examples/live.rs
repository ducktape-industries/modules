//! `cargo run -p forge-view --example live`: Forge in the real app over a
//! fake node with a repository and the people who may push to it, under the
//! bounds a founding gives forge. `web` holds a history: a long branch and
//! tag list, a long source file, and three open changes: a large diff, a
//! small one, and a release the size of a real one (280 files, +78k −21k,
//! binaries, a file too long to diff); `docs` is empty; `mine` is the
//! reader's own.
use ducktape_view_guest::live;
use forge::{Op, Revision};
use gitcore::wire::pktline;
use gitcore::{
    Commit, Hash, Kind, MemoryObjects, Mode, Objects, Oid, Signature, Tree, TreeEntry, pack,
};

fn main() {
    live::run("forge", |net| {
        net.seat::<chat::Chat>(chat::MODULE);
        net.seat::<forge::Forge>(forge::MODULE);
        net.init(forge::MODULE, &founding::forge_bounds())?;
        let ada = net.person("ada");
        for repo in ["web", "docs"] {
            net.submit(
                ada,
                forge::MODULE,
                &Op::Create {
                    repo: repo.into(),
                    hash: abi::HashKind::Sha256,
                },
            )?;
            net.submit(
                ada,
                forge::MODULE,
                &Op::Grant {
                    repo: repo.into(),
                    principal: forge::Principal::Account(net.me()),
                },
            )?;
        }
        // one repository of my own, so its Settings (owner only) can be
        // tried: pushed the same history
        net.submit(
            net.me(),
            forge::MODULE,
            &Op::Create {
                repo: "mine".into(),
                hash: abi::HashKind::Sha256,
            },
        )?;
        let (request, pack) = history();
        let pack = attached(net, &pack);
        for (who, repo) in [(net.me(), "mine"), (ada, "web")] {
            net.submit(
                who,
                forge::MODULE,
                &Op::Push {
                    repo: repo.into(),
                    request: request.clone(),
                    pack: Some(pack),
                },
            )?;
        }
        for (from, title) in [
            ("refactor/split-every-module", "Split every module in two"),
            ("feature/search-0", "Search the index"),
            ("release", "Release 0.2"),
        ] {
            net.submit(
                ada,
                forge::MODULE,
                &Op::ChangeOpen {
                    repo: "web".into(),
                    from: Revision::Ref(format!("refs/heads/{from}").into_bytes()),
                    into: b"refs/heads/main".to_vec(),
                    title: title.into(),
                    body: String::new(),
                    reviewers: Vec::new(),
                },
            )?;
        }
        Ok(())
    })
}

/// How many modules `src/` holds, and the lines of each.
const MODULES: usize = 30;
const LINES: usize = 400;
/// How many files `legacy/` holds on `main`; the release deletes them.
const LEGACY: usize = 52;

/// A file's path and blob.
type File = (String, Oid);

/// `web`'s push: `main` (a long `src/lib.rs`, `MODULES` modules, `LEGACY`
/// old files, two binary fonts and a lock file too large to show), 90
/// branches one commit off it, twelve tags, a branch rewriting every module
/// and adding as many again, and `release`: a merge commit shaped like a
/// real release (192 files added, 52 deleted, 15 modified, the fonts moved).
fn attached(net: &live::Network, pack: &[u8]) -> abi::BlobId {
    let id = guest::blob_id(abi::HashKind::Sha256, forge::PACK_KIND, pack).unwrap();
    let blob = guest::Blob {
        kind: forge::PACK_KIND.into(),
        body: pack.to_vec(),
    };
    net.host(forge::MODULE).borrow_mut().blobs.insert(id, blob);
    id
}

fn history() -> (Vec<u8>, Vec<u8>) {
    let mut store = MemoryObjects::new(Hash::Sha256);
    let s = &mut store;
    let readme = blob(s, b"# web\n\nThe site, its search and its docs.\n");
    let lib = blob(s, &source("lib", 1, 1200));
    let modules: Vec<File> = (0..MODULES)
        .map(|n| {
            (
                format!("src/mod_{n:02}.rs"),
                blob(s, &source("m", n, LINES)),
            )
        })
        .collect();
    let legacy: Vec<File> = (0..LEGACY)
        .map(|n| {
            (
                format!("legacy/d{}/old_{n:02}.rs", n % 6),
                blob(s, &source("old", n, LINES)),
            )
        })
        .collect();
    let fonts: Vec<(String, Oid)> = ["Sans", "Mono"]
        .iter()
        .map(|name| (format!("{name}.ttf"), blob(s, &binary(name.len()))))
        .collect();
    let lock = blob(s, &source("lock", 0, 3600));
    let base = |fonts_dir: &str, lock: Oid, modules: &[File]| {
        let mut files = vec![
            ("README.md".to_owned(), readme),
            ("src/lib.rs".to_owned(), lib),
            ("Cargo.lock".to_owned(), lock),
        ];
        files.extend(modules.iter().cloned());
        files.extend(
            fonts
                .iter()
                .map(|(name, id)| (format!("{fonts_dir}/{name}"), *id)),
        );
        files
    };
    let mut main_files = base("assets/fonts", lock, &modules);
    main_files.extend(legacy.iter().cloned());
    let main_tree = tree_of(s, &main_files);
    let main = commit(s, main_tree, &[], "Start the site");

    let zero = Hash::Sha256.zero();
    let mut commands = vec![(zero, main, "refs/heads/main".to_owned())];
    let words = [
        "search", "login", "cache", "theme", "ci", "docs", "deps", "layout", "perf",
    ];
    for n in 0..90 {
        let name = match n % 3 {
            0 => format!("feature/{}-{n}", words[n % words.len()]),
            1 => format!("fix/{}-{n}", words[n % words.len()]),
            _ => format!(
                "people/ada/a-rather-long-branch-name-about-the-{}-work-{n}",
                words[n % words.len()]
            ),
        };
        let readme = blob(
            s,
            format!("# web\n\nThe site, its search and its docs.\n\n{name}\n").as_bytes(),
        );
        let mut files = main_files.clone();
        files[0] = ("README.md".to_owned(), readme);
        let tree = tree_of(s, &files);
        let tip = commit(s, tree, &[main], &name);
        commands.push((zero, tip, format!("refs/heads/{name}")));
    }
    for n in 1..=12 {
        commands.push((zero, main, format!("refs/tags/v0.{n}.0")));
    }

    let mut split = Vec::new();
    for n in 0..MODULES {
        split.push((
            format!("src/mod_{n:02}.rs"),
            blob(s, &source("a", n, LINES)),
        ));
        split.push((
            format!("src/mod_{n:02}_b.rs"),
            blob(s, &source("b", n, LINES)),
        ));
    }
    let mut files = base("assets/fonts", lock, &split);
    files.extend(legacy.iter().cloned());
    let tree = tree_of(s, &files);
    let tip = commit(s, tree, &[main], "Split every module in two");
    commands.push((
        zero,
        tip,
        "refs/heads/refactor/split-every-module".to_owned(),
    ));

    // the release: every module touched in places (the first one rewritten
    // line by line, past any search), the lock file regenerated, the fonts
    // moved, `legacy/` gone, and 192 files added across 48 directories,
    // 15 of them over a thousand lines
    let mut touched = Vec::new();
    for (n, (path, _)) in modules.iter().enumerate() {
        let text = match n {
            0 => interleaved("m", n, LINES),
            n if n < 15 => edited("m", n, LINES),
            _ => source("m", n, LINES),
        };
        touched.push((path.clone(), blob(s, &text)));
    }
    let lock = blob(s, &edited("lock", 0, 3600));
    let mut files = base("app/assets/fonts", lock, &touched);
    let areas = [
        "ax", "render", "shell", "runtime", "editor", "backend", "tree", "ops",
    ];
    let parts = ["audit", "layers", "entities", "inputs", "seat", "tests"];
    for i in 0..192 {
        let lines = if i < 15 {
            1100 + i * 75
        } else {
            50 + (i * 37) % 500
        };
        let path = format!("src/{}/{}/new_{i:03}.rs", areas[i % 8], parts[(i / 8) % 6]);
        files.push((path, blob(s, &source("new", i, lines))));
    }
    let side = commit(s, main_tree, &[main], "Branch off for the release");
    let tree = tree_of(s, &files);
    let tip = commit(
        s,
        tree,
        &[main, side],
        "Merge pull request #255 from web/dev",
    );
    commands.push((zero, tip, "refs/heads/release".to_owned()));

    let ids: Vec<Oid> = store.ids().copied().collect();
    let objects: Vec<_> = ids
        .iter()
        .map(|id| store.get(id).unwrap().unwrap())
        .collect();
    let pack = pack::write(objects, Hash::Sha256).unwrap();
    let mut request = Vec::new();
    for (index, (old, new, name)) in commands.iter().enumerate() {
        let mut line = format!("{old} {new} {name}").into_bytes();
        if index == 0 {
            line.extend_from_slice(b"\0report-status side-band-64k");
        }
        pktline::push(&mut request, &line);
    }
    request.extend_from_slice(pktline::flush());
    (request, pack)
}

/// `lines` lines of Rust-looking source, distinct per `prefix` and `n`.
fn source(prefix: &str, n: usize, lines: usize) -> Vec<u8> {
    let mut out = format!("//! {prefix} {n}: generated for the live example.\n\n");
    for line in 0..lines {
        out += &format!(
            "pub fn {prefix}_{n}_{line}(x: u64) -> u64 {{ x.wrapping_mul({line}).rotate_left({n}) ^ 0x{:x} }}\n",
            line * 7919 + n
        );
    }
    out.into_bytes()
}

/// [`source`] with every fortieth line changed: a file touched in places.
fn edited(prefix: &str, n: usize, lines: usize) -> Vec<u8> {
    let text = String::from_utf8(source(prefix, n, lines)).unwrap();
    text.lines()
        .enumerate()
        .map(|(i, line)| match i % 40 {
            7 => format!("{line} // edited\n"),
            _ => format!("{line}\n"),
        })
        .collect::<String>()
        .into_bytes()
}

/// [`source`] with a line inserted after every line: an edit no search
/// under a founding's cost finds.
fn interleaved(prefix: &str, n: usize, lines: usize) -> Vec<u8> {
    let text = String::from_utf8(source(prefix, n, lines)).unwrap();
    text.lines()
        .map(|line| format!("{line}\n// between\n"))
        .collect::<String>()
        .into_bytes()
}

/// A megabyte that is not text, distinct per `seed`.
fn binary(seed: usize) -> Vec<u8> {
    let mut state = seed as u32 + 1;
    (0..1 << 20)
        .map(|_| {
            state = state.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
            (state >> 24) as u8
        })
        .collect()
}

fn blob(store: &mut MemoryObjects, content: &[u8]) -> Oid {
    store.put(Kind::Blob, content).unwrap()
}

/// The tree holding `files`, each at its slash-separated path.
fn tree_of(store: &mut MemoryObjects, files: &[File]) -> Oid {
    let mut dirs: std::collections::BTreeMap<&str, Vec<File>> = Default::default();
    let mut entries = Vec::new();
    for (path, id) in files {
        match path.split_once('/') {
            Some((dir, rest)) => dirs.entry(dir).or_default().push((rest.to_owned(), *id)),
            None => entries.push(entry(Mode::Regular, path, *id)),
        }
    }
    for (dir, files) in dirs {
        let id = tree_of(store, &files);
        entries.push(entry(Mode::Directory, dir, id));
    }
    entries.sort_by(|a, b| a.name.cmp(&b.name));
    tree(store, entries)
}

fn entry(mode: Mode, name: &str, id: Oid) -> TreeEntry {
    TreeEntry {
        mode,
        name: name.as_bytes().to_vec(),
        id,
    }
}

fn tree(store: &mut MemoryObjects, entries: Vec<TreeEntry>) -> Oid {
    store
        .put(Kind::Tree, &Tree { entries }.serialize())
        .unwrap()
}

fn commit(store: &mut MemoryObjects, tree: Oid, parents: &[Oid], message: &str) -> Oid {
    let ada = Signature {
        name: b"Ada".to_vec(),
        email: b"ada@example.com".to_vec(),
        time: 1_791_000_000,
        offset_minutes: 0,
    };
    let commit = Commit {
        tree,
        parents: parents.to_vec(),
        author: ada.clone(),
        committer: ada,
        extra: Vec::new(),
        message: format!("{message}\n").into_bytes(),
    };
    store.put(Kind::Commit, &commit.serialize()).unwrap()
}
