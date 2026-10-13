//! `cargo run -p forge-view --example live`: Forge in the real app over a
//! fake node with a repository and the people who may push to it. `web`
//! holds a history: a long branch and tag list, a long source file, and two
//! open changes, one of them a large diff; `docs` is empty.
use ducktape_view_guest::live;
use forge::{Bounds, Op, Revision};
use gitcore::wire::pktline;
use gitcore::{
    Commit, Hash, Kind, MemoryObjects, Mode, Objects, Oid, Signature, Tree, TreeEntry, pack,
};

fn main() {
    live::run("forge", |net| {
        net.seat::<chat::Chat>(chat::MODULE);
        net.seat::<forge::Forge>(forge::MODULE);
        net.init(
            forge::MODULE,
            &Bounds {
                max_objects: 10_000,
                max_delta_depth: 64,
                max_object_size: 1 << 20,
                push_walk: 1000,
                fetch_walk: 1000,
                merge_cost: 100_000,
                page_size: 128,
                log_walk: 1000,
                tree_walk: 256,
                diff_bytes: 4 << 20,
                blob_bytes: 1 << 20,
                record_bytes: 64 << 10,
            },
        )?;
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
        net.submit(
            ada,
            forge::MODULE,
            &Op::Push {
                repo: "web".into(),
                request: history(),
            },
        )?;
        for (from, title) in [
            ("refactor/split-every-module", "Split every module in two"),
            ("feature/search-0", "Search the index"),
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

/// `web`'s push: `main` (a long `src/lib.rs` and `MODULES` modules), 90
/// branches one commit off it, twelve tags, and a branch rewriting every
/// module and adding as many again.
fn history() -> Vec<u8> {
    let mut store = MemoryObjects::new(Hash::Sha256);
    let s = &mut store;
    let readme = blob(s, b"# web\n\nThe site, its search and its docs.\n");
    let lib = blob(s, &source("lib", 1, 1200));
    let modules: Vec<(String, Oid)> = (0..MODULES)
        .map(|n| (format!("mod_{n:02}.rs"), blob(s, &source("m", n, LINES))))
        .collect();
    let main_tree = root(s, readme, lib, &modules);
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
        let tree = root(s, readme, lib, &modules);
        let tip = commit(s, tree, &[main], &name);
        commands.push((zero, tip, format!("refs/heads/{name}")));
    }
    for n in 1..=12 {
        commands.push((zero, main, format!("refs/tags/v0.{n}.0")));
    }

    let mut split = Vec::new();
    for n in 0..MODULES {
        split.push((format!("mod_{n:02}.rs"), blob(s, &source("a", n, LINES))));
        split.push((format!("mod_{n:02}_b.rs"), blob(s, &source("b", n, LINES))));
    }
    let tree = root(s, readme, lib, &split);
    let tip = commit(s, tree, &[main], "Split every module in two");
    commands.push((
        zero,
        tip,
        "refs/heads/refactor/split-every-module".to_owned(),
    ));

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
    request.extend_from_slice(&pack);
    request
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

fn blob(store: &mut MemoryObjects, content: &[u8]) -> Oid {
    store.put(Kind::Blob, content).unwrap()
}

/// The root tree: `README.md` and `src/` holding `lib.rs` and the modules.
fn root(store: &mut MemoryObjects, readme: Oid, lib: Oid, modules: &[(String, Oid)]) -> Oid {
    let mut files = vec![("lib.rs".to_owned(), lib)];
    files.extend(modules.iter().cloned());
    files.sort();
    let src = tree(
        store,
        files
            .into_iter()
            .map(|(name, id)| entry(Mode::Regular, &name, id))
            .collect(),
    );
    tree(
        store,
        vec![
            entry(Mode::Regular, "README.md", readme),
            entry(Mode::Directory, "src", src),
        ],
    )
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
