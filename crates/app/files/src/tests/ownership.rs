//! Write lists: a path's creator, the list its creator sets, the nearest list at or above
//! a path deciding who writes it, and the list moving and going with its path.
use describe::Value;

use super::*;

fn cy() -> Env {
    MockHost::env(PROGRAM).signed(vec![3; 32], Some(3))
}

fn hosted() -> MockHost {
    let host = MockHost::default();
    host.identity(vec![profile(1, "ada"), profile(2, "bo"), profile(3, "cy")]);
    host
}

fn set(path: &str, write: Option<Ownership>) -> Change {
    Change::SetOwnership {
        path: path.into(),
        write,
    }
}

fn change(host: &MockHost, env: Env, changes: Vec<Change>) -> Result<BlobId, Error> {
    commit(host, env, head(host), changes)
}

fn refused_with(host: &MockHost, env: Env, changes: Vec<Change>) -> String {
    let op = Op::Commit {
        base: head(host),
        message: String::new(),
        changes,
    };
    refused(host, env, op).code
}

fn owned(host: &MockHost, path: &str, at: Option<BlobId>) -> PathOwnership {
    let asked = Query::Ownership {
        path: path.into(),
        at,
    };
    match ask(host, asked).unwrap() {
        Reply::Ownership(ownership) => ownership,
        other => panic!("{other:?}"),
    }
}

#[test]
fn the_first_account_to_make_a_path_and_the_directories_it_makes_is_their_creator() {
    let host = hosted();
    change(&host, ada(), vec![put(&host, "/shared/d/f", "a")]).unwrap();
    change(
        &host,
        bo(),
        vec![
            put(&host, "/shared/d/f", "b"),
            put(&host, "/shared/d/g", "b"),
        ],
    )
    .unwrap();
    assert_eq!(owned(&host, "/shared/d/f", None).creator, Some(1));
    assert_eq!(owned(&host, "/shared/d", None).creator, Some(1));
    assert_eq!(owned(&host, "/shared/d/g", None).creator, Some(2));
    let root = owned(&host, "/shared", None);
    assert_eq!((root.creator, root.write), (None, None));
    assert_eq!(root.writers, Writers::Chain);
    assert_eq!(owned(&host, "/shared/d/f", None).writers, Writers::Shared);
    assert_eq!(
        owned(&host, "/home/ada/x", None).writers,
        Writers::Home("ada".into())
    );
    change(&host, ada().root(), vec![put(&host, "/shared/c", "c")]).unwrap();
    assert_eq!(owned(&host, "/shared/c", None).creator, None);
}

#[test]
fn owner_allows_its_creator_accounts_the_listed_and_anyone_every_account() {
    let host = hosted();
    change(
        &host,
        ada(),
        vec![
            put(&host, "/shared/d/f", "a"),
            set("/shared/d", Some(Ownership::Owner)),
        ],
    )
    .unwrap();
    assert_eq!(
        owned(&host, "/shared/d/f", None).writers,
        Writers::Listed {
            path: "/shared/d".into(),
            creator: Some(1),
            write: Ownership::Owner,
        }
    );
    assert_eq!(
        refused_with(&host, bo(), vec![put(&host, "/shared/d/f", "b")]),
        code::UNAUTHORIZED
    );
    change(&host, ada(), vec![put(&host, "/shared/d/f", "a2")]).unwrap();

    let listed = Ownership::Accounts(vec![2, 1, 2]);
    change(&host, ada(), vec![set("/shared/d", Some(listed))]).unwrap();
    assert_eq!(
        owned(&host, "/shared/d", None).write,
        Some(Ownership::Accounts(vec![2])),
        "once each, never its creator"
    );
    change(&host, bo(), vec![put(&host, "/shared/d/f", "b")]).unwrap();
    assert_eq!(
        refused_with(&host, cy(), vec![put(&host, "/shared/d/f", "c")]),
        code::UNAUTHORIZED
    );

    change(
        &host,
        ada(),
        vec![set("/shared/d", Some(Ownership::Anyone))],
    )
    .unwrap();
    change(&host, cy(), vec![put(&host, "/shared/d/f", "c")]).unwrap();
    assert_eq!(text(&host, "/shared/d/f", None).unwrap(), "c");

    change(&host, ada(), vec![set("/shared/d", None)]).unwrap();
    let cleared = owned(&host, "/shared/d", None);
    assert_eq!((cleared.creator, cleared.write), (Some(1), None));
    assert_eq!(cleared.writers, Writers::Shared);
}

#[test]
fn every_write_to_a_listed_path_is_checked_and_a_refusal_leaves_nothing() {
    let host = hosted();
    change(
        &host,
        ada(),
        vec![
            put(&host, "/shared/d/f", "a"),
            put(&host, "/shared/s", "s"),
            set("/shared/d", Some(Ownership::Owner)),
        ],
    )
    .unwrap();
    let before = head(&host);
    let writes = [
        Change::Mkdir {
            path: "/shared/d/m".into(),
        },
        Change::Symlink {
            path: "/shared/d/l".into(),
            target: "/shared/s".into(),
        },
        Change::Remove {
            path: "/shared/d/f".into(),
        },
        Change::Remove {
            path: "/shared/d".into(),
        },
        Change::Move {
            from: "/shared/d/f".into(),
            to: "/shared/f".into(),
        },
        Change::Move {
            from: "/shared/s".into(),
            to: "/shared/d/s".into(),
        },
        set("/shared/d/f", Some(Ownership::Anyone)),
    ];
    for write in writes {
        assert_eq!(
            refused_with(&host, bo(), vec![write.clone()]),
            code::UNAUTHORIZED,
            "{write:?}"
        );
    }
    assert_eq!(head(&host), before);
}

#[test]
fn the_nearest_list_wins_over_a_list_higher_up_and_over_the_home_rule() {
    let host = hosted();
    change(
        &host,
        ada(),
        vec![
            put(&host, "/home/ada/proj/f", "a"),
            put(&host, "/home/ada/proj/inner/g", "a"),
            set("/home/ada/proj", Some(Ownership::Accounts(vec![2]))),
            set("/home/ada/proj/inner", Some(Ownership::Owner)),
        ],
    )
    .unwrap();
    change(&host, bo(), vec![put(&host, "/home/ada/proj/f", "b")]).unwrap();
    assert_eq!(
        refused_with(&host, bo(), vec![put(&host, "/home/ada/proj/inner/g", "b")]),
        code::UNAUTHORIZED
    );
    assert_eq!(
        refused_with(&host, bo(), vec![put(&host, "/home/ada/other", "b")]),
        code::UNAUTHORIZED
    );

    change(
        &host,
        ada(),
        vec![
            put(&host, "/shared/d/open/f", "a"),
            set("/shared/d/open", Some(Ownership::Anyone)),
            set("/shared/d", Some(Ownership::Owner)),
        ],
    )
    .unwrap();
    change(&host, cy(), vec![put(&host, "/shared/d/open/f", "c")]).unwrap();
    assert_eq!(
        refused_with(&host, cy(), vec![put(&host, "/shared/d/f", "c")]),
        code::UNAUTHORIZED
    );
}

#[test]
fn only_its_creator_sets_a_paths_list_and_a_home_folders_by_the_account_named_so() {
    let host = hosted();
    change(
        &host,
        ada(),
        vec![
            put(&host, "/shared/d/f", "a"),
            put(&host, "/home/ada/f", "a"),
        ],
    )
    .unwrap();
    change(&host, bo(), vec![put(&host, "/shared/d/g", "b")]).unwrap();
    assert_eq!(
        refused_with(&host, bo(), vec![set("/shared/d", Some(Ownership::Anyone))]),
        code::UNAUTHORIZED
    );
    change(
        &host,
        bo(),
        vec![set("/shared/d/g", Some(Ownership::Owner))],
    )
    .unwrap();
    assert_eq!(
        refused_with(&host, ada(), vec![set("/shared/d/g", None)]),
        code::UNAUTHORIZED
    );
    assert_eq!(
        refused_with(&host, ada(), vec![set("/shared/nothing", None)]),
        code::UNAUTHORIZED
    );
    assert_eq!(
        refused_with(&host, ada().root(), vec![set("/shared/nothing", None)]),
        code::NOT_FOUND
    );
    assert_eq!(
        refused_with(
            &host,
            ada(),
            vec![set("/shared/d", Some(Ownership::Accounts(vec![99])))]
        ),
        code::NOT_FOUND
    );

    assert_eq!(
        refused_with(&host, bo(), vec![set("/home/ada", Some(Ownership::Anyone))]),
        code::UNAUTHORIZED
    );
    let shared_with_bo = Some(Ownership::Accounts(vec![2]));
    change(&host, ada(), vec![set("/home/ada", shared_with_bo.clone())]).unwrap();
    let home = owned(&host, "/home/ada", None);
    assert_eq!((home.creator, home.write), (Some(1), shared_with_bo));
    change(&host, bo(), vec![put(&host, "/home/ada/from-bo", "b")]).unwrap();

    change(&host, ada().root(), vec![put(&host, "/shared/chain", "c")]).unwrap();
    assert_eq!(
        refused_with(
            &host,
            ada(),
            vec![set("/shared/chain", Some(Ownership::Owner))]
        ),
        code::UNAUTHORIZED
    );
    change(
        &host,
        ada().root(),
        vec![set("/shared/chain", Some(Ownership::Owner))],
    )
    .unwrap();
    assert_eq!(
        refused_with(&host, ada(), vec![put(&host, "/shared/chain", "a")]),
        code::UNAUTHORIZED
    );
}

#[test]
fn a_move_carries_creator_and_list_and_a_remove_clears_them() {
    let host = hosted();
    change(
        &host,
        ada(),
        vec![
            put(&host, "/shared/d/f", "a"),
            set("/shared/d", Some(Ownership::Owner)),
        ],
    )
    .unwrap();
    let locked = owned(&host, "/shared/d", None);
    let moved = Change::Move {
        from: "/shared/d".into(),
        to: "/shared/e".into(),
    };
    change(&host, ada(), vec![moved]).unwrap();
    let carried = owned(&host, "/shared/e", None);
    assert_eq!(
        (carried.creator, carried.write),
        (locked.creator, locked.write)
    );
    assert_eq!(owned(&host, "/shared/e/f", None).creator, Some(1));
    let left = owned(&host, "/shared/d", None);
    assert_eq!((left.creator, left.write), (None, None));
    assert_eq!(
        refused_with(&host, bo(), vec![put(&host, "/shared/e/f", "b")]),
        code::UNAUTHORIZED
    );
    change(&host, bo(), vec![put(&host, "/shared/d/f", "b")]).unwrap();
    assert_eq!(owned(&host, "/shared/d", None).creator, Some(2));

    let removed = Change::Remove {
        path: "/shared/e".into(),
    };
    change(&host, ada(), vec![removed]).unwrap();
    let gone = owned(&host, "/shared/e", None);
    assert_eq!((gone.creator, gone.write), (None, None));
    change(&host, bo(), vec![put(&host, "/shared/e/f", "b")]).unwrap();
    assert_eq!(owned(&host, "/shared/e", None).creator, Some(2));
}

#[test]
fn the_chain_writes_anywhere_whatever_a_list_says() {
    let host = hosted();
    change(
        &host,
        ada(),
        vec![
            put(&host, "/shared/d/f", "a"),
            set("/shared/d", Some(Ownership::Owner)),
        ],
    )
    .unwrap();
    change(
        &host,
        ada().root(),
        vec![put(&host, "/shared/d/f", "chain")],
    )
    .unwrap();
    change(
        &host,
        ada().root(),
        vec![set("/shared/d", Some(Ownership::Accounts(vec![3])))],
    )
    .unwrap();
    assert_eq!(text(&host, "/shared/d/f", None).unwrap(), "chain");
    change(&host, cy(), vec![put(&host, "/shared/d/f", "c")]).unwrap();
}

#[test]
fn a_list_keeps_allowing_the_account_numbers_it_holds_whatever_they_are_named() {
    let host = hosted();
    change(
        &host,
        ada(),
        vec![
            put(&host, "/shared/d/f", "a"),
            set("/shared/d", Some(Ownership::Accounts(vec![2]))),
        ],
    )
    .unwrap();
    host.identity(vec![profile(1, "ada"), profile(2, "cy"), profile(3, "bo")]);
    change(&host, bo(), vec![put(&host, "/shared/d/f", "account 2")]).unwrap();
    assert_eq!(
        refused_with(&host, cy(), vec![put(&host, "/shared/d/f", "account 3")]),
        code::UNAUTHORIZED
    );
}

#[test]
fn a_list_is_read_at_the_snapshot_asked() {
    let host = hosted();
    let open = change(&host, ada(), vec![put(&host, "/shared/d/f", "a")]).unwrap();
    change(&host, ada(), vec![set("/shared/d", Some(Ownership::Owner))]).unwrap();
    assert_eq!(owned(&host, "/shared/d", Some(open)).write, None);
    assert_eq!(owned(&host, "/shared/d", Some(open)).snapshot, Some(open));
    assert_eq!(
        owned(&host, "/shared/d", None).write,
        Some(Ownership::Owner)
    );
}

#[test]
fn a_commit_that_sets_a_list_says_so_to_whoever_signs_it() {
    let host = host();
    let op = Op::Commit {
        base: None,
        message: "lock".into(),
        changes: vec![
            put(&host, "/shared/d/f", "a"),
            set("/shared/d", Some(Ownership::Accounts(vec![2]))),
        ],
    };
    let said = describe(&op);
    let lists: Vec<_> = said
        .fields
        .iter()
        .filter(|field| field.label == "write list")
        .map(|field| field.value.clone())
        .collect();
    assert_eq!(
        lists,
        [Value::List(vec![
            Value::text("/shared/d"),
            Value::List(vec![Value::text("its creator"), Value::Account(2)]),
        ])]
    );
}
