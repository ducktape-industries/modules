//! Files over the fake node: the real `files` program seated beside the
//! boot set, its tree seeded through its own commits, the view opened as
//! the dev account (named `dev`, so its home is `/home/dev`). Tests by
//! what they cover: `model` (the rules both designs read), `desktop`
//! (the pages under a pointer), `phone` (the screens under a finger).
mod desktop;
mod model;
mod phone;

use files::{Change, Content, Op};
use view::methods::{HostSession, Session};
use view::rig::Network;
use view::testing::TestAppContext;

use crate::Files;

/// A file's text in dev's home.
pub(crate) const NOTES: &str = "hello\nfrom dev";
/// Bytes that are not UTF-8.
pub(crate) const NOT_TEXT: [u8; 4] = [0xff, 0xfe, 0x00, 0x01];

pub(crate) fn put(path: &str, bytes: &[u8]) -> Change {
    Change::Put {
        path: path.into(),
        exec: false,
        meta: Default::default(),
        content: Content::Inline(bytes.to_vec()),
    }
}

/// One commit by `who`, based on the head: the snapshot it made.
pub(crate) fn commit(
    net: &Network,
    who: u64,
    message: &str,
    changes: Vec<Change>,
) -> files::BlobId {
    let head = match net.query(files::PROGRAM, &files::Query::Head) {
        Ok(files::Reply::Head(head)) => head,
        other => panic!("files answers its head: {other:?}"),
    };
    let op = Op::Commit {
        base: head.map(|row| row.id),
        message: message.into(),
        changes,
    };
    let output = net
        .submit(who, files::PROGRAM, &op)
        .unwrap_or_else(|refusal| panic!("{message}: {refusal:?}"));
    abi::decode(&output).expect("a commit answers its snapshot")
}

/// The network every test starts from: three commits and a pin.
/// 1. dev: `/home/dev/notes.txt`, `/home/dev/docs/plan.md`
/// 2. ada: `/home/ada/diary.txt`, `/shared/readme.txt`, `/shared/blob.bin`
///    (not text), the link `/shared/latest` to the readme
/// 3. dev: `/home/dev/notes.txt` again, its text [`NOTES`]
///
/// and the pin `v1` on the first snapshot.
pub(crate) fn seeded() -> Network {
    let net = Network::new();
    net.seat::<files::Files>(files::PROGRAM);
    let dev = net.me();
    let ada = net.person("ada");
    let first = commit(
        &net,
        dev,
        "Start my notes",
        vec![
            put("/home/dev/notes.txt", b"hello"),
            put("/home/dev/docs/plan.md", b"# Plan"),
        ],
    );
    commit(
        &net,
        ada,
        "Share the readme",
        vec![
            put("/home/ada/diary.txt", b"dear diary"),
            put("/shared/readme.txt", b"read me"),
            put("/shared/blob.bin", &NOT_TEXT),
            Change::Symlink {
                path: "/shared/latest".into(),
                target: "/shared/readme.txt".into(),
            },
        ],
    );
    commit(
        &net,
        dev,
        "Say more",
        vec![put("/home/dev/notes.txt", NOTES.as_bytes())],
    );
    let pin = Op::Pin {
        name: "v1".into(),
        snapshot: first,
    };
    net.submit(dev, files::PROGRAM, &pin)
        .expect("a pin on a minted snapshot");
    net
}

/// The view over `net`, read by `session`'s account, under a finger when
/// `touch`.
pub(crate) fn opened_as(net: &Network, session: Session, touch: bool) -> TestAppContext {
    let mut cx = TestAppContext::new();
    net.back(&cx.host());
    let sessions = cx.host().stream::<HostSession>();
    cx.open::<Files>();
    cx.simulate_touch(touch);
    sessions.send(session);
    cx.run_until_parked();
    cx
}

/// The view over `net`, read by dev, under a pointer.
pub(crate) fn desktop(net: &Network) -> TestAppContext {
    opened_as(net, net.session(), false)
}

/// The view over `net`, read by dev, under a finger.
pub(crate) fn phone(net: &Network) -> TestAppContext {
    opened_as(net, net.session(), true)
}

/// Whether some text on screen contains `part`.
pub(crate) fn shows(cx: &TestAppContext, part: &str) -> bool {
    cx.texts().iter().any(|text| text.contains(part))
}
