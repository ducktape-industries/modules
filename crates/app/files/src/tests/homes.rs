//! Homes: `/home/<name>/` is open to anyone until the account identity names so sets a
//! write list on it; the folder itself is the chain's, as `/home` and `/shared` are.
use super::*;

fn set(path: &str, write: Option<Ownership>) -> Change {
    Change::SetOwnership {
        path: path.into(),
        write,
    }
}

#[test]
fn another_account_writes_into_a_home_with_no_list_but_not_the_home_folder_itself() {
    let host = host();
    commit(
        &host,
        ada(),
        None,
        vec![put(&host, "/home/ada/a", "a"), put(&host, "/shared/s", "s")],
    )
    .unwrap();
    commit(&host, bo(), None, vec![put(&host, "/home/bo/b", "b")]).unwrap();
    let writes = [
        Change::Move {
            from: "/shared/s".into(),
            to: "/home/bo/s".into(),
        },
        Change::Move {
            from: "/home/bo/b".into(),
            to: "/shared/b".into(),
        },
        Change::Symlink {
            path: "/home/bo/l".into(),
            target: "/shared/b".into(),
        },
        Change::Mkdir {
            path: "/home/bo/d".into(),
        },
        Change::Remove {
            path: "/home/bo/d".into(),
        },
    ];
    for change in writes {
        commit(&host, ada(), head(&host), vec![change.clone()])
            .unwrap_or_else(|refusal| panic!("{change:?}: {refusal:?}"));
    }
    assert_eq!(text(&host, "/home/bo/s", None).unwrap(), "s");
    assert_eq!(text(&host, "/shared/b", None).unwrap(), "b");
    let folders = [
        Change::Remove {
            path: "/home/bo".into(),
        },
        Change::Move {
            from: "/home/bo".into(),
            to: "/shared/bo".into(),
        },
        Change::Remove {
            path: "/home".into(),
        },
        Change::Remove {
            path: "/shared".into(),
        },
        Change::Mkdir {
            path: "/home/cy".into(),
        },
    ];
    for change in folders {
        let op = Op::Commit {
            base: head(&host),
            message: String::new(),
            changes: vec![change.clone()],
        };
        assert_eq!(
            refused(&host, bo(), op).code,
            code::UNAUTHORIZED,
            "{change:?}"
        );
    }
}

#[test]
fn a_home_closes_when_the_account_named_so_lists_it() {
    let host = host();
    commit(
        &host,
        bo(),
        None,
        vec![put(&host, "/home/ada/from-bo", "b")],
    )
    .unwrap();
    let op = Op::Commit {
        base: head(&host),
        message: String::new(),
        changes: vec![set("/home/ada", Some(Ownership::Owner))],
    };
    assert_eq!(refused(&host, bo(), op).code, code::UNAUTHORIZED);
    commit(
        &host,
        ada(),
        head(&host),
        vec![set("/home/ada", Some(Ownership::Owner))],
    )
    .unwrap();
    let op = Op::Commit {
        base: head(&host),
        message: String::new(),
        changes: vec![put(&host, "/home/ada/from-bo", "b2")],
    };
    assert_eq!(refused(&host, bo(), op).code, code::UNAUTHORIZED);
    commit(
        &host,
        ada(),
        head(&host),
        vec![put(&host, "/home/ada/from-bo", "a")],
    )
    .unwrap();
    commit(
        &host,
        ada().root(),
        head(&host),
        vec![put(&host, "/home/ada/c", "c")],
    )
    .unwrap();
    assert_eq!(text(&host, "/home/ada/c", None).unwrap(), "c");
}

#[test]
fn a_home_folder_list_follows_the_name_identity_holds_at_the_commit() {
    let host = host();
    host.identity(vec![profile(1, "ada lovelace"), profile(2, "bo")]);
    commit(&host, ada(), None, vec![put(&host, "/home/ada/a", "1")]).unwrap();
    let op = Op::Commit {
        base: head(&host),
        message: String::new(),
        changes: vec![set("/home/ada", Some(Ownership::Owner))],
    };
    assert_eq!(refused(&host, ada(), op).code, code::UNAUTHORIZED);
    commit(
        &host,
        ada(),
        head(&host),
        vec![
            put(&host, "/home/ada lovelace/a", "2"),
            set("/home/ada lovelace", Some(Ownership::Owner)),
        ],
    )
    .unwrap();
}
