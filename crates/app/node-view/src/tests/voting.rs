//! The vote on the sheet: who operates a node, the motions on each row
//! and the door for a reader whose key votes, a press that enacts asked
//! first, the motions that wait, Change a module and the scheduled
//! changes; the words for each motion.
use super::*;
use crate::voting::{change, code_id, default_height, words};
use ducktape_view_guest::methods::{HostSession, Session, Submit};
use module_registry::{Change, Entry, Modules, Scheduled};
use valset::{Motion, Op, Pending, Voted};

const CODE: [u8; 32] = [0x7e; 32];
const NOW: [u8; 32] = [0x11; 32];

fn entry(program: &str, code: [u8; 32]) -> Entry {
    Entry {
        program: program.into(),
        code: abi::BlobId::Sha256(code),
        params: b"params".to_vec(),
    }
}

/// What the registry runs: chat and valset, on code NOW.
fn roster() -> Vec<Entry> {
    vec![entry("chat", NOW), entry("valset", NOW)]
}

/// valset at its new code at `height`.
fn set_valset(height: u64) -> Motion {
    let op = module_registry::Op::Schedule(Scheduled {
        height,
        change: Change::Set(entry("valset", CODE)),
    });
    Motion::Registry {
        op: abi::encode(&op),
    }
}

/// The sheet for a reader whose seated key is `signer`: valset answering
/// the set, the reader's voices (THIS votes for itself), and the motions
/// that wait; the registry its roster and `scheduled`; a vote counted.
fn sheet(
    signer: &[u8],
    needed: u64,
    pending: Vec<Pending>,
    scheduled: Vec<Scheduled>,
) -> TestAppContext {
    let mut cx = TestAppContext::new();
    node(&mut cx);
    let members = vec![
        membership(&RESIDENT, "10.0.0.9:4000", valset::Role::Resident),
        membership(&OTHER, "10.0.0.2:4000", valset::Role::Validator),
        membership(&THIS, "10.0.0.1:4000", valset::Role::Validator),
    ];
    cx.host().handle::<Query<Valset>>(move |query| {
        Ok(match query {
            valset::Query::Validators => {
                valset::Reply::Validators(vec![THIS.to_vec(), OTHER.to_vec(), UNLISTED.to_vec()])
            }
            valset::Query::Memberships { .. } => valset::Reply::Memberships(page(members.clone())),
            valset::Query::Voices { key } => valset::Reply::Voices(match key == THIS {
                true => vec![THIS.to_vec()],
                false => Vec::new(),
            }),
            valset::Query::Motions { .. } => valset::Reply::Motions {
                needed,
                page: page(pending.clone()),
            },
            other => panic!("unexpected query: {other:?}"),
        })
    });
    cx.host().handle::<Query<Modules>>(move |query| {
        Ok(match query {
            module_registry::Query::Program(_) => module_registry::Reply::Program {
                height: 4200,
                entry: None,
            },
            module_registry::Query::At(4200) => module_registry::Reply::Programs(roster()),
            module_registry::Query::Scheduled { .. } => {
                module_registry::Reply::Scheduled(page(scheduled.clone()))
            }
            other => panic!("unexpected query: {other:?}"),
        })
    });
    cx.host().handle::<Submit<Valset>>(move |_| {
        Ok(abi::encode(&Voted::Counted {
            votes: 1,
            needed: needed.max(2),
        }))
    });
    let session = cx.host().stream::<HostSession>();
    cx.open::<Nodes>();
    session.send(Session {
        connected: true,
        chain_id: "Workshop".into(),
        signer: abi::hex(signer),
        account: Some(1),
        endpoint: String::new(),
    });
    cx.run_until_parked();
    cx
}

fn submitted(cx: &TestAppContext) -> Vec<Op> {
    cx.host().requests::<Submit<Valset>>()
}

/// A key on an agent reads who operates it; a key on no account, or on an
/// agent that does not act now (refused `unauthorized`), reads as before.
#[test]
fn a_row_says_who_operates_a_node_on_an_agent() {
    let mut cx = TestAppContext::new();
    node(&mut cx);
    respond(&mut cx);
    let agent = identity::Profile {
        number: 11,
        name: "validator vps".into(),
        kind: identity::Kind::Managed {
            manager: 10,
            category: identity::Category::Agent,
            standing: identity::Standing::Active,
        },
    };
    let ada = identity::Profile {
        number: 10,
        name: "ada".into(),
        kind: identity::Kind::Person,
    };
    cx.host()
        .handle::<Query<identity::Identity>>(move |query| match query {
            identity::Query::OfKey { key } if key == UNLISTED => {
                Err(Error::new("unauthorized", "account 12 is suspended"))
            }
            identity::Query::OfKey { key } => {
                Ok(identity::Reply::Number((key == OTHER).then_some(11)))
            }
            identity::Query::Profile { number } => Ok(identity::Reply::Profile(
                [agent.clone(), ada.clone()]
                    .into_iter()
                    .find(|profile| profile.number == number),
            )),
            other => panic!("unexpected query: {other:?}"),
        });
    cx.open::<Nodes>();
    cx.run_until_parked();
    assert_eq!(
        texts_of(&cx, "nodes-operator-1"),
        ["operated by ada (validator vps)"]
    );
    for row in [0, 2, 3] {
        assert!(cx.find(&format!("nodes-operator-{row}")).is_none(), "{row}");
    }
    assert!(cx.has_text("10.0.0.1:4000"));
}

/// Identity refusing one row's operator is that row's line, with the
/// reason; the list stands.
#[test]
fn a_refused_operator_read_marks_its_row_and_keeps_the_list() {
    let mut cx = TestAppContext::new();
    node(&mut cx);
    respond(&mut cx);
    cx.host()
        .handle::<Query<identity::Identity>>(|query| match query {
            identity::Query::OfKey { key } if key == OTHER => {
                Err(Error::new("unavailable", "identity is restarting"))
            }
            identity::Query::OfKey { .. } => Ok(identity::Reply::Number(None)),
            other => panic!("unexpected query: {other:?}"),
        });
    cx.open::<Nodes>();
    cx.run_until_parked();
    assert_eq!(
        texts_of(&cx, "nodes-operator-1"),
        ["operator unreadable: identity is restarting"]
    );
    for row in [0, 2, 3] {
        assert!(cx.find(&format!("nodes-operator-{row}")).is_none(), "{row}");
    }
    assert!(cx.has_text("10.0.0.1:4000"));
}

#[test]
fn a_reader_who_does_not_vote_gets_no_controls() {
    let cx = sheet(&RESIDENT, 2, Vec::new(), Vec::new());
    for id in [
        "nodes-door",
        "nodes-motions",
        "nodes-change",
        "nodes-promote-3",
    ] {
        assert!(cx.find(id).is_none(), "{id}");
    }
    assert!(cx.host().requests::<Query<Modules>>().is_empty());
}

#[test]
fn a_reader_who_votes_gets_each_rows_motions_and_the_door() {
    let mut cx = sheet(&THIS, 2, Vec::new(), Vec::new());
    for id in [
        "nodes-door-open",
        "nodes-door-close",
        "nodes-demote-0",
        "nodes-remove-1",
        "nodes-promote-3",
        "nodes-motions",
        "nodes-change",
        "nodes-scheduled",
    ] {
        assert!(cx.find(id).is_some(), "{id}: {:?}", cx.texts());
    }
    assert!(cx.find("nodes-promote-0").is_none() && cx.find("nodes-demote-3").is_none());
    assert!(cx.has_text("No motion waits for votes.") && cx.has_text("No change is scheduled."));
    // one vote of two: cast at once
    cx.simulate_click("nodes-promote-3");
    cx.run_until_parked();
    assert_eq!(
        submitted(&cx),
        [Op::Vote(Motion::Promote {
            key: RESIDENT.to_vec()
        })]
    );
    assert!(cx.has_text("Vote counted: 1 of 2"));
    // the vote's reply reads the voices again
    let voices = cx
        .host()
        .requests::<Query<Valset>>()
        .into_iter()
        .filter(|query| matches!(query, valset::Query::Voices { .. }))
        .count();
    assert_eq!(voices, 2);
}

#[test]
fn a_press_that_enacts_says_what_happens_and_asks_once() {
    let mut cx = sheet(&THIS, 1, Vec::new(), Vec::new());
    cx.simulate_click("nodes-door-open");
    cx.run_until_parked();
    assert!(cx.has_text("Open the door: anyone enrolls without an invite"));
    assert!(submitted(&cx).is_empty());
    cx.simulate_click("nodes-confirm-no");
    cx.run_until_parked();
    assert!(cx.find("nodes-confirm").is_none() && submitted(&cx).is_empty());
    cx.host()
        .handle::<Submit<Valset>>(|_| Ok(abi::encode(&Voted::Enacted)));
    cx.simulate_click("nodes-door-open");
    cx.run_until_parked();
    cx.simulate_click("nodes-confirm-yes");
    cx.run_until_parked();
    assert_eq!(submitted(&cx), [Op::Vote(Motion::Door { open: true })]);
    assert!(cx.has_text("Enacted") && cx.find("nodes-confirm").is_none());
}

#[test]
fn a_waiting_motion_reads_decoded_and_is_voted_with_its_own_bytes() {
    let waiting = Pending {
        motion: set_valset(5000),
        voters: vec![OTHER.to_vec()],
    };
    let mut cx = sheet(&THIS, 2, vec![waiting.clone()], Vec::new());
    let words = texts_of(&cx, "nodes-motion-0");
    assert_eq!(
        words,
        [
            "Set valset to code sha256:7e7e7e7e…7e7e at block 5,000 · params unchanged",
            "1 of 2 · yours has not voted",
            "Vote"
        ]
    );
    // THIS's vote makes two of two: asked first
    cx.simulate_click("nodes-motion-vote-0");
    cx.run_until_parked();
    cx.simulate_click("nodes-confirm-yes");
    cx.run_until_parked();
    assert_eq!(submitted(&cx), [Op::Vote(waiting.motion.clone())]);

    let voted = Pending {
        voters: vec![OTHER.to_vec(), THIS.to_vec()],
        ..waiting
    };
    let cx = sheet(&THIS, 3, vec![voted], Vec::new());
    assert_eq!(
        texts_of(&cx, "nodes-motion-0")[1..],
        ["2 of 3 · yours voted"]
    );
}

#[test]
fn change_a_module_votes_a_schedule_of_the_published_code() {
    let mut cx = sheet(&THIS, 2, Vec::new(), Vec::new());
    cx.simulate_click("nodes-change-propose");
    cx.run_until_parked();
    assert!(cx.has_text("Pick a program to change."));
    cx.simulate_click("nodes-change-program-valset");
    cx.run_until_parked();
    // ten minutes of 1 s blocks past 4,200
    assert!(cx.has_text("Runs code sha256:11111111…1111 now; its params stay."));
    cx.simulate_input("nodes-change-code", "sha256:7e");
    cx.simulate_click("nodes-change-propose");
    cx.run_until_parked();
    assert!(cx.has_text("A code id is sha256: and 64 hex digits."));
    cx.simulate_input("nodes-change-code", &format!("sha256:{}", abi::hex(&CODE)));
    cx.simulate_click("nodes-change-propose");
    cx.run_until_parked();
    assert_eq!(submitted(&cx), [Op::Vote(set_valset(4800))]);
}

#[test]
fn a_scheduled_change_still_to_land_is_cancelled_by_a_vote() {
    let ahead = Scheduled {
        height: 4300,
        change: Change::Set(entry("valset", CODE)),
    };
    let landed = Scheduled {
        height: 4100,
        change: Change::Remove("chat".into()),
    };
    let mut cx = sheet(&THIS, 2, Vec::new(), vec![landed, ahead]);
    assert_eq!(
        texts_of(&cx, "nodes-scheduled-0")[..2],
        [
            "Set valset to code sha256:7e7e7e7e…7e7e at block 4,300 · params unchanged",
            "in 100 blocks"
        ]
    );
    assert!(cx.find("nodes-scheduled-1").is_none());
    cx.simulate_click("nodes-scheduled-cancel-0");
    cx.run_until_parked();
    let cancel = module_registry::Op::Cancel {
        height: 4300,
        program: "valset".into(),
    };
    assert_eq!(
        submitted(&cx),
        [Op::Vote(Motion::Registry {
            op: abi::encode(&cancel)
        })]
    );
}

#[test]
fn every_motion_reads_in_words() {
    let roster = roster();
    let key = vec![0xab; 32];
    let cases = [
        (
            Motion::Promote { key: key.clone() },
            "Promote abababab…abab to validator",
        ),
        (
            Motion::Demote { key: key.clone() },
            "Demote abababab…abab to resident",
        ),
        (
            Motion::Remove { key },
            "Remove abababab…abab from the network",
        ),
        (
            Motion::Door { open: false },
            "Close the door: enrolling takes an invite",
        ),
        (
            Motion::Registry {
                op: abi::encode(&module_registry::Op::Cancel {
                    height: 4300,
                    program: "chat".into(),
                }),
            },
            "Cancel chat's change at block 4,300",
        ),
        (
            Motion::Registry {
                op: abi::encode(&module_registry::Op::Schedule(Scheduled {
                    height: 9,
                    change: Change::Set(Entry {
                        params: Vec::new(),
                        ..entry("chat", CODE)
                    }),
                })),
            },
            "Set chat to code sha256:7e7e7e7e…7e7e at block 9 · params changed",
        ),
        (
            Motion::Registry {
                op: abi::encode(&module_registry::Op::Schedule(Scheduled {
                    height: 9,
                    change: Change::Set(entry("forge", CODE)),
                })),
            },
            "Set forge to code sha256:7e7e7e7e…7e7e at block 9 · a new program",
        ),
        (
            Motion::Registry {
                op: b"not an op".to_vec(),
            },
            "A registry change this screen can't read",
        ),
    ];
    for (motion, said) in cases {
        assert_eq!(words(&motion, &roster), said);
    }
}

#[test]
fn a_change_needs_a_program_a_code_and_a_later_block() {
    let code = abi::hex(&CODE);
    let roster = roster();
    let built = |program, code: &str, height| change(program, code, height, 4200, &roster);
    assert!(built(Some("forge"), &code, "4800").is_err());
    assert!(built(Some("valset"), &code, "4,200").is_err());
    assert!(built(Some("valset"), &code, "soon").is_err());
    assert_eq!(built(Some("valset"), &code, "4,800"), Ok(set_valset(4800)));
    assert_eq!(code_id(&code), Some(abi::BlobId::Sha256(CODE)));
    assert_eq!(code_id("sha256:7e"), None);
    assert_eq!(default_height(4200, 2000), 4500);
}

/// Until the motions are read (here they are refused), no press votes: a
/// vote that enacts would go unasked.
#[test]
fn no_press_votes_before_the_motions_are_read() {
    let mut cx = TestAppContext::new();
    node(&mut cx);
    respond_voter(&mut cx);
    cx.host().never::<Query<Modules>>();
    let session = cx.host().stream::<HostSession>();
    cx.open::<Nodes>();
    session.send(Session {
        signer: abi::hex(&THIS),
        ..Session::default()
    });
    cx.run_until_parked();
    assert!(cx.has_text("not yet"));
    let Some(ducktape_view_guest::wire::Node::Container(
        ducktape_view_guest::wire::ContainerNode {
            interactivity: Some(interactivity),
            ..
        },
    )) = cx.find("nodes-door-open")
    else {
        panic!("the door's button")
    };
    assert_eq!(interactivity.aria.disabled, Some(true));
    assert!(interactivity.on_click.is_none());
}

/// valset for a reader whose key THIS votes, its motions refused.
fn respond_voter(cx: &mut TestAppContext) {
    cx.host().handle::<Query<Valset>>(move |query| {
        Ok(match query {
            valset::Query::Validators => valset::Reply::Validators(vec![THIS.to_vec()]),
            valset::Query::Memberships { .. } => valset::Reply::Memberships(page(vec![])),
            valset::Query::Voices { .. } => valset::Reply::Voices(vec![THIS.to_vec()]),
            valset::Query::Motions { .. } => {
                return Err(Error::new("unavailable", "not yet"));
            }
            other => panic!("unexpected query: {other:?}"),
        })
    });
}
