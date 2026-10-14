// Walk tests on a hand-built DAG: ancestry, wants minus haves, reachable objects, path lookup.

#[path = "common/mod.rs"]
mod common;

use common::{blob, commit, file_tree, tree};
use gitcore::walk::{Listing, Verdict, closure, commits, is_ancestor, tree_at_path};
use gitcore::{Error, Hash, MemoryObjects, Mode, Oid};
use std::collections::BTreeSet;

struct Dag {
    store: MemoryObjects,
    root: Oid,
    a: Oid,
    b: Oid,
    c: Oid,
    merge: Oid,
    other: Oid,
    trees: Vec<Oid>,
}

fn dag() -> Dag {
    let mut store = MemoryObjects::new(Hash::Sha1);
    let t0 = file_tree(&mut store, &[("f", "0\n")]);
    let t1 = file_tree(&mut store, &[("f", "1\n")]);
    let t2 = file_tree(&mut store, &[("f", "2\n")]);
    let t3 = file_tree(&mut store, &[("f", "3\n"), ("g", "g\n")]);
    let root = commit(&mut store, t0, &[], 100, "root");
    let a = commit(&mut store, t1, &[root], 200, "a");
    let b = commit(&mut store, t2, &[root], 200, "b");
    let c = commit(&mut store, t3, &[a], 300, "c");
    let merge = commit(&mut store, t3, &[c, b], 400, "merge");
    let other = commit(&mut store, t0, &[], 50, "other");
    Dag {
        store,
        root,
        a,
        b,
        c,
        merge,
        other,
        trees: vec![t0, t1, t2, t3],
    }
}

#[test]
fn ancestor_verdicts() {
    let d = dag();
    assert_eq!(
        is_ancestor(&d.store, &d.root, &d.merge, 100).unwrap(),
        Verdict::Yes
    );
    assert_eq!(
        is_ancestor(&d.store, &d.b, &d.merge, 100).unwrap(),
        Verdict::Yes
    );
    assert_eq!(
        is_ancestor(&d.store, &d.merge, &d.merge, 100).unwrap(),
        Verdict::Yes
    );
    assert_eq!(
        is_ancestor(&d.store, &d.merge, &d.root, 100).unwrap(),
        Verdict::No
    );
    assert_eq!(is_ancestor(&d.store, &d.b, &d.c, 100).unwrap(), Verdict::No);
    assert_eq!(
        is_ancestor(&d.store, &d.other, &d.merge, 100).unwrap(),
        Verdict::No
    );
    assert_eq!(
        is_ancestor(&d.store, &d.root, &d.merge, 2).unwrap(),
        Verdict::CapReached
    );
    let missing = Oid::from_hex(Hash::Sha1, "1111111111111111111111111111111111111111").unwrap();
    assert_eq!(
        is_ancestor(&d.store, &d.root, &missing, 10),
        Err(Error::MissingObject(missing))
    );
}

#[test]
fn commits_from_tips_excluding_haves() {
    let d = dag();
    let all = commits(&d.store, &[d.merge], &[], 100).unwrap();
    assert_eq!(all.len(), 5);
    assert_eq!(all[0], d.merge);
    assert_eq!(all[1], d.c);
    assert_eq!(all[4], d.root);
    let mut same_time = [d.a, d.b];
    same_time.sort();
    assert_eq!(&all[2..4], &same_time);

    let since_a = commits(&d.store, &[d.merge], &[d.a], 100).unwrap();
    assert_eq!(since_a, vec![d.merge, d.c, d.b]);

    let since_c_and_b = commits(&d.store, &[d.merge], &[d.c, d.b], 100).unwrap();
    assert_eq!(since_c_and_b, vec![d.merge]);

    let nothing = commits(&d.store, &[d.merge], &[d.merge], 100).unwrap();
    assert!(nothing.is_empty());

    let two_tips = commits(&d.store, &[d.c, d.other], &[d.root], 100).unwrap();
    assert_eq!(two_tips, vec![d.c, d.a, d.other]);

    let unknown_have =
        Oid::from_hex(Hash::Sha1, "2222222222222222222222222222222222222222").unwrap();
    assert_eq!(
        commits(&d.store, &[d.c], &[unknown_have], 100).unwrap(),
        vec![d.c, d.a, d.root]
    );
    assert_eq!(
        commits(&d.store, &[d.merge], &[], 3),
        Err(Error::CapReached)
    );
}

#[test]
fn commits_handles_clock_skew_between_have_and_want() {
    let mut store = MemoryObjects::new(Hash::Sha1);
    let t = file_tree(&mut store, &[("f", "x\n")]);
    let old = commit(&mut store, t, &[], 500, "old but dated late");
    let mid = commit(&mut store, t, &[old], 100, "mid");
    let new = commit(&mut store, t, &[mid], 200, "new");
    assert_eq!(
        commits(&store, &[new], &[old], 100).unwrap(),
        vec![new, mid]
    );
    assert_eq!(commits(&store, &[new], &[mid], 100).unwrap(), vec![new]);
}

fn listed_in(store: &MemoryObjects, wants: &[Oid], haves: &[Oid]) -> BTreeSet<Oid> {
    let mut listing = Listing::of(wants, haves);
    while let Some(walk) = listing.pending() {
        listing.receive(closure(store, walk).unwrap());
    }
    listing
        .into_listed()
        .into_iter()
        .map(|item| item.id)
        .collect()
}

fn listed(d: &Dag, wants: &[Oid], haves: &[Oid]) -> BTreeSet<Oid> {
    listed_in(&d.store, wants, haves)
}

fn blobs(contents: &[&[u8]]) -> BTreeSet<Oid> {
    let mut scratch = MemoryObjects::new(Hash::Sha1);
    contents
        .iter()
        .map(|body| blob(&mut scratch, body))
        .collect()
}

#[test]
fn a_closure_lists_a_history_once_whatever_its_merges() {
    let d = dag();
    let mut expected: BTreeSet<Oid> = [d.root, d.a, d.b, d.c, d.merge].into_iter().collect();
    expected.extend(d.trees.iter().copied());
    expected.extend(blobs(&[b"0\n", b"1\n", b"2\n", b"3\n", b"g\n"]));
    assert_eq!(listed(&d, &[d.merge], &[]), expected);
}

#[test]
fn a_closure_stops_at_the_commits_the_client_holds() {
    let d = dag();
    let mut expected: BTreeSet<Oid> = [d.merge, d.b].into_iter().collect();
    expected.insert(d.trees[2]);
    expected.extend(blobs(&[b"2\n"]));
    assert_eq!(listed(&d, &[d.merge], &[d.c, d.root]), expected);
    let only_the_merge: BTreeSet<Oid> = [d.merge].into_iter().collect();
    assert_eq!(listed(&d, &[d.merge], &[d.c, d.b]), only_the_merge);
}

#[test]
fn a_closure_stops_at_what_the_client_holds_below_its_haves() {
    let mut store = MemoryObjects::new(Hash::Sha1);
    let first = file_tree(&mut store, &[("f", "0\n")]);
    let mut main = vec![commit(&mut store, first, &[], 100, "root")];
    for n in 1..10 {
        let tree = file_tree(&mut store, &[("f", &format!("{n}\n"))]);
        let parent = main[n - 1];
        main.push(commit(&mut store, tree, &[parent], 100 + n as i64, "main"));
    }
    let side_tree = file_tree(&mut store, &[("f", "1\n"), ("side", "s\n")]);
    let side = commit(&mut store, side_tree, &[main[1]], 500, "side");
    let mut expected: BTreeSet<Oid> = [side, side_tree].into_iter().collect();
    expected.extend(blobs(&[b"s\n"]));
    assert_eq!(listed_in(&store, &[side], &[main[9]]), expected);

    let merged = commit(&mut store, side_tree, &[main[9], side], 600, "merge");
    expected.insert(merged);
    assert_eq!(listed_in(&store, &[merged], &[main[9]]), expected);
}

#[test]
fn nested_reachability_and_path_lookup() {
    let mut store = MemoryObjects::new(Hash::Sha1);
    let leaf = blob(&mut store, b"leaf\n");
    let inner = tree(&mut store, &[("leaf.txt", Mode::Regular, leaf)]);
    let gitlink = Oid::from_hex(Hash::Sha1, "3333333333333333333333333333333333333333").unwrap();
    let root = tree(
        &mut store,
        &[
            ("dir", Mode::Directory, inner),
            ("sub", Mode::Gitlink, gitlink),
        ],
    );
    let head = commit(&mut store, root, &[], 1, "head");
    assert_eq!(
        listed_in(&store, &[head], &[]),
        [head, root, inner, leaf].into_iter().collect()
    );

    let found = tree_at_path(&store, &root, b"dir/leaf.txt")
        .unwrap()
        .unwrap();
    assert_eq!(found.id, leaf);
    assert_eq!(found.mode, Mode::Regular);
    assert_eq!(
        tree_at_path(&store, &root, b"dir").unwrap().unwrap().id,
        inner
    );
    assert!(tree_at_path(&store, &root, b"dir/nope").unwrap().is_none());
    assert!(
        tree_at_path(&store, &root, b"dir/leaf.txt/deeper")
            .unwrap()
            .is_none()
    );
    assert!(tree_at_path(&store, &root, b"").unwrap().is_none());
    assert!(
        tree_at_path(&store, &root, b"dir//leaf.txt")
            .unwrap()
            .is_none()
    );
}
