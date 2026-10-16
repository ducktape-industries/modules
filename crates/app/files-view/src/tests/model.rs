//! The rules both designs read, without a screen: who writes where, what a
//! form commits, when bytes are text or a picture, and how a desktop lays
//! its pages out.
use std::collections::BTreeMap;

use files::{BlobId, Change, Content, EntryInfo, Node, Ownership, PathOwnership, Writers};
use view::ImageFormat;
use view::TextField;
use view::prelude::Loadable;

use crate::state::{
    Folder, Holds, Opened, Reader, child_of, holds_by_name, name_of, parent_of, picture_format,
};
use crate::tests::{PNG, file};
use crate::ui::Layout;
use crate::upload::{UPLOAD_MOST, progress, puts, sent, uploads};
use crate::words;
use crate::write::{Form, Write, built};
use view::methods::Picked;

const DEV: u64 = 7;
const ADA: u64 = 8;

fn dev() -> Reader {
    Reader {
        account: Some(DEV),
        name: Loadable::Ready(Some("dev".into())),
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
        id: 1,
        write,
        base: None,
        name: TextField::new(name),
        text: TextField::new(text),
        upload: Vec::new(),
        message: TextField::new(message),
        problem: None,
        refused: None,
        sending: false,
    }
}

/// A form's message and its one change.
fn one(built: Result<(String, Vec<Change>), String>) -> (String, [Change; 1]) {
    let (message, changes) = built.unwrap();
    let changes: [Change; 1] = changes.try_into().expect("one change");
    (message, changes)
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
        picture: None,
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
        name: Loadable::Ready(None),
    };
    assert!(unnamed.writes(&Writers::Shared));
    assert!(!unnamed.writes(&Writers::Home("dev".into())));
}

#[test]
fn a_new_folder_is_made_by_its_name_in_the_folder_on_screen() {
    let write = Write::NewFolder {
        folder: "/home/dev".into(),
    };
    let (message, [change]) = one(built(&form(write, " docs ", "", "")));
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
    let (message, [change]) = one(built(&form(write, "a.txt", "line", "First file")));
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
    let (_, [change]) = one(built(&form(write, "", "echo hi", "")));
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
    let (message, [change]) = one(rename("/home/dev/a"));
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
fn a_folder_made_at_a_files_path_is_no_file_to_show_or_write() {
    let mut now_a_folder = opened(b"", true);
    now_a_folder.entry = Some(EntryInfo {
        path: "/shared/a".into(),
        node: Node::Dir {
            entries: 0,
            tree: abi::BlobId::Sha256([0; 32]),
        },
        meta: BTreeMap::new(),
    });
    assert!(now_a_folder.file().is_none());
    assert!(!now_a_folder.editable());
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
    assert_eq!(parent_of("/home/dev/notes.txt"), "/home/dev");
    assert_eq!(parent_of("/shared"), "/");
}

#[test]
fn a_picture_is_known_by_its_first_bytes_and_is_never_text() {
    assert_eq!(picture_format(&PNG), Some(ImageFormat::Png));
    assert_eq!(
        picture_format(&[0xff, 0xd8, 0xff, 0xe0]),
        Some(ImageFormat::Jpeg)
    );
    assert_eq!(picture_format(b"GIF89a.."), Some(ImageFormat::Gif));
    assert_eq!(
        picture_format(b"RIFF\0\0\0\0WEBPVP8 "),
        Some(ImageFormat::Webp)
    );
    assert_eq!(
        picture_format(b"BM\x36\0\0\0\0\0\0\0"),
        Some(ImageFormat::Bmp)
    );
    // a text that starts as a bitmap's mark has no zero bytes after it
    assert_eq!(picture_format(b"BMW owners' notes"), None);
    let picture = opened(&PNG, true);
    assert_eq!(picture.holds(), Holds::Picture);
    assert_eq!(picture.text(), None);
    assert!(
        !picture.editable(),
        "a picture's bytes are not edited as text"
    );
    assert_eq!(opened(b"plain", true).holds(), Holds::Text);
    assert_eq!(opened(&[0xff, 0xfe], true).holds(), Holds::Bytes);
}

#[test]
fn a_listed_file_is_known_by_its_name() {
    assert_eq!(holds_by_name("cat.JPG"), Holds::Picture);
    assert_eq!(holds_by_name("notes.md"), Holds::Text);
    assert_eq!(holds_by_name("archive.zip"), Holds::Bytes);
    assert_eq!(
        holds_by_name(".png"),
        Holds::Bytes,
        "a dotfile has no extension"
    );
    assert_eq!(holds_by_name("README"), Holds::Bytes);
}

#[test]
fn a_desktop_folds_its_side_under_880_and_splits_from_1200() {
    assert_eq!(Layout::of(479.), Layout::Narrow);
    assert_eq!(Layout::of(879.), Layout::Narrow);
    assert_eq!(Layout::of(880.), Layout::Wide);
    assert_eq!(Layout::of(1199.), Layout::Wide);
    assert_eq!(Layout::of(1200.), Layout::Split);
}

fn picked(name: &str, size: u64) -> Picked {
    Picked {
        name: name.into(),
        size,
        file: 0,
    }
}

fn listed(path: &str, node: Node) -> EntryInfo {
    EntryInfo {
        path: path.into(),
        node,
        meta: Default::default(),
    }
}

#[test]
fn an_upload_carries_small_files_in_its_commit_and_stages_the_rest() {
    let chunk = files::CHUNK_SIZE;
    let listed = [
        listed("/shared/old.txt", file()),
        listed(
            "/shared/photos",
            Node::Dir {
                tree: BlobId::Sha256([0; 32]),
                entries: 1,
            },
        ),
    ];
    let uploads = uploads(
        "/shared",
        vec![
            picked("a.png", 700 << 10),
            // past what the commit still carries: staged
            picked("b.png", 700 << 10),
            picked("big.mov", 2 * chunk + 1),
            picked("huge.mov", UPLOAD_MOST + 1),
            picked("old.txt", 3),
            // a folder of its name is there: never written over
            picked("photos", 3),
            // picked twice: the second is left out
            picked("a.png", 1),
        ],
        &listed,
    );
    let paths: Vec<_> = uploads.iter().map(|upload| upload.path.clone()).collect();
    assert_eq!(paths[0], Ok("/shared/a.png".into()));
    assert_eq!(paths[3], Err(words::too_big_to_upload(UPLOAD_MOST + 1)));
    assert_eq!(paths[5], Err(words::held_by(&listed[1].node)));
    assert_eq!(paths[6], Err(words::PICKED_TWICE.into()));
    assert!(uploads[4].replaces && !uploads[0].replaces && !uploads[5].replaces);
    assert_eq!(
        progress(&uploads),
        Some((0, 1 + 3)),
        "b.png's one chunk, big.mov's three"
    );
    assert_eq!(sent(&uploads), ["a.png", "b.png", "big.mov", "old.txt"]);
    assert_eq!(
        puts(&uploads),
        Err(words::NOTHING_TO_UPLOAD.into()),
        "nothing is read yet"
    );
}

#[test]
fn an_upload_with_nothing_to_send_builds_no_commit() {
    let uploads = uploads("/shared", vec![picked("a/b", 1)], &[]);
    assert!(uploads[0].path.is_err(), "a name with a slash is no name");
    assert_eq!(puts(&uploads), Err(words::NOTHING_TO_UPLOAD.into()));
    assert_eq!(words::uploads(&["a.png"]), "Upload a.png");
    assert_eq!(words::uploads(&["a.png", "b.png"]), "Upload 2 files");
}
