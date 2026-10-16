//! Nodes under a finger: the Expo app's two screens (`ui/phone/`), each in
//! the states Expo draws (full, still being read, refused, empty) and
//! through each of its presses; the width sweep at 360, 390 and 430
//! (SPEC §8.1d-1); and a vote's outcome in the Expo app's words.
use super::voting::{sheet, submitted};
use super::*;
use crate::voting::{cancel, said};
use valset::{Motion, Op, Pending, Voted};
use view::methods::{Submit, Submitted};
use view::testing;

/// The view as a phone 390 wide shows it.
fn phone(cx: &mut TestAppContext) {
    cx.simulate_touch(true);
    cx.simulate_resize(390., 844.);
    cx.run_until_parked();
}

/// Every text in `expected` is on screen.
fn shows(cx: &TestAppContext, expected: &[&str]) {
    for text in expected {
        assert!(cx.has_text(text), "{text}: {:?}", cx.texts());
    }
}

fn click(cx: &mut TestAppContext, id: &str) {
    cx.simulate_click(id);
    cx.run_until_parked();
}

/// A change to valset's code the programs role holds for height 4,500,
/// made by a vote.
fn valset_upgrade() -> programs::Pending {
    programs::Pending {
        height: 4500,
        change: programs::Change::Set(programs::Entry {
            program: "valset".into(),
            code: abi::BlobId::Sha256([0x7e; 32]),
            params: b"params".to_vec(),
        }),
        by: None,
    }
}

/// A reader who votes (one voice of the two a motion needs), with a
/// promotion of the resident waiting on OTHER's vote, so the reader's own
/// vote enacts it, and [`valset_upgrade`] scheduled.
fn voter() -> TestAppContext {
    let waiting = Pending {
        motion: Motion::Promote {
            key: RESIDENT.to_vec(),
        },
        voters: vec![OTHER.to_vec()],
    };
    let mut cx = sheet(&THIS, 2, vec![waiting], vec![valset_upgrade()]);
    phone(&mut cx);
    cx
}

/// The view on a phone over a node that answers every ask, but the one
/// `serve` answers its own way.
fn phone_where(serve: impl FnOnce(&TestAppContext)) -> TestAppContext {
    let mut cx = TestAppContext::new();
    node(&mut cx);
    respond(&mut cx);
    serve(&cx);
    cx.open::<Nodes>();
    phone(&mut cx);
    cx
}

// The list (Expo `app/(tabs)/nodes/index.tsx`)

#[test]
fn the_list_reads_the_network_its_numbers_and_every_member() {
    let (mut cx, _) = ready();
    phone(&mut cx);
    shows(
        &cx,
        &[
            "Workshop",
            "● In sync · this node abcd",
            "HEIGHT",
            "4,200",
            "last block just now",
            "BLOCK TIME",
            "1,000 ms",
            "Epoch 42 · 1 / 100",
            "Next epoch in 99 blocks",
            "Validators",
            "3",
            "Residents",
            "1",
            "Height: the newest block the validator voted to finalize. Quiet: none for 20 blocks.",
        ],
    );
    assert!(cx.find("nodes-table").is_none(), "the desktop's table");
    assert_eq!(
        texts_of(&cx, "nodes-member-0"),
        ["abcd", "10.0.0.1:4000 · this node", "In sync"]
    );
    assert_eq!(
        texts_of(&cx, "nodes-member-1"),
        ["9c1e", "10.0.0.2:4000", "In sync"]
    );
    assert_eq!(texts_of(&cx, "nodes-member-2"), ["5f0a", "—", "Quiet"]);
    assert_eq!(
        texts_of(&cx, "nodes-member-3"),
        ["0102", "10.0.0.9:4000"],
        "a resident's state is not reported"
    );
    assert_eq!(cx.back_handler(), None, "nothing to go back to");
}

#[test]
fn a_reader_who_does_not_vote_has_no_door_and_no_vote() {
    let (mut cx, _) = ready();
    phone(&mut cx);
    assert!(cx.find("nodes-door").is_none());
    assert!(!cx.has_text("Motions") && !cx.has_text("Scheduled changes"));
}

#[test]
fn the_numbers_wait_for_the_node_and_name_no_network_yet() {
    let cx = phone_where(|cx| cx.host().never::<ChainStatus>());
    shows(&cx, &["Nodes", "Reading node status…"]);
    assert!(
        cx.find("nodes-member-0").is_none(),
        "the rows read the node's height"
    );
}

#[test]
fn a_refused_status_says_why_and_retry_reads_it_again() {
    let mut cx = phone_where(|cx| {
        cx.host()
            .refuse::<ChainStatus>("unavailable", "The node is unavailable. Try again.")
    });
    shows(&cx, &["The node is unavailable. Try again.", "Retry"]);
    cx.host().handle::<ChainStatus>(|()| Ok(status()));
    click(&mut cx, "nodes-status-retry");
    assert!(cx.has_text("4,200") && cx.find("nodes-status-refused").is_none());
}

#[test]
fn the_members_wait_for_valset() {
    let cx = phone_where(|cx| cx.host().never::<Query<Valset>>());
    shows(&cx, &["Reading the members…"]);
}

#[test]
fn a_refused_member_read_says_why_and_retry_reads_it_again() {
    let mut cx = phone_where(|cx| {
        cx.host()
            .refuse::<Query<Valset>>("unavailable", "valset is not running here")
    });
    shows(&cx, &["valset is not running here", "Retry"]);
    respond(&mut cx);
    click(&mut cx, "nodes-retry");
    assert!(cx.find("nodes-member-0").is_some(), "{:?}", cx.texts());
}

#[test]
fn a_network_with_no_members_says_so() {
    let cx = phone_where(|cx| serve_set(cx, Vec::new(), Vec::new()));
    shows(
        &cx,
        &["No members", "The validator set of this network is empty."],
    );
}

#[test]
fn a_silent_node_says_so_and_when_it_last_answered() {
    let (mut cx, ticks) = ready();
    phone(&mut cx);
    cx.host().never::<ChainStatus>();
    for _ in 0..4 {
        ticks.send(());
    }
    cx.run_until_parked();
    shows(
        &cx,
        &["● Not answering · this node abcd", "Last answer 4s ago"],
    );
    click(&mut cx, "nodes-member-0");
    shows(&cx, &["Last answer 4s ago"]);
}

#[test]
fn a_member_row_opens_its_page_and_back_returns_to_the_list() {
    let (mut cx, _) = ready();
    phone(&mut cx);
    click(&mut cx, "nodes-member-0");
    assert!(cx.find("nodes-page").is_none(), "the list gave way");
    assert!(cx.back_handler().is_some(), "Back has somewhere to go");
    cx.simulate_back();
    cx.run_until_parked();
    assert!(cx.find("nodes-node").is_none() && cx.find("nodes-page").is_some());
    assert_eq!(cx.back_handler(), None);
    click(&mut cx, "nodes-member-3");
    click(&mut cx, "nodes-node-back");
    assert!(cx.find("nodes-page").is_some(), "the page's own Back");
}

#[test]
fn a_voter_reads_the_motions_that_wait_and_the_changes_scheduled() {
    let cx = voter();
    shows(
        &cx,
        &[
            "Motions",
            "Promote 0102 to validator",
            "1 of 2 · yours has not voted",
            "Vote",
            "Scheduled changes",
            "by vote · in 300 blocks",
            "Propose cancel",
            "Change a program in Explorer, Programs.",
        ],
    );
}

#[test]
fn a_voter_with_nothing_waiting_reads_no_motion_and_no_change() {
    let mut cx = sheet(&THIS, 2, Vec::new(), Vec::new());
    phone(&mut cx);
    shows(
        &cx,
        &["No motion waits for votes.", "No change is scheduled."],
    );
}

#[test]
fn the_door_opens_as_a_sheet_and_back_closes_it_before_the_list() {
    let mut cx = voter();
    click(&mut cx, "nodes-door");
    shows(
        &cx,
        &[
            "Open the door",
            "anyone enrolls without an invite",
            "Close the door",
            "enrolling takes an invite",
        ],
    );
    assert!(cx.back_handler().is_some());
    cx.simulate_back();
    cx.run_until_parked();
    assert!(cx.find("nodes-door-sheet").is_none() && cx.find("nodes-page").is_some());
    assert_eq!(cx.back_handler(), None);
}

#[test]
fn a_door_row_closes_the_sheet_and_votes_its_motion() {
    let mut cx = voter();
    click(&mut cx, "nodes-door");
    click(&mut cx, "nodes-door-open");
    assert!(cx.find("nodes-door-sheet").is_none());
    assert_eq!(submitted(&cx), [Op::Vote(Motion::Door { open: true })]);
    shows(&cx, &["Vote counted: 1 of 2"]);
}

#[test]
fn a_vote_that_enacts_asks_first_and_not_now_sends_nothing() {
    let mut cx = voter();
    click(&mut cx, "nodes-motion-vote-0");
    shows(
        &cx,
        &[
            "Your vote reaches the quorum: this happens now, and a vote can't be taken back.",
            "Not now",
            "Vote and enact",
            "This phone's key signs it. The node runs the program's rules; a refusal comes back in its words.",
        ],
    );
    click(&mut cx, "nodes-confirm/cancel");
    assert!(cx.find("nodes-confirm-sheet").is_none() && submitted(&cx).is_empty());
    click(&mut cx, "nodes-motion-vote-0");
    cx.simulate_dismiss("nodes-confirm-sheet");
    cx.run_until_parked();
    assert!(cx.find("nodes-confirm-sheet").is_none() && submitted(&cx).is_empty());
}

#[test]
fn a_vote_that_enacts_goes_on_yes_and_its_outcome_stays_until_dismissed() {
    let mut cx = voter();
    cx.host()
        .handle::<Submit<Valset>>(|_| Ok(testing::applied(abi::encode(&Voted::Enacted))));
    click(&mut cx, "nodes-motion-vote-0");
    click(&mut cx, "nodes-confirm/submit");
    let promote = Motion::Promote {
        key: RESIDENT.to_vec(),
    };
    assert_eq!(submitted(&cx), [Op::Vote(promote)]);
    assert!(cx.has_text("Enacted") && cx.find("nodes-outcome").is_some());
    click(&mut cx, "nodes-outcome-dismiss");
    assert!(cx.find("nodes-outcome").is_none());
}

#[test]
fn a_refused_vote_says_why_on_the_outcome_strip() {
    let mut cx = voter();
    cx.host()
        .refuse::<Submit<Valset>>("unauthorized", "only a validator votes");
    click(&mut cx, "nodes-door");
    click(&mut cx, "nodes-door-close");
    shows(
        &cx,
        &["That didn’t go through: only a validator votes", "Dismiss"],
    );
}

#[test]
fn propose_cancel_votes_to_cancel_that_change() {
    let mut cx = voter();
    click(&mut cx, "nodes-scheduled-cancel-0");
    assert_eq!(submitted(&cx), [Op::Vote(cancel(&valset_upgrade()))]);
}

// A member's page (Expo `app/(tabs)/nodes/[key].tsx`)

#[test]
fn a_validator_page_reads_its_key_address_vote_and_the_blocks_it_led() {
    let (mut cx, _) = ready();
    phone(&mut cx);
    click(&mut cx, "nodes-member-0");
    shows(
        &cx,
        &[
            "abcd",
            "validator 1",
            "Key",
            "Address",
            "10.0.0.1:4000 · this node",
            "Height",
            "4,200 voted",
            "Behind",
            "0",
            "Status",
            "In sync",
            "Proposed",
            "last 64 blocks · 4,137 → 4,200",
        ],
    );
    assert!(cx.find("nodes-node-strip").is_some());
}

#[test]
fn a_resident_page_reads_no_vote_and_says_it_does_not_propose() {
    let (mut cx, _) = ready();
    phone(&mut cx);
    click(&mut cx, "nodes-member-3");
    shows(&cx, &["resident", "Doesn't propose"]);
    assert!(!cx.has_text("Behind") && cx.find("nodes-node-strip").is_none());
}

#[test]
fn a_page_waits_for_the_members_it_reads() {
    let mut cx = TestAppContext::new();
    node(&mut cx);
    cx.host().never::<Query<Valset>>();
    let nodes = cx.open::<Nodes>();
    phone(&mut cx);
    cx.update(&nodes, |view, _, cx| {
        view.open = Some(THIS.to_vec());
        cx.notify();
    });
    cx.run_until_parked();
    shows(&cx, &["Node", "Reading the members…"]);
}

#[test]
fn a_page_on_a_key_that_left_reads_not_a_member() {
    let mut cx = TestAppContext::new();
    node(&mut cx);
    respond(&mut cx);
    let nodes = cx.open::<Nodes>();
    phone(&mut cx);
    click(&mut cx, "nodes-member-3");
    serve_set(
        &cx,
        vec![THIS.to_vec()],
        vec![membership(&THIS, "10.0.0.1:4000", valset::Role::Validator)],
    );
    cx.update(&nodes, |view, _, cx| view.read(cx));
    cx.run_until_parked();
    shows(
        &cx,
        &[
            "Node",
            "Not a member",
            "This key holds no seat on the network.",
        ],
    );
}

#[test]
fn a_voter_demotes_or_removes_a_validator_from_its_page() {
    let mut cx = voter();
    click(&mut cx, "nodes-member-1");
    assert!(cx.find("nodes-node-promote").is_none());
    click(&mut cx, "nodes-node-demote");
    click(&mut cx, "nodes-node-remove");
    let key = OTHER.to_vec();
    assert_eq!(
        submitted(&cx),
        [
            Op::Vote(Motion::Demote { key: key.clone() }),
            Op::Vote(Motion::Remove { key })
        ]
    );
}

#[test]
fn a_voter_promotes_a_resident_from_its_page() {
    let mut cx = voter();
    click(&mut cx, "nodes-member-3");
    assert!(cx.find("nodes-node-demote").is_none());
    click(&mut cx, "nodes-node-promote");
    // OTHER voted for it already: the reader's vote enacts, so it asks
    click(&mut cx, "nodes-confirm/submit");
    let promote = Motion::Promote {
        key: RESIDENT.to_vec(),
    };
    assert_eq!(submitted(&cx), [Op::Vote(promote)]);
}

// Both screens

/// Under a finger at 360, 390 and 430 every frame fits and every press is
/// at least 44 each way, through the list, the door, the confirm sheet and
/// both kinds of member page; Back is set exactly while there is somewhere
/// to go back to.
#[test]
fn every_screen_fits_a_phone_360_390_and_430_wide() {
    let mut cx = voter();
    testing::sweep(&mut cx, |cx| {
        assert_eq!(cx.back_handler(), None, "nothing to go back to");
        assert!(cx.find("nodes-member-0").is_some(), "the members");
        assert!(cx.find("nodes-motion-vote-0").is_some(), "Vote");
        assert!(
            cx.find("nodes-scheduled-cancel-0").is_some(),
            "Propose cancel"
        );
        click(cx, "nodes-door");
        assert!(cx.find("nodes-door-sheet").is_some(), "the door sheet");
        cx.simulate_back();
        cx.run_until_parked();
        click(cx, "nodes-motion-vote-0");
        assert!(
            cx.find("nodes-confirm-sheet").is_some(),
            "the confirm sheet"
        );
        cx.simulate_back();
        cx.run_until_parked();
        assert!(cx.find("nodes-confirm-sheet").is_none());
        click(cx, "nodes-member-0");
        assert!(cx.find("nodes-node-remove").is_some(), "a validator's page");
        cx.simulate_back();
        cx.run_until_parked();
        click(cx, "nodes-member-3");
        assert!(cx.find("nodes-node-promote").is_some(), "a resident's page");
        cx.simulate_back();
        cx.run_until_parked();
        assert!(cx.find("nodes-page").is_some(), "back on the list");
    });
}

#[test]
fn a_vote_says_what_it_came_to_in_the_expo_words() {
    let applied = |voted: Voted| Ok(testing::applied(abi::encode(&voted)));
    assert_eq!(said(applied(Voted::Enacted)), Ok("Enacted".into()));
    assert_eq!(
        said(applied(Voted::Counted {
            votes: 1,
            needed: 2
        })),
        Ok("Vote counted: 1 of 2".into())
    );
    let refusal = |code: &str, message: &str| Error::new(code, message);
    assert_eq!(
        said(Ok(Submitted::Refused {
            frame: [0; 32],
            until: 0,
            refusal: refusal("unauthorized", "only a validator votes"),
        })),
        Err("That didn’t go through: only a validator votes".into())
    );
    assert_eq!(
        said(Err(refusal(
            methods::refusal::RPC_CLIENT,
            "connection refused"
        ))),
        Err("Not sent: the node can’t be reached. Check the connection, then try again.".into())
    );
    assert_eq!(
        said(Err(refusal(
            methods::refusal::SESSION_LOCKED,
            "No key is unlocked."
        ))),
        Err("Not sent: No key is unlocked.".into())
    );
    assert_eq!(
        said(Ok(Submitted::InDoubt {
            frame: [0; 32],
            until: 0
        })),
        Err(
            "This may have gone through: no answer came back. What you see is read again from the node."
                .into()
        )
    );
}
