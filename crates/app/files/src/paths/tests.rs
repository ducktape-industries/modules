use super::*;

#[test]
fn a_path_is_absolute_and_composed() {
    assert_eq!(segments("/").unwrap(), Vec::<String>::new());
    assert_eq!(segments("/shared/a").unwrap(), ["shared", "a"]);
    for bad in [
        "shared",
        "/shared//a",
        "/shared/a/",
        "/shared/./a",
        "/shared/../a",
        "/a\0b",
        "/cafe\u{301}",
    ] {
        assert_eq!(
            segments(bad).unwrap_err().code,
            ::program::code::INVALID_INPUT,
            "{bad:?}"
        );
    }
    assert_eq!(
        change_path("/").unwrap_err().code,
        ::program::code::INVALID_INPUT
    );
}

fn named(number: AccountNumber, label: &str) -> Writer {
    Writer::Account {
        number,
        label: Some(label.to_owned()),
    }
}

fn unlisted(path: &str) -> (Vec<String>, Vec<Option<Access>>) {
    let segments = change_path(path).unwrap();
    let along = vec![None; segments.len()];
    (segments, along)
}

fn write(writer: &Writer, path: &str) -> Result<(), Error> {
    let (segments, along) = unlisted(path);
    writable(writer, &segments, &along, Act::Write)
}

fn listed_at(
    path: &str,
    depth: usize,
    creator: Option<u64>,
    write: Ownership,
) -> Vec<Option<Access>> {
    let (_, mut along) = unlisted(path);
    along[depth] = Some(Access {
        creator,
        write: Some(write),
    });
    along
}

#[test]
fn an_account_writes_under_its_own_home_and_shared_and_the_chain_anywhere() {
    let ada = named(1, "ada");
    for ok in ["/home/ada/notes", "/home/ada/x/y", "/shared/a"] {
        assert!(write(&ada, ok).is_ok(), "{ok}");
    }
    for refused in [
        "/home",
        "/home/ada",
        "/home/bo/x",
        "/home/Ada/x",
        "/shared",
        "/etc/passwd",
        "/x",
    ] {
        assert_eq!(
            write(&ada, refused).unwrap_err().code,
            ::program::code::UNAUTHORIZED,
            "{refused}"
        );
    }
    assert!(write(&Writer::Chain, "/etc/passwd").is_ok());
    assert!(write(&Writer::Chain, "/home/bo/x").is_ok());
}

#[test]
fn another_home_is_unauthorized_and_an_unnamed_account_has_none() {
    let refused = write(&named(1, "ada"), "/home/bo/agents/scout.md").unwrap_err();
    assert_eq!(refused.code, ::program::code::UNAUTHORIZED);
    let unnamed = Writer::Account {
        number: 3,
        label: None,
    };
    assert_eq!(
        write(&unnamed, "/home/bo/x").unwrap_err().code,
        ::program::code::UNAUTHORIZED
    );
    assert!(write(&unnamed, "/shared/x").is_ok());
}

#[test]
fn the_nearest_list_decides_over_a_higher_list_and_the_home_rule() {
    let path = "/home/ada/d/f";
    let (segments, _) = unlisted(path);
    let mut along = listed_at(path, 2, Some(1), Ownership::Accounts(vec![2]));
    along[1] = Some(Access {
        creator: None,
        write: Some(Ownership::Owner),
    });
    assert_eq!(
        writers(&segments, &along, Act::Write),
        Writers::Listed {
            path: "/home/ada/d".into(),
            creator: Some(1),
            write: Ownership::Accounts(vec![2]),
        }
    );
    assert!(writable(&named(2, "bo"), &segments, &along, Act::Write).is_ok());
    assert!(writable(&named(1, "ada"), &segments, &along, Act::Write).is_ok());
    let stranger = writable(&named(3, "cy"), &segments, &along, Act::Write).unwrap_err();
    assert_eq!(stranger.code, ::program::code::UNAUTHORIZED);
    assert!(writable(&Writer::Chain, &segments, &along, Act::Write).is_ok());
}

#[test]
fn a_list_names_who_it_allows() {
    assert_eq!(
        allows(Some(1), &Ownership::Owner),
        "only its creator, account 1"
    );
    assert_eq!(
        allows(Some(1), &Ownership::Accounts(vec![4, 9])),
        "its creator, account 1, and account 4, account 9"
    );
    assert_eq!(allows(None, &Ownership::Anyone), "any account");
}

#[test]
fn a_home_folder_list_is_set_by_the_account_named_so_and_any_other_by_its_creator() {
    let (home, along) = unlisted("/home/ada");
    assert!(writable(&named(1, "ada"), &home, &along, Act::SetOwnership).is_ok());
    assert!(writable(&named(1, "ada"), &home, &along, Act::Write).is_err());
    assert!(writable(&named(2, "bo"), &home, &along, Act::SetOwnership).is_err());
    let path = "/shared/d";
    let (segments, _) = unlisted(path);
    let mine = vec![
        None,
        Some(Access {
            creator: Some(1),
            write: None,
        }),
    ];
    assert!(writable(&named(1, "ada"), &segments, &mine, Act::SetOwnership).is_ok());
    let other = writable(&named(2, "bo"), &segments, &mine, Act::SetOwnership).unwrap_err();
    assert_eq!(other.code, ::program::code::UNAUTHORIZED);
    let (_, nobody) = unlisted(path);
    assert!(writable(&named(1, "ada"), &segments, &nobody, Act::SetOwnership).is_err());
    assert!(writable(&Writer::Chain, &segments, &nobody, Act::SetOwnership).is_ok());
}
