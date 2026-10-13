//! files founded on ducktape's host: chunks staged as kernel blobs, referenced by a
//! later frame of the same block, read back, and dropped with a refused commit.
use super::*;

use files::{BlobId, CHUNK_SIZE, Change, Content, Op, Query, Reply};

const ADA_KEY: u64 = 11;

fn apps() -> Vec<Founding> {
    vec![founding(files::MODULE, &program("files"))]
}

fn put(path: &str, bytes: &[u8]) -> Change {
    Change::Put {
        path: path.into(),
        exec: false,
        meta: Default::default(),
        content: Content::Inline(bytes.to_vec()),
    }
}

fn commit(base: Option<BlobId>, changes: Vec<Change>) -> Op {
    Op::Commit {
        base,
        message: "work".into(),
        changes,
    }
}

impl Net {
    async fn read(&self, path: &str, offset: u64, len: u64) -> files::Contents {
        let asked = Query::Read {
            path: path.into(),
            at: None,
            offset,
            len,
        };
        let Reply::Contents(contents) = self.ask(files::MODULE, &asked).await else {
            panic!()
        };
        contents
    }
}

#[test]
fn files_keeps_its_bytes_as_kernel_blobs_and_its_refs_in_state() {
    deterministic::Runner::default().start(|context| async move {
        let dir = tempfile::tempdir().unwrap();
        let mut net = Net::found_with(context, dir.path(), apps(), Vec::new(), MEMBER_CAP).await;
        let ada = public(ADA_KEY);
        let create = identity::Op::Create {
            name: "Ada".into(),
            scheme: Scheme::Ed25519,
        };
        net.apply(&ada, identity::MODULE, &create).await;

        let full = vec![b'a'; CHUNK_SIZE as usize];
        let first: BlobId = abi::decode(
            &net.apply(&ada, files::MODULE, &Op::Stage { bytes: full })
                .await,
        )
        .unwrap();
        let tail = Op::Stage {
            bytes: b"tail".to_vec(),
        };
        let chunks_change = |last: BlobId| Change::Put {
            path: "/shared/big".into(),
            exec: false,
            meta: Default::default(),
            content: Content::Chunks {
                size: CHUNK_SIZE + 4,
                chunks: vec![first, last],
            },
        };
        let last = guest::blob_id(abi::HashKind::Sha256, files::CHUNK, b"tail").unwrap();
        let same_block = vec![
            net.submission(&ada, files::MODULE, abi::encode(&tail)),
            net.submission(
                &ada,
                files::MODULE,
                abi::encode(&commit(
                    None,
                    vec![chunks_change(last), put("/shared/a", b"hello")],
                )),
            ),
        ];
        let applied = net.block(same_block).await;
        let snapshot: BlobId = abi::decode(output_of(&applied.submissions[1])).unwrap();
        assert_eq!(
            abi::decode::<BlobId>(output_of(&applied.submissions[0])).unwrap(),
            last
        );

        let across = net.read("/shared/big", CHUNK_SIZE - 2, 10).await;
        assert_eq!(
            (across.bytes.as_slice(), across.eof),
            (&b"aatail"[..], true)
        );
        assert_eq!(across.snapshot, snapshot);
        assert_eq!(net.read("/shared/a", 0, 100).await.bytes, b"hello");
        assert!(net.host.blob(&first).unwrap().is_some());

        let doomed = b"never stored".to_vec();
        let doomed_id = guest::blob_id(abi::HashKind::Sha256, files::CHUNK, &doomed).unwrap();
        let refused = commit(
            Some(snapshot),
            vec![
                put("/shared/new", &doomed),
                put("/shared/a/under-a-file", b"x"),
            ],
        );
        assert_eq!(
            net.refuse(&ada, files::MODULE, &refused).await,
            reason::WRONG_STATE
        );
        assert_eq!(net.host.blob(&doomed_id).unwrap(), None);
        let Reply::Head(Some(head)) = net.ask(files::MODULE, &Query::Head).await else {
            panic!()
        };
        assert_eq!((head.id, head.seq), (snapshot, 1));

        let stranger = public(99);
        assert_eq!(
            net.refuse(
                &stranger,
                files::MODULE,
                &commit(None, vec![put("/shared/s", b"s")])
            )
            .await,
            reason::UNAUTHORIZED
        );
    });
}
