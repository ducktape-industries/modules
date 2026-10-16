//! `cargo run -p files-view --example live`: Files in the real app over a
//! fake node with the real `files` program seated. The dev account (named
//! `dev`) keeps notes and a plan in its home; ada shares a readme, a link
//! to it and a small image, and keeps a diary in hers; the first snapshot
//! is pinned `v1`. Dev writes in its home and `/shared`, and reads ada's.
use files::{Change, Content, Op};
use view::rig;

fn put(path: &str, bytes: &[u8]) -> Change {
    Change::Put {
        path: path.into(),
        exec: false,
        meta: Default::default(),
        content: Content::Inline(bytes.to_vec()),
    }
}

/// The first eight bytes of a PNG: a file that is not text.
const PNG_HEAD: [u8; 8] = [0x89, b'P', b'N', b'G', 0x0d, 0x0a, 0x1a, 0x0a];

fn main() {
    rig::run("files", |net| {
        net.seat::<files::Files>(files::PROGRAM);
        let dev = net.me();
        let ada = net.person("ada");
        // each commit is based on the head, so it is stale on nothing
        let commit = |who, message: &str, changes| {
            let files::Reply::Head(head) = net.query(files::PROGRAM, &files::Query::Head)? else {
                unreachable!("files answers Head with its head");
            };
            let op = Op::Commit {
                base: head.map(|row| row.id),
                message: message.into(),
                changes,
            };
            net.submit(who, files::PROGRAM, &op)
        };
        let first = commit(
            dev,
            "Start my notes",
            vec![
                put(
                    "/home/dev/notes.txt",
                    b"Ship files-view.\nAsk ada about the readme.",
                ),
                put(
                    "/home/dev/docs/plan.md",
                    b"# Plan\n\n1. Browse\n2. Write\n3. History",
                ),
            ],
        )?;
        commit(
            ada,
            "Share the readme",
            vec![
                put(
                    "/shared/readme.txt",
                    b"Everything under /shared is anyone's to write.",
                ),
                put("/shared/logo.png", &PNG_HEAD),
                Change::Symlink {
                    path: "/shared/latest".into(),
                    target: "/shared/readme.txt".into(),
                },
                put("/home/ada/diary.txt", b"Dear diary,"),
            ],
        )?;
        commit(
            dev,
            "Say more in my notes",
            vec![put(
                "/home/dev/notes.txt",
                b"Ship files-view.\nAsk ada about the readme.\nPin the release.",
            )],
        )?;
        let pin = Op::Pin {
            name: "v1".into(),
            snapshot: abi::decode(&first).expect("a commit answers its snapshot"),
        };
        net.submit(dev, files::PROGRAM, &pin)?;
        Ok(())
    })
}
