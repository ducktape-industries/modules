//! The row words, one state each: the fold of valset's answers, the strip's
//! window, and every Height, Behind and Status a row can read.
use ducktape_view_guest::methods::{Block, BlockPage, NetworkStatus, Peer};

use crate::queries::{Node, fold};
use crate::recent::{Recent, WINDOW};
use crate::row::{Row, Status, synced, unsynced};

fn member(key: u8, role: valset::Role) -> valset::Membership {
    valset::Membership {
        key: vec![key],
        address: format!("10.0.0.{key}:4000"),
        role,
    }
}

fn node(key: u8, validator: bool) -> Node {
    Node {
        key: vec![key],
        address: String::new(),
        validator,
        seat: None,
    }
}

#[test]
fn the_fold_seats_validators_first_and_keeps_each_key_once() {
    use valset::Role::{Resident, Validator};
    let nodes = fold(
        &[vec![3], vec![1], vec![9]],
        vec![
            member(1, Validator),
            member(2, Resident),
            member(3, Validator),
            member(3, Validator),
            member(4, Resident),
        ],
    );
    let keys: Vec<(u8, Option<u32>)> = nodes.iter().map(|n| (n.key[0], n.seat)).collect();
    assert_eq!(
        keys,
        [
            (3, Some(0)),
            (1, Some(1)),
            (9, Some(2)),
            (2, None),
            (4, None)
        ]
    );
    assert!(nodes[2].validator && nodes[2].address.is_empty());
    assert!(!nodes[3].validator);
}

fn led(from: u64, to: u64, by: impl Fn(u64) -> u8) -> Vec<Block> {
    (from..=to)
        .rev()
        .map(|height| Block {
            height,
            proposer: Some(vec![by(height)]),
            ..Block::default()
        })
        .collect()
}

#[test]
fn the_strip_asks_the_highest_missing_run_then_keeps_its_span() {
    let mut recent = Recent::default();
    let first = recent.next(100).unwrap();
    assert_eq!(
        first,
        BlockPage {
            before: Some(101),
            limit: 20
        }
    );
    recent.land(&first, 100, led(81, 100, |_| 1));
    let older = recent.next(100).unwrap();
    assert_eq!(
        older,
        BlockPage {
            before: Some(81),
            limit: 20
        }
    );
    for page in [
        older,
        BlockPage {
            before: Some(61),
            limit: 20,
        },
    ] {
        let below = page.before.unwrap();
        recent.land(&page, 100, led(below - 20, below - 1, |_| 1));
    }
    assert_eq!(
        recent.next(100),
        Some(BlockPage {
            before: Some(41),
            limit: 4
        })
    );
    recent.land(
        &BlockPage {
            before: Some(41),
            limit: 4,
        },
        100,
        led(37, 40, |_| 2),
    );
    assert_eq!(recent.next(100), None);
    assert_eq!(recent.led.len() as u64, WINDOW);
    // a head two blocks on asks for those two, and 37 and 38 fall out
    let fresh = recent.next(102).unwrap();
    assert_eq!(
        fresh,
        BlockPage {
            before: Some(103),
            limit: 2
        }
    );
    recent.land(&fresh, 102, led(101, 102, |_| 1));
    assert_eq!(recent.led.keys().next(), Some(&39));
    assert_eq!(recent.proposed(&[2], 102), (2, Some(40), 64));
    assert_eq!(recent.led(&[1], 102), Some(true));
    assert_eq!(recent.led(&[1], 39), Some(false));
    assert_eq!(recent.led(&[1], 200), None);
}

#[test]
fn a_short_archive_ends_the_asking() {
    let mut recent = Recent::default();
    let first = recent.next(10).unwrap();
    assert_eq!(
        first,
        BlockPage {
            before: Some(11),
            limit: 11
        }
    );
    recent.land(&first, 10, led(4, 10, |_| 1));
    assert_eq!(recent.bottom, 4);
    assert_eq!(recent.next(10), None);
}

/// Four validators: Quiet after 16 blocks without leading one.
fn strip(last: u64) -> Recent {
    let mut recent = Recent::default();
    let page = BlockPage {
        before: Some(101),
        limit: 64,
    };
    recent.land(&page, 100, led(37, 100, |h| if h == last { 7 } else { 1 }));
    recent
}

#[test]
fn a_validator_reads_quiet_past_four_blocks_a_validator() {
    let validator = node(7, true);
    let row = unsynced(&validator, 100, 4, &strip(84));
    assert_eq!(row.status, Status::Led { count: 1, of: 64 });
    assert_eq!(row.status.word(), "1 of 64");
    let row = unsynced(&validator, 100, 4, &strip(83));
    assert_eq!(
        row.status,
        Status::Quiet {
            since: Some(83),
            of: 64
        }
    );
    let row = unsynced(&validator, 100, 4, &strip(0));
    assert_eq!(
        row.status,
        Status::Quiet {
            since: None,
            of: 64
        }
    );
    assert_eq!((row.signed, row.behind), (None, None));
}

/// A resident's cells say nothing, with or without `chain.network`.
#[test]
fn a_residents_row_is_blank() {
    let resident = node(2, false);
    let blank = Row {
        signed: None,
        behind: None,
        status: Status::Blank,
    };
    assert_eq!(unsynced(&resident, 100, 4, &Recent::default()), blank);
    let network = network(vec![peer(2, Some(100))]);
    assert_eq!(synced(&resident, 4, &network), blank);
    assert_eq!(blank.status.word(), "—");
}

fn network(members: Vec<Peer>) -> NetworkStatus {
    NetworkStatus {
        height: 4_295,
        members,
    }
}

fn peer(key: u8, signed: Option<u64>) -> Peer {
    Peer {
        key: vec![key],
        signed,
    }
}

/// Four validators: In sync within 2 blocks, then N behind, then Quiet
/// past 16; one this node has heard no vote from is Quiet too. No vote
/// counts before its block is applied, so none reads ahead.
#[test]
fn every_word_a_validators_signature_gives() {
    let cases = [
        (Some(4_295), Some(0), Status::InSync),
        (Some(4_293), Some(2), Status::InSync),
        (Some(4_292), Some(3), Status::Behind(3)),
        (Some(4_279), Some(16), Status::Behind(16)),
        (
            Some(4_278),
            Some(17),
            Status::Quiet {
                since: Some(4_278),
                of: 0,
            },
        ),
        (None, None, Status::Quiet { since: None, of: 0 }),
    ];
    for (signed, behind, status) in cases {
        let row = synced(&node(1, true), 4, &network(vec![peer(1, signed)]));
        assert_eq!(
            row,
            Row {
                signed,
                behind,
                status
            }
        );
    }
    // a validator the reply does not list reads as one that signed nothing
    let row = synced(&node(9, true), 4, &network(vec![]));
    assert_eq!(row.status.word(), "Quiet");
    assert_eq!(Status::Behind(1_200).word(), "1,200 behind");
}
