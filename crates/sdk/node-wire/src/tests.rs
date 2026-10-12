//! The wire's golden: each shape against the bytes core encodes it to,
//! written out by hand in core's field order. A field core adds, drops or
//! moves fails here once the workspace's core rev moves past it. Each
//! vector names the core lines it was read from, at core origin/main
//! 8d076bc40 (`crates/` omitted); recheck them when the core rev moves.
use super::*;

fn text(text: &str) -> Vec<u8> {
    [&(text.len() as u32).to_le_bytes()[..], text.as_bytes()].concat()
}

/// `kernel/host/src/lib.rs:76` (`BlockId = [u8; 32]`), `:117-120` (`Tip`).
fn tip() -> (Tip, Vec<u8>) {
    let tip = Tip {
        height: 3,
        id: [4; 32],
    };
    (tip, [&3u64.to_le_bytes()[..], &[4; 32]].concat())
}

/// `noded/src/wire.rs:5` (`NODE_CONTRACT`), `:30-45` (`Status`);
/// `kernel/abi/src/lib.rs:6` (`Root`), `:96-101` (`Roles`).
#[test]
fn a_status_decodes_the_nodes_field_order_ending_in_the_roles() {
    let mut bytes = text("n");
    for number in [1u64, 2, 3, 4] {
        bytes.extend(number.to_le_bytes());
    }
    bytes.extend([5; 32]);
    bytes.extend([6; 32]);
    bytes.extend(7u64.to_le_bytes());
    bytes.extend([1, 0, 0, 0, 8]);
    bytes.extend(NODE_CONTRACT.to_le_bytes());
    bytes.extend([9; 32]);
    for role in ["r", "v", "i"] {
        bytes.extend(text(role));
    }
    let status = Status {
        network: "n".into(),
        time: 1,
        block_time_ms: 2,
        epoch_length: 3,
        height: 4,
        tip: [5; 32],
        root: Root([6; 32]),
        epoch: 7,
        identity: vec![8],
        contract: NODE_CONTRACT,
        genesis: [9; 32],
        roles: Roles {
            registry: "r".into(),
            validators: "v".into(),
            identity: "i".into(),
        },
    };
    assert_eq!(abi::decode::<Status>(&bytes).unwrap(), status);
    assert_eq!(
        status.anchor(),
        Tip {
            height: 4,
            id: [5; 32]
        }
    );
}

/// The bytes `host::Receipt` encodes a submission with one nested run
/// to: program "a", Applied [7], no events, nested [program "b",
/// Applied [], no events, nested []]. `kernel/host/src/lib.rs:146-155`
/// (`Receipt`); `kernel/abi/src/lib.rs:171-174` (`Outcome`).
#[test]
fn receipt_decodes_the_hosts_nested_runs() {
    let bytes = [
        1, 0, 0, 0, b'a', 0, 1, 0, 0, 0, 7, 0, 0, 0, 0, 1, 0, 0, 0, //
        1, 0, 0, 0, b'b', 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
    ];
    let receipt: Receipt = abi::decode(&bytes).unwrap();
    assert_eq!(receipt.program, "a");
    assert_eq!(receipt.outcome, Outcome::Applied { output: vec![7] });
    assert_eq!(receipt.nested.len(), 1);
    assert_eq!(receipt.nested[0].program, "b");
}

/// A block of one tx whose receipt carries one event, and the reads that
/// ask for blocks and stream changes. `noded/src/wire.rs:139-147`
/// (`Finalized`), `:124-131` (`Tx`), `:89-93` (`Change`), `:96` and
/// `:103-106` (`MAX_BLOCKS`, `Blocks`), `:110-113` (`BlockRef`), `:57-60`
/// (`Query`); `kernel/host/src/lib.rs:167-170` (`Layer`), `:146-155`
/// (`Receipt`).
#[test]
fn a_finalized_block_and_the_reads_around_it_are_the_nodes_bytes() {
    let (anchor, anchor_bytes) = tip();
    let receipt = Receipt {
        program: "c".into(),
        outcome: Outcome::Applied { output: vec![] },
        events: vec![vec![1, 2]],
        nested: vec![],
    };
    let receipt_bytes = [
        &text("c")[..],
        &[0, 0, 0, 0, 0],
        &[1, 0, 0, 0, 2, 0, 0, 0, 1, 2],
        &[0, 0, 0, 0],
    ]
    .concat();
    let tx = Tx {
        hash: [9; 32],
        signer: vec![1],
        anchor,
        target: "t".into(),
        payload: vec![6],
        receipt: Some(receipt),
    };
    let tx_bytes = [
        &[9; 32][..],
        &[1, 0, 0, 0, 1],
        &anchor_bytes,
        &text("t"),
        &[1, 0, 0, 0, 6],
        &[1],
        &receipt_bytes,
    ]
    .concat();
    let block = Finalized {
        height: 10,
        id: [11; 32],
        parent: [12; 32],
        time: 13,
        epoch: 14,
        proposer: Some(vec![15]),
        txs: vec![tx],
    };
    let block_bytes = [
        &10u64.to_le_bytes()[..],
        &[11; 32],
        &[12; 32],
        &13u64.to_le_bytes(),
        &14u64.to_le_bytes(),
        &[1, 1, 0, 0, 0, 15],
        &[1, 0, 0, 0],
        &tx_bytes,
    ]
    .concat();
    assert_eq!(abi::encode(&block), block_bytes);
    assert_eq!(abi::decode::<Finalized>(&block_bytes).unwrap(), block);

    let change = Change {
        height: 2,
        root: Root([3; 32]),
        writes: vec![(vec![4], Some(vec![5])), (vec![6], None)],
    };
    let change_bytes = [
        &2u64.to_le_bytes()[..],
        &[3; 32],
        &[2, 0, 0, 0],
        &[1, 0, 0, 0, 4, 1, 1, 0, 0, 0, 5],
        &[1, 0, 0, 0, 6, 0],
    ]
    .concat();
    assert_eq!(abi::encode(&change), change_bytes);

    let page = Blocks {
        before: Some(7),
        limit: MAX_BLOCKS,
    };
    let page_bytes = [&[1][..], &7u64.to_le_bytes(), &100u32.to_le_bytes()].concat();
    assert_eq!(abi::encode(&page), page_bytes);
    assert_eq!(
        abi::encode(&BlockRef::Height(7)),
        [&[0][..], &7u64.to_le_bytes()].concat()
    );
    assert_eq!(
        abi::encode(&BlockRef::Id([8; 32])),
        [&[1][..], &[8; 32]].concat()
    );
    let query = Query {
        layer: Layer::Preconfirmed,
        frame: vec![1],
    };
    assert_eq!(abi::encode(&query), [1, 1, 0, 0, 0, 1]);
}

/// `noded/src/wire.rs:124-131` (`Tx`, `receipt: None`).
#[test]
fn a_tx_without_a_receipt_decodes_the_nodes_field_order() {
    let (anchor, anchor_bytes) = tip();
    let mut bytes = vec![9; 32];
    bytes.extend([1, 0, 0, 0, 1]);
    bytes.extend(anchor_bytes);
    bytes.extend([1, 0, 0, 0, b't', 1, 0, 0, 0, 6, 0]);
    let tx = Tx {
        hash: [9; 32],
        signer: vec![1],
        anchor,
        target: "t".into(),
        payload: vec![6],
        receipt: None,
    };
    assert_eq!(abi::decode::<Tx>(&bytes).unwrap(), tx);
}

/// The bytes noded's own test pins `Network` to: `noded/src/network.rs:90`
/// (`the_route_is_borsh_in_declaration_order`); the types at
/// `noded/src/wire.rs:153-157` (`Network`), `:170-173` (`PeerStatus`).
#[test]
fn network_decodes_the_nodes_bytes() {
    let mut bytes = Vec::new();
    bytes.extend(2u64.to_le_bytes());
    bytes.extend(2u32.to_le_bytes());
    bytes.extend(1u32.to_le_bytes());
    bytes.extend([9, 1]);
    bytes.extend(1u64.to_le_bytes());
    bytes.extend(1u32.to_le_bytes());
    bytes.extend([7, 0]);
    let network = Network {
        height: 2,
        members: vec![
            PeerStatus {
                key: vec![9],
                signed: Some(1),
            },
            PeerStatus {
                key: vec![7],
                signed: None,
            },
        ],
    };
    assert_eq!(abi::decode::<Network>(&bytes).unwrap(), network);
    assert_eq!(abi::encode(&network), bytes);
}

#[cfg(feature = "frame")]
mod frame {
    use super::*;
    use crate::commonware_cryptography::{Signer as _, ed25519};
    use crate::keyscheme::KeyScheme;
    use crate::{Body, FRAME_NAMESPACE, Frame};

    /// `kernel/node/src/frame.rs:7` (`NAMESPACE`), `:43-62` (`Frame::sign`).
    #[test]
    fn a_frame_signs_its_body_and_names_the_signer() {
        let key = ed25519::PrivateKey::from_seed(7);
        let (anchor, _) = tip();
        let frame = Frame::sign(&key, b"net", anchor, Some(5), "demo", vec![1, 2]);
        assert_eq!(frame.body.signer, key.public_key().as_ref().to_vec());
        assert_eq!(frame.body.scheme, KeyScheme::Ed25519);
        let decoded: Frame = abi::decode(&frame.encode()).unwrap();
        assert_eq!(decoded, frame);
        assert_eq!(frame.proof.len(), 64);
        assert!(KeyScheme::Ed25519.verify(
            &frame.body.signer,
            FRAME_NAMESPACE,
            &frame.body.preimage(),
            &frame.proof
        ));
    }

    /// `kernel/node/src/frame.rs:10-18` (`Body`), `:27-29` (`preimage`);
    /// `kernel/keyscheme/src/lib.rs:50` (`KeyScheme`).
    #[test]
    fn a_body_encodes_in_the_nodes_field_order() {
        let (anchor, anchor_bytes) = tip();
        let body = Body {
            scheme: KeyScheme::Ed25519,
            signer: vec![1],
            network: vec![2],
            anchor,
            salt: Some(5),
            target: "t".into(),
            payload: vec![6],
        };
        let mut bytes = abi::encode(&KeyScheme::Ed25519);
        bytes.extend([1, 0, 0, 0, 1, 1, 0, 0, 0, 2]);
        bytes.extend(anchor_bytes);
        bytes.push(1);
        bytes.extend(5u64.to_le_bytes());
        bytes.extend([1, 0, 0, 0, b't', 1, 0, 0, 0, 6]);
        assert_eq!(body.preimage(), bytes);
    }
}
