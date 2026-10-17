//! Homes: `/home/<name>/` is written only by the account identity names so, at the
//! commit; the chain writes there too.
use super::*;

#[test]
fn a_home_is_written_moved_out_of_and_removed_only_by_the_account_it_names() {
    let host = host();
    commit(
        &host,
        ada(),
        None,
        vec![put(&host, "/home/ada/a", "a"), put(&host, "/shared/s", "s")],
    )
    .unwrap();
    commit(&host, bo(), None, vec![put(&host, "/home/bo/b", "b")]).unwrap();
    let into_bo = Change::Move {
        from: "/shared/s".into(),
        to: "/home/bo/s".into(),
    };
    let out_of_bo = Change::Move {
        from: "/home/bo/b".into(),
        to: "/shared/b".into(),
    };
    let remove_bo = Change::Remove {
        path: "/home/bo/b".into(),
    };
    let link_in_bo = Change::Symlink {
        path: "/home/bo/l".into(),
        target: "/shared/s".into(),
    };
    let mkdir_in_bo = Change::Mkdir {
        path: "/home/bo/d".into(),
    };
    for change in [into_bo, out_of_bo, remove_bo, link_in_bo, mkdir_in_bo] {
        let op = Op::Commit {
            base: None,
            message: String::new(),
            changes: vec![change.clone()],
        };
        assert_eq!(
            refused(&host, ada(), op).code,
            code::UNAUTHORIZED,
            "{change:?}"
        );
    }
    let moved = Change::Move {
        from: "/home/ada/a".into(),
        to: "/shared/a".into(),
    };
    commit(&host, ada(), head(&host), vec![moved]).unwrap();
    assert_eq!(text(&host, "/shared/a", None).unwrap(), "a");
    commit(
        &host,
        bo(),
        head(&host),
        vec![Change::Remove {
            path: "/home/bo/b".into(),
        }],
    )
    .unwrap();
    commit(
        &host,
        ada().root(),
        None,
        vec![put(&host, "/home/bo/c", "c")],
    )
    .unwrap();
    assert_eq!(text(&host, "/home/bo/c", None).unwrap(), "c");
}

#[test]
fn a_home_follows_the_name_identity_holds_at_the_commit() {
    let host = host();
    commit(&host, ada(), None, vec![put(&host, "/home/ada/a", "1")]).unwrap();
    host.identity(vec![profile(1, "ada lovelace"), profile(2, "bo")]);
    let op = Op::Commit {
        base: None,
        message: String::new(),
        changes: vec![put(&host, "/home/ada/a", "2")],
    };
    assert_eq!(refused(&host, ada(), op).code, code::UNAUTHORIZED);
    commit(
        &host,
        ada(),
        None,
        vec![put(&host, "/home/ada lovelace/a", "2")],
    )
    .unwrap();
    assert_eq!(text(&host, "/home/ada/a", None).unwrap(), "1");
}
