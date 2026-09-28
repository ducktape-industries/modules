//! The row words, one state each: the fold of valset's answers, the strip's
//! window, and every Height, Behind, Heard and Status a row can read.
use ducktape_view_guest::methods::{Block, BlockPage, NetworkStatus, Peer, Report, Said};

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
    let row = unsynced(&validator, false, 100, 4, &strip(84));
    assert_eq!(row.status, Status::Led { count: 1, of: 64 });
    assert_eq!(row.status.word(), "1 of 64");
    let row = unsynced(&validator, false, 100, 4, &strip(83));
    assert_eq!(
        row.status,
        Status::Quiet {
            since: Some(83),
            of: 64
        }
    );
    let row = unsynced(&validator, false, 100, 4, &strip(0));
    assert_eq!(
        row.status,
        Status::Quiet {
            since: None,
            of: 64
        }
    );
    assert_eq!(
        (row.height, row.behind, row.heard.as_str()),
        (None, None, "—")
    );
}

#[test]
fn without_the_network_a_resident_is_not_reported_and_this_node_knows_itself() {
    let recent = Recent::default();
    let resident = unsynced(&node(2, false), false, 100, 4, &recent);
    assert_eq!(resident.status.word(), "Not reported");
    let this = unsynced(&node(2, false), true, 100, 4, &recent);
    assert_eq!(
        this,
        Row {
            height: Some((100, "")),
            behind: Some(0),
            heard: "this node".into(),
            status: Status::InSync,
        }
    );
}

fn network(members: Vec<Peer>) -> NetworkStatus {
    NetworkStatus {
        height: 4_295,
        at: 1_000_000,
        members,
    }
}

fn peer(key: u8, signed: Option<u64>, said: Option<(u64, Report)>) -> Peer {
    Peer {
        key: vec![key],
        signed,
        said: said.map(|(at, report)| Said { at, report }),
    }
}

fn height(height: u64) -> Report {
    Report::Height {
        height,
        tip: [0; 32],
    }
}

#[test]
fn every_word_the_network_gives_a_row() {
    let heard = 999_000;
    let cases = [
        // a validator by its signature, within two blocks
        (
            true,
            peer(1, Some(4_293), Some((heard, height(4_295)))),
            "4,293 signed",
            "2",
            "In sync",
        ),
        // a resident by its report
        (
            false,
            peer(1, None, Some((heard, height(4_251)))),
            "4,251 reported",
            "44",
            "44 behind",
        ),
        // a member past this node's tip: this node lags
        (
            false,
            peer(1, None, Some((heard, height(4_300)))),
            "4,300 reported",
            "-5",
            "5 ahead",
        ),
        // an answer older than three seconds
        (
            true,
            peer(1, Some(3_871), Some((996_999, height(3_871)))),
            "3,871 signed",
            "424",
            "Not answering",
        ),
        // never answered
        (false, peer(1, None, None), "—", "—", "Not answering"),
        // keeps its height to itself
        (
            false,
            peer(1, None, Some((heard, Report::Withheld))),
            "—",
            "—",
            "Withheld",
        ),
        // a validator whose signature this node has not seen reads its report
        (
            true,
            peer(1, None, Some((heard, height(4_294)))),
            "4,294 reported",
            "1",
            "In sync",
        ),
    ];
    for (validator, peer, shown, behind, word) in cases {
        let row = synced(&node(1, validator), false, &network(vec![peer.clone()]));
        let height = row.height.map_or("—".to_owned(), |(h, by)| {
            format!("{} {by}", ducktape_view_guest::design::grouped(h))
        });
        let behind_shown = row.behind.map_or("—".to_owned(), |b| b.to_string());
        assert_eq!(
            (
                height.as_str(),
                behind_shown.as_str(),
                row.status.word().as_str()
            ),
            (shown, behind, word),
            "{peer:?}"
        );
    }
}

#[test]
fn heard_is_the_nodes_clock_minus_the_answer() {
    let peer = peer(1, None, Some((940_000, height(4_295))));
    let row = synced(&node(1, false), false, &network(vec![peer]));
    assert_eq!(row.heard, "1m ago");
    // this node never asks itself: its own tip, and its word
    let row = synced(&node(1, false), true, &network(vec![]));
    assert_eq!(
        (row.height, row.heard.as_str(), row.status),
        (Some((4_295, "")), "this node", Status::InSync)
    );
}
