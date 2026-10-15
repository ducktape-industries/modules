//! `ObjectWriter::flush` over the mock host: an object is stored under the id the pack reader gave it, or the flush fails naming both ids.

use gitcore::{Hash, Kind, Object, Oid, WriteObjects};
use program::{BlobId, MockHost, code};

use super::{ObjectWriter, blob_id_of};

fn oid(hash: Hash, body: &[u8]) -> Oid {
    gitcore::oid_of(hash, Kind::Blob, body).expect("a blob's id")
}

#[test]
fn an_object_kept_under_its_own_id_is_stored_under_it() {
    for hash in [Hash::Sha1, Hash::Sha256] {
        let host = MockHost::default();
        let ctx = host.exec(MockHost::env("forge"));
        let id = oid(hash, b"hello\n");
        let mut writer = ObjectWriter::new(&ctx, hash);
        writer
            .keep(id, Object::new(Kind::Blob, b"hello\n".to_vec()))
            .expect("kept");
        writer.flush().expect("flushed");
        let stored: Vec<BlobId> = host.borrow().blobs.keys().copied().collect();
        assert_eq!(stored, vec![blob_id_of(&id)], "{hash:?}");
    }
}

#[test]
fn an_object_kept_under_another_id_fails_the_flush_naming_both_ids() {
    for hash in [Hash::Sha1, Hash::Sha256] {
        let host = MockHost::default();
        let ctx = host.exec(MockHost::env("forge"));
        let named = oid(hash, b"other\n");
        let actual = blob_id_of(&oid(hash, b"hello\n"));
        let mut writer = ObjectWriter::new(&ctx, hash);
        writer
            .keep(named, Object::new(Kind::Blob, b"hello\n".to_vec()))
            .expect("kept");
        let error = writer
            .flush()
            .expect_err("a misnamed object fails the flush");
        assert_eq!(error.code, code::PROTOCOL, "{hash:?}");
        assert!(
            error.message.contains(&named.to_string()),
            "{}",
            error.message
        );
        assert!(
            error.message.contains(&abi::hex(actual.digest())),
            "{}",
            error.message
        );
    }
}
