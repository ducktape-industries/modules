//! The rules both designs read, without a screen: who writes where, what a
//! form commits, and when bytes are text.
use std::collections::BTreeMap;

use files::{Change, Content, EntryInfo, Node, Ownership, PathOwnership, Writers};
use view::TextField;

use crate::state::{Folder, Name, Opened, Reader, child_of, name_of};
use crate::words;
use crate::write::{Form, Write, built};

const DEV: u64 = 7;
const ADA: u64 = 8;

fn dev() -> Reader {
    Reader {
        account: Some(DEV),
        name: Name::Named("dev".into()),
    }
}

fn folder(path: &str, writers: Option<Writers>) -> Folder {
    Folder {
        path: path.into(),
        snapshot: None,
        entries: Vec::new(),
        ownership: writers.map(|writers| PathOwnership {
            snapshot: None,
            creator: None,
            write: None,
            writers,
        }),
    }
}

fn form(write: Write, name: &str, text: &str, message: &str) -> Form {
    Form {
        write,
        base: None,
        name: TextField::new(name),
        text: TextField::new(text),
        message: TextField::new(message),
        problem: None,
        refused: None,
        sending: false,
    }
}

fn opened(bytes: &[u8], whole: bool) -> Opened {
    Opened {
        path: "/shared/a".into(),
        snapshot: None,
        entry: Some(EntryInfo {
            path: "/shared/a".into(),
            node: Node::File {
                file: abi::BlobId::Sha256([0; 32]),
                size: 1 << 20,
                exec: false,
            },
            meta: BTreeMap::new(),
        }),
        bytes: bytes.to_vec(),
        whole,
        ownership: None,
    }
}

#[test]
fn what_is_made_under_shared_is_anyones_to_write() {
    for path in ["/shared", "/shared/team/notes"] {
        let children = folder(path, Some(Writers::Chain)).children();
        assert_eq!(children, Writers::Shared, "{path}");
        assert!(dev().writes(&children));
    }
}

#[test]
fn what_is_made_in_a_home_is_its_named_account_alone() {
    let mine = folder("/home/dev/docs", Some(Writers::Home("dev".into()))).children();
    let hers = folder("/home/ada", Some(Writers::Chain)).children();
    assert_eq!(hers, Writers::Home("ada".into()));
    assert!(dev().writes(&mine));
    assert!(!dev().writes(&hers));
}

#[test]
fn nothing_an_account_makes_goes_at_the_root_or_beside_the_homes() {
    for path in ["/", "/home", "/elsewhere"] {
        let children = folder(path, None).children();
        assert_eq!(children, Writers::Chain, "{path}");
        assert!(!dev().writes(&children), "{path}");
    }
}

#[test]
fn a_write_list_over_a_folder_decides_for_what_is_made_in_it() {
    let listed = Writers::Listed {
        path: "/home/ada/team".into(),
        creator: Some(ADA),
        write: Ownership::Accounts(vec![DEV]),
    };
    let children = folder("/home/ada/team/plans", Some(listed.clone())).children();
    assert_eq!(children, listed);
    assert!(
        dev().writes(&children),
        "the list names dev, though the home is ada's"
    );
    let owners_only = Writers::Listed {
        path: "/shared/ada".into(),
        creator: Some(ADA),
        write: Ownership::Owner,
    };
    assert!(!dev().writes(&folder("/shared/ada", Some(owners_only)).children()));
}

#[test]
fn a_reader_signed_out_writes_nothing() {
    let nobody = Reader::default();
    assert!(!nobody.writes(&Writers::Shared));
}

#[test]
fn a_reader_identity_names_no_way_writes_under_shared_only() {
    let unnamed = Reader {
        account: Some(DEV),
        name: Name::Unnamed,
    };
    assert!(unnamed.writes(&Writers::Shared));
    assert!(!unnamed.writes(&Writers::Home("dev".into())));
}

#[test]
fn a_new_folder_is_made_by_its_name_in_the_folder_on_screen() {
    let write = Write::NewFolder {
        folder: "/home/dev".into(),
    };
    let (message, change) = built(&form(write, " docs ", "", "")).unwrap();
    assert_eq!(
        change,
        Change::Mkdir {
            path: "/home/dev/docs".into()
        }
    );
    assert_eq!(
        message, "Make docs",
        "an empty message says what the change does"
    );
}

#[test]
fn a_new_file_in_the_root_carries_its_text_and_the_typed_message() {
    let write = Write::NewFile { folder: "/".into() };
    let (message, change) = built(&form(write, "a.txt", "line", "First file")).unwrap();
    assert_eq!(message, "First file");
    assert_eq!(
        change,
        Change::Put {
            path: "/a.txt".into(),
            exec: false,
            meta: BTreeMap::new(),
            content: Content::Inline(b"line".to_vec()),
        }
    );
}

#[test]
fn a_name_that_is_empty_or_holds_a_slash_builds_nothing() {
    let new = |name: &str| {
        let write = Write::NewFolder {
            folder: "/shared".into(),
        };
        built(&form(write, name, "", "")).unwrap_err()
    };
    assert_eq!(new("  "), words::NAME_IT);
    assert_eq!(new("a/b"), words::ONE_NAME);
    assert!(
        new("..").contains(".."),
        "files' own words for a path it refuses"
    );
}

#[test]
fn an_edit_keeps_the_files_mode_and_metadata() {
    let meta = BTreeMap::from([("type".to_owned(), "text/markdown".to_owned())]);
    let write = Write::Edit {
        path: "/shared/run.sh".into(),
        exec: true,
        meta: meta.clone(),
    };
    let (_, change) = built(&form(write, "", "echo hi", "")).unwrap();
    assert_eq!(
        change,
        Change::Put {
            path: "/shared/run.sh".into(),
            exec: true,
            meta,
            content: Content::Inline(b"echo hi".to_vec()),
        }
    );
}

#[test]
fn a_rename_to_where_it_is_or_to_no_path_builds_nothing() {
    let rename = |to: &str| {
        let write = Write::Rename {
            path: "/shared/a".into(),
        };
        built(&form(write, to, "", ""))
    };
    assert_eq!(rename("/shared/a").unwrap_err(), words::SAME_PATH);
    assert!(rename("b").unwrap_err().contains("absolute"));
    let (message, change) = rename("/home/dev/a").unwrap();
    assert_eq!(
        change,
        Change::Move {
            from: "/shared/a".into(),
            to: "/home/dev/a".into()
        }
    );
    assert_eq!(message, "Move /shared/a to /home/dev/a");
}

#[test]
fn a_read_cut_inside_a_character_is_text_up_to_it() {
    // "é" is two bytes: the read ends after the first
    let cut = opened(&[b'a', 0xc3], false);
    assert_eq!(cut.text(), Some("a"));
    assert!(!cut.editable(), "a file read in part is not edited here");
}

#[test]
fn bytes_that_are_not_utf8_are_not_text() {
    assert_eq!(opened(&[0xff, 0xfe], true).text(), None);
    let whole_but_broken = opened(&[b'a', 0xc3], true);
    assert_eq!(whole_but_broken.text(), None);
    assert!(opened(b"plain", true).editable());
}

#[test]
fn a_path_is_named_by_its_last_segment_and_made_in_its_folder() {
    assert_eq!(name_of("/home/dev/notes.txt"), "notes.txt");
    assert_eq!(name_of("/"), "/");
    assert_eq!(child_of("/", "shared"), "/shared");
    assert_eq!(child_of("/shared", "a"), "/shared/a");
}
