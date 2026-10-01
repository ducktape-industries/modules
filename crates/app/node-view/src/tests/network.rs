//! The sheet with `chain.network`: each validator's newest vote, what a
//! node that is not voting shows, a refusal after an answer, a node that
//! does not serve it, a silent node, and the narrow widths.
use super::*;
use crate::table::NO_NETWORK;

/// How the app refuses `chain.network` for a node without the route.
fn unsupported(cx: &TestAppContext) {
    cx.host()
        .refuse::<ChainNetwork>(methods::refusal::UNKNOWN_REQUEST, NO_NETWORK);
}

/// The sheet over node `this`, serving `chain.network` with `network`.
fn voting(this: [u8; 2], network: NetworkStatus) -> (TestAppContext, StreamSender<ClockTicks>) {
    let mut cx = TestAppContext::new();
    let ticks = node(&cx);
    cx.host().handle::<ChainStatus>(move |()| {
        Ok(NodeStatus {
            identity: this.to_vec(),
            ..status()
        })
    });
    cx.host().stream::<Changes<ValsetApi>>();
    respond(&mut cx);
    cx.host()
        .handle::<ChainNetwork>(move |()| Ok(network.clone()));
    cx.open::<Nodes>();
    cx.run_until_parked();
    (cx, ticks)
}

const VOTES: &str =
    "Height: the newest block the validator voted to finalize. Quiet: none for 20 blocks.";
const NOT_VOTING: &str = "This node isn't voting right now, so it can't see the validators' votes.";

/// With `chain.network`: a validator's newest vote, how far it is from the
/// tip, one word; a resident's row is its key and address.
#[test]
fn the_network_fills_height_behind_and_status() {
    let (mut cx, ticks) = voting(THIS, seen(4200));
    let row = |cx: &TestAppContext, index: usize| texts_of(cx, &format!("nodes-row-{index}"));
    assert_eq!(
        row(&cx, 0)[1..3],
        ["10.0.0.1:4000".to_owned(), "this node".to_owned()]
    );
    let tail = |cx: &TestAppContext, index: usize| {
        let texts = row(cx, index);
        texts[texts.len().saturating_sub(4)..].to_vec()
    };
    assert_eq!(tail(&cx, 0), ["4,200", "voted", "0", "In sync"]);
    assert_eq!(tail(&cx, 1), ["4,199", "voted", "1", "In sync"]);
    assert_eq!(
        row(&cx, 2)[row(&cx, 2).len() - 5..],
        ["3,871", "voted", "329", "Quiet", "since 3,871"]
    );
    assert_eq!(tail(&cx, 3), ["Doesn't propose", "—", "—", "—"]);
    for gone in [
        "Not reported",
        "reported",
        "Heard",
        "Not answering",
        "Withheld",
        "Checking",
        "signed",
    ] {
        assert!(!cx.has_text(gone), "{gone}");
    }
    // the same 20 whatever the set's size (three validators here)
    assert!(cx.has_text(VOTES) && !cx.has_text(NOT_VOTING));
    // one ask in flight at a time, again on the clock
    ticks.send(());
    cx.run_until_parked();
    assert_eq!(cx.host().requests::<ChainNetwork>().len(), 2);
}

/// A node that reports no vote of its own hears none: a resident, and a
/// validator in valset not seated yet (or catching up after a restart)
/// while the others' votes still reach the seated. It says so once, and no
/// validator reads Quiet for the votes it cannot hear.
#[test]
fn a_node_that_is_not_voting_says_so() {
    let unheard = |key: [u8; 2]| Peer {
        key: key.to_vec(),
        signed: None,
    };
    let resident = NetworkStatus {
        height: 4200,
        members: [RESIDENT, UNLISTED, OTHER, THIS].map(unheard).to_vec(),
    };
    let mut promoted = seen(4200);
    promoted.members[3].signed = None;
    for (this, network, marked) in [(RESIDENT, resident, 3), (THIS, promoted, 0)] {
        let (cx, _) = voting(this, network);
        assert!(cx.has_text(NOT_VOTING), "{:?}", cx.texts());
        for index in 0..3 {
            let texts = texts_of(&cx, &format!("nodes-row-{index}"));
            assert_eq!(texts[texts.len() - 3..], ["—", "—", "—"], "row {index}");
        }
        assert!(texts_of(&cx, &format!("nodes-row-{marked}")).contains(&"this node".to_owned()));
        assert!(!cx.has_text("Quiet") && !cx.has_text("voted"));
        assert!(cx.find("nodes-footnote").is_none());
    }
}

/// The table's least width, as the wire carries it.
fn table_min_width(cx: &TestAppContext) -> serde_json::Value {
    match cx.find("nodes-table") {
        Some(ducktape_view_guest::wire::Node::Container(table)) => {
            serde_json::to_value(table.style.min_size.width).unwrap()
        }
        _ => panic!("no table"),
    }
}

/// Where row `row`'s strip sits and its marks' gap: `true` where it is on
/// a line of its own under the row's cells.
fn strip_of(cx: &TestAppContext, row: &str) -> (bool, serde_json::Value) {
    use ducktape_view_guest::wire::Node;
    fn marks(node: &Node) -> Option<&ducktape_view_guest::wire::ContainerNode> {
        match node {
            Node::Container(strip) if strip.children.len() == crate::recent::WINDOW as usize => {
                Some(strip)
            }
            Node::Container(container) => container.children.iter().find_map(marks),
            _ => None,
        }
    }
    let Some(Node::Container(line)) = cx.find(row) else {
        panic!("no {row}");
    };
    let below = matches!(line.children.as_slice(), [_, under] if marks(under).is_some());
    let strip =
        (line.children.iter().find_map(marks)).unwrap_or_else(|| panic!("no strip in {row}"));
    (below, serde_json::to_value(strip.style.gap.width).unwrap())
}

/// As the sheet narrows, the strip's gaps close a pixel at a time; where
/// even the narrowest does not fit beside the other columns, the strip
/// goes on a line of its own under each row's cells, and its name under
/// the column names. It never leaves: at the app's narrowest layout (480),
/// Key, Height, Behind and Status fit the sheet less its inset and gutter
/// (432), with the strip under them.
#[test]
fn a_narrow_sheet_moves_the_strip_under_its_row() {
    let (mut cx, _) = voting(THIS, seen(4200));
    let label = "Proposed · last 64 blocks · 4,137 → 4,200";
    let at = |cx: &mut TestAppContext, width: f32| {
        cx.simulate_measure("nodes-viewport", width, 680.);
        cx.run_until_parked();
        strip_of(cx, "nodes-row-0")
    };
    let (beside, below) = (false, true);
    let gap = |px: &str| serde_json::json!(px);
    assert_eq!(strip_of(&cx, "nodes-row-0"), (beside, gap("3px")));
    assert_eq!(at(&mut cx, 1057.), (beside, gap("3px")));
    assert_eq!(at(&mut cx, 1056.), (beside, gap("2px")));
    assert_eq!(at(&mut cx, 994.), (beside, gap("2px")));
    assert_eq!(at(&mut cx, 993.), (beside, gap("1px")));
    // the column is kept as wide as its name, 41 characters of mono caption
    assert_eq!(at(&mut cx, 943.), (beside, gap("1px")));
    assert_eq!(texts_of(&cx, "nodes-columns")[2], label);
    assert_eq!(table_min_width(&cx), serde_json::json!("895px"));
    // Address stays: under the row, the strip loses nothing
    assert_eq!(at(&mut cx, 942.), (below, gap("3px")));
    assert!(cx.has_text("10.0.0.1:4000") && cx.has_text("since 3,871"));
    assert_eq!(
        texts_of(&cx, "nodes-columns"),
        ["Key", "Address", "Height", "Behind", "Status", label]
    );
    assert_eq!(table_min_width(&cx), serde_json::json!("624px"));
    assert_eq!(at(&mut cx, 480.), (below, gap("3px")));
    assert!(!cx.has_text("Address") && !cx.has_text("10.0.0.1:4000"));
    assert_eq!(
        texts_of(&cx, "nodes-row-0"),
        ["abcd", "this node", "4,200", "voted", "0", "In sync"]
    );
    assert_eq!(
        texts_of(&cx, "nodes-row-3"),
        ["0102", "—", "—", "—", "Doesn't propose"]
    );
    // a resident's line goes under its row as a validator's strip does
    let Some(ducktape_view_guest::wire::Node::Container(resident)) = cx.find("nodes-row-3") else {
        panic!("no nodes-row-3");
    };
    assert_eq!(resident.children.len(), 2, "cells, then Doesn't propose");
    assert_eq!(texts_of(&cx, "nodes-columns").last().unwrap(), label);
    assert!(cx.has_text("since 3,871") && cx.has_text(VOTES));
    assert_eq!(table_min_width(&cx), serde_json::json!("428px"));
    assert_eq!(at(&mut cx, 1064.), (beside, gap("3px")));
    assert!(cx.has_text("10.0.0.1:4000"));
}

/// A refusal after an answer (a node restarting past the app's retries, a
/// dropped link) keeps the votes on screen and is logged; only a node that
/// does not serve `chain.network` clears them.
#[test]
fn a_refusal_after_an_answer_keeps_the_votes() {
    let (mut cx, ticks) = voting(THIS, seen(4200));
    cx.host()
        .refuse::<ChainNetwork>("unavailable", "The node could not be reached.");
    ticks.send(());
    cx.run_until_parked();
    assert!(
        cx.has_text("4,199") && cx.has_text(VOTES),
        "{:?}",
        cx.texts()
    );
    assert!(!cx.has_text(NO_NETWORK));
    assert!(
        cx.host()
            .logs()
            .iter()
            .any(|line| line.contains("could not be reached"))
    );
    unsupported(&cx);
    ticks.send(());
    cx.run_until_parked();
    assert!(cx.has_text(NO_NETWORK) && !cx.has_text("voted"));
}

/// A node that does not serve `chain.network` says so in one line over
/// empty cells, and logs it once.
#[test]
fn a_node_without_the_network_says_so_and_logs_once() {
    let mut cx = TestAppContext::new();
    let ticks = node(&cx);
    unsupported(&cx);
    cx.host().stream::<Changes<ValsetApi>>();
    respond(&mut cx);
    cx.open::<Nodes>();
    cx.run_until_parked();
    for _ in 0..2 {
        ticks.send(());
        cx.run_until_parked();
    }
    assert_eq!(cx.host().requests::<ChainNetwork>().len(), 3);
    assert!(cx.has_text(NO_NETWORK), "{:?}", cx.texts());
    for index in 0..3 {
        let texts = texts_of(&cx, &format!("nodes-row-{index}"));
        assert_eq!(texts[texts.len() - 3..], ["—", "—", "—"], "row {index}");
    }
    assert!(!cx.has_text("Quiet") && !cx.has_text("voted"));
    assert!(cx.find("nodes-footnote").is_none());
    let logged = cx
        .host()
        .logs()
        .iter()
        .filter(|line| line.contains("the validators' votes"))
        .count();
    assert_eq!(logged, 1, "{:?}", cx.host().logs());
}

/// Every status badge's text colour, in row order.
fn badge_inks(node: &ducktape_view_guest::wire::Node, inks: &mut Vec<ducktape_view_guest::Hsla>) {
    use ducktape_view_guest::wire::{ElementIdWire, Node};
    if let Node::Container(container) = node {
        if let Some(ElementIdWire::NamedInteger(name, _)) = &container.id
            && name == "nodes-status-word"
        {
            inks.extend(container.style.text.color);
        }
        for child in &container.children {
            badge_inks(child, inks);
        }
    }
}

/// A node that stops answering keeps its last numbers and says so after
/// three silent seconds; the last block ages meanwhile. Its rows are what
/// it said then: every badge goes grey under the last answer's age.
#[test]
fn a_silent_node_reads_not_answering() {
    let (mut cx, ticks) = ready();
    cx.host().never::<ChainStatus>();
    for _ in 0..3 {
        ticks.send(());
    }
    cx.run_until_parked();
    assert!(
        cx.has_text("In sync") && cx.has_text("3s ago") && !cx.has_text("Last answer 3s ago"),
        "{:?}",
        cx.texts()
    );
    let theme = ducktape_view_guest::Theme::light();
    let inks = |cx: &TestAppContext| {
        let mut inks = Vec::new();
        badge_inks(cx.find("nodes-table").expect("the table"), &mut inks);
        inks
    };
    assert_eq!(inks(&cx), [theme.success, theme.success, theme.warning]);
    ticks.send(());
    cx.run_until_parked();
    assert!(cx.has_text("Not answering") && cx.has_text("4,200"));
    assert!(cx.has_text("Last answer 4s ago"), "{:?}", cx.texts());
    assert_eq!(inks(&cx), [theme.muted; 3]);
}
