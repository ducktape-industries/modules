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
/// A one-pixel PNG.
pub(crate) const PNG: [u8; 67] = [
    0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a, 0x00, 0x00, 0x00, 0x0d, 0x49, 0x48, 0x44, 0x52,
    0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x01, 0x08, 0x06, 0x00, 0x00, 0x00, 0x1f, 0x15, 0xc4,
    0x89, 0x00, 0x00, 0x00, 0x0a, 0x49, 0x44, 0x41, 0x54, 0x78, 0x9c, 0x63, 0x00, 0x01, 0x00, 0x00,
    0x05, 0x00, 0x01, 0x0d, 0x0a, 0x2d, 0xb4, 0x00, 0x00, 0x00, 0x00, 0x49, 0x45, 0x4e, 0x44, 0xae,
    0x42, 0x60, 0x82,
];

/// When the seed's first block lands (unix ms), each next one a minute
/// on: pinned, so a byline drawn to the second is the same on every run.
pub(crate) const T0: u64 = 1_790_000_000_000;

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
///    (not text), `/shared/dot.png` ([`PNG`]), the link `/shared/latest` to
///    the readme
/// 3. dev: `/home/dev/notes.txt` again, its text [`NOTES`]
///
/// and the pin `v1` on the first snapshot.
pub(crate) fn seeded() -> Network {
    let net = Network::new();
    net.seat::<files::Files>(files::PROGRAM);
    let dev = net.me();
    let ada = net.person("ada");
    net.at(T0);
    let first = commit(
        &net,
        dev,
        "Start my notes",
        vec![
            put("/home/dev/notes.txt", b"hello"),
            put("/home/dev/docs/plan.md", b"# Plan"),
        ],
    );
    net.at(T0 + 60_000);
    commit(
        &net,
        ada,
        "Share the readme",
        vec![
            put("/home/ada/diary.txt", b"dear diary"),
            put("/shared/readme.txt", b"read me"),
            put("/shared/blob.bin", &NOT_TEXT),
            put("/shared/dot.png", &PNG),
            Change::Symlink {
                path: "/shared/latest".into(),
                target: "/shared/readme.txt".into(),
            },
        ],
    );
    net.at(T0 + 120_000);
    commit(
        &net,
        dev,
        "Say more",
        vec![put("/home/dev/notes.txt", NOTES.as_bytes())],
    );
    net.at(T0 + 180_000);
    let pin = Op::Pin {
        name: "v1".into(),
        snapshot: first,
    };
    net.submit(dev, files::PROGRAM, &pin)
        .expect("a pin on a minted snapshot");
    net
}

/// A file node, for the words a file's kind is said in.
pub(crate) fn file() -> files::Node {
    files::Node::File {
        file: files::BlobId::Sha256([0; 32]),
        size: 0,
        exec: false,
    }
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

/// The view over `net`, read by dev, under a pointer in a pane `width`
/// wide.
pub(crate) fn desktop_at(net: &Network, width: f32) -> TestAppContext {
    let mut cx = desktop(net);
    cx.simulate_resize(width, 900.);
    cx.run_until_parked();
    cx
}

/// The view over `net`, read by dev, under a finger.
pub(crate) fn phone(net: &Network) -> TestAppContext {
    opened_as(net, net.session(), true)
}

/// Whether some text on screen contains `part`.
pub(crate) fn shows(cx: &TestAppContext, part: &str) -> bool {
    cx.texts().iter().any(|text| text.contains(part))
}
