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
        caller: vec![Party::Account(number)],
        label: Some(label.to_owned()),
    }
}

fn through(number: AccountNumber, program: &str) -> Writer {
    Writer::Account {
        number,
        caller: caller(number, &Origin::Program(program.into())),
        label: None,
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
fn with_no_list_anyone_writes_beneath_the_namespace_roots_and_the_chain_anywhere() {
    let ada = named(1, "ada");
    let unnamed = Writer::Account {
        number: 3,
        caller: vec![Party::Account(3)],
        label: None,
    };
    for writer in [&ada, &unnamed, &through(7, "harness")] {
        for ok in [
            "/home/ada/notes",
            "/home/bo/x",
            "/home/Ada/x/y",
            "/shared/a",
            "/etc/passwd",
            "/x",
        ] {
            assert!(write(writer, ok).is_ok(), "{ok}");
        }
        for root in ["/home", "/home/ada", "/home/bo", "/shared"] {
            assert_eq!(
                write(writer, root).unwrap_err().code,
                ::program::code::UNAUTHORIZED,
                "{root}"
            );
        }
    }
    for path in ["/etc/passwd", "/home/bo/x", "/home", "/shared"] {
        assert!(write(&Writer::Chain, path).is_ok(), "{path}");
    }
    let (segments, along) = unlisted("/home/bo/x");
    assert_eq!(writers(&segments, &along), Writers::Anyone);
}

#[test]
fn the_nearest_list_decides_over_a_higher_list() {
    let path = "/home/ada/d/f";
    let (segments, _) = unlisted(path);
    let bo = Party::Account(2);
    let mut along = listed_at(path, 2, Some(1), Ownership::Listed(vec![bo.clone()]));
    along[1] = Some(Access {
        creator: None,
        write: Some(Ownership::Owner),
    });
    assert_eq!(
        writers(&segments, &along),
        Writers::Listed {
            path: "/home/ada/d".into(),
            creator: Some(1),
            write: Ownership::Listed(vec![bo]),
        }
    );
    assert!(writable(&named(2, "bo"), &segments, &along, Act::Write).is_ok());
    assert!(writable(&named(1, "ada"), &segments, &along, Act::Write).is_ok());
    let stranger = writable(&named(3, "cy"), &segments, &along, Act::Write).unwrap_err();
    assert_eq!(stranger.code, ::program::code::UNAUTHORIZED);
    assert!(writable(&Writer::Chain, &segments, &along, Act::Write).is_ok());
}

#[test]
fn a_listed_program_writes_as_any_account_it_sends_for_and_no_other_program_does() {
    let path = "/shared/d/f";
    let (segments, _) = unlisted(path);
    let along = listed_at(
        path,
        1,
        Some(1),
        Ownership::Listed(vec![Party::Program("harness".into())]),
    );
    assert!(writable(&through(7, "harness"), &segments, &along, Act::Write).is_ok());
    assert!(writable(&through(8, "harness"), &segments, &along, Act::Write).is_ok());
    assert!(writable(&named(1, "ada"), &segments, &along, Act::Write).is_ok());
    for refused in [through(8, "chat"), named(8, "agent")] {
        assert_eq!(
            writable(&refused, &segments, &along, Act::Write)
                .unwrap_err()
                .code,
            ::program::code::UNAUTHORIZED
        );
    }
}

#[test]
fn a_list_names_who_it_allows() {
    assert_eq!(
        allows(Some(1), &Ownership::Owner),
        "only its creator, account 1"
    );
    assert_eq!(
        allows(
            Some(1),
            &Ownership::Listed(vec![
                Party::Account(4),
                Party::Account(9),
                Party::Program("harness".into())
            ])
        ),
        "its creator, account 1, and account 4, account 9, program harness"
    );
    assert_eq!(allows(None, &Ownership::Anyone), "anyone");
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
