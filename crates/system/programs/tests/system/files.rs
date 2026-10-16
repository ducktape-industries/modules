//! files founded on ducktape's host: chunks staged as kernel blobs, referenced by a
//! later frame of the same block, read back, and dropped with a refused commit; an
//! account writes its own home, named after it in identity, and is refused another's;
//! a write list set on a folder decides who writes under it.
use super::*;

use files::{BlobId, CHUNK_SIZE, Change, Content, Op, Ownership, Query, Reply, Writers};

const ADA_KEY: u64 = 11;
const BO_KEY: u64 = 12;
const CY_KEY: u64 = 13;

fn apps() -> Vec<Founding> {
    vec![founding(files::PROGRAM, &program("files"))]
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
        let Reply::Contents(contents) = self.ask(files::PROGRAM, &asked).await else {
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
        net.apply(&ada, identity::PROGRAM, &create).await;

        let full = vec![b'a'; CHUNK_SIZE as usize];
        let first: BlobId = abi::decode(
            &net.apply(&ada, files::PROGRAM, &Op::Stage { bytes: full })
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
        let last = program::blob_id(abi::HashKind::Sha256, files::CHUNK, b"tail").unwrap();
        let same_block = vec![
            net.submission(&ada, files::PROGRAM, abi::encode(&tail)),
            net.submission(
                &ada,
                files::PROGRAM,
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
        assert!(net.blob(&first).await.is_some());

        let doomed = b"never stored".to_vec();
        let doomed_id = program::blob_id(abi::HashKind::Sha256, files::CHUNK, &doomed).unwrap();
        let refused = commit(
            Some(snapshot),
            vec![
                put("/shared/new", &doomed),
                put("/shared/a/under-a-file", b"x"),
            ],
        );
        assert_eq!(
            net.refuse(&ada, files::PROGRAM, &refused).await,
            reason::WRONG_STATE
        );
        assert_eq!(net.blob(&doomed_id).await, None);
        let Reply::Head(Some(head)) = net.ask(files::PROGRAM, &Query::Head).await else {
            panic!()
        };
        assert_eq!((head.id, head.seq), (snapshot, 1));

        let stranger = public(99);
        assert_eq!(
            net.refuse(
                &stranger,
                files::PROGRAM,
                &commit(None, vec![put("/shared/s", b"s")])
            )
            .await,
            reason::UNAUTHORIZED
        );

        net.apply(
            &ada,
            files::PROGRAM,
            &commit(Some(snapshot), vec![put("/home/Ada/notes", b"mine")]),
        )
        .await;
        assert_eq!(net.read("/home/Ada/notes", 0, 100).await.bytes, b"mine");
        assert_eq!(
            net.refuse(
                &ada,
                files::PROGRAM,
                &commit(None, vec![put("/home/Bo/notes", b"theirs")])
            )
            .await,
            reason::UNAUTHORIZED
        );
    });
}

#[test]
fn a_write_list_on_a_shared_folder_lets_its_creator_and_the_listed_accounts_write_under_it() {
    deterministic::Runner::default().start(|context| async move {
        let dir = tempfile::tempdir().unwrap();
        let mut net = Net::found_with(context, dir.path(), apps(), Vec::new(), MEMBER_CAP).await;
        let mut numbers = Vec::new();
        for (key, name) in [(ADA_KEY, "Ada"), (BO_KEY, "Bo"), (CY_KEY, "Cy")] {
            let create = identity::Op::Create {
                name: name.into(),
                scheme: Scheme::Ed25519,
            };
            let number: u64 =
                abi::decode(&net.apply(&public(key), identity::PROGRAM, &create).await).unwrap();
            numbers.push(number);
        }
        let [ada, bo, _] = numbers[..] else { panic!() };
        let team = Change::SetOwnership {
            path: "/shared/team".into(),
            write: Some(Ownership::Accounts(vec![bo])),
        };
        let first: BlobId = abi::decode(
            &net.apply(
                &public(ADA_KEY),
                files::PROGRAM,
                &commit(None, vec![put("/shared/team/plan", b"ada"), team]),
            )
            .await,
        )
        .unwrap();
        let asked = Query::Ownership {
            path: "/shared/team/plan".into(),
            at: None,
        };
        let Reply::Ownership(owned) = net.ask(files::PROGRAM, &asked).await else {
            panic!()
        };
        assert_eq!(owned.creator, Some(ada));
        assert_eq!(
            owned.writers,
            Writers::Listed {
                path: "/shared/team".into(),
                creator: Some(ada),
                write: Ownership::Accounts(vec![bo]),
            }
        );
        net.apply(
            &public(BO_KEY),
            files::PROGRAM,
            &commit(Some(first), vec![put("/shared/team/plan", b"bo")]),
        )
        .await;
        assert_eq!(net.read("/shared/team/plan", 0, 100).await.bytes, b"bo");
        assert_eq!(
            net.refuse(
                &public(CY_KEY),
                files::PROGRAM,
                &commit(None, vec![put("/shared/team/cy", b"cy")])
            )
            .await,
            reason::UNAUTHORIZED
        );
    });
}
