//! The vote on the sheet: who operates a node, the motions on each row
//! and the door for a reader whose key votes, a press that enacts asked
//! first, the motions that wait, and the scheduled changes and who made
//! each; the words for each motion.
use super::*;
use crate::queries::Names;
use crate::voting::words;
use ducktape_view_guest::methods::{HostSession, Session, Submit};
use module_registry::{Change, Entry, Grant, Modules, Scheduled, Scope};
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
    scheduled: Vec<module_registry::Pending>,
) -> TestAppContext {
    let mut cx = TestAppContext::new();
    node(&mut cx);
    sheet_over(cx, signer, needed, pending, scheduled)
}

/// [`sheet`] over a node the test seated.
fn sheet_over(
    mut cx: TestAppContext,
    signer: &[u8],
    needed: u64,
    pending: Vec<Pending>,
    scheduled: Vec<module_registry::Pending>,
) -> TestAppContext {
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
            runner: None,
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
        "nodes-scheduled",
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
        "nodes-scheduled",
    ] {
        assert!(cx.find(id).is_some(), "{id}: {:?}", cx.texts());
    }
    // a module is changed in Explorer: no form of its own here
    assert!(cx.find("nodes-change").is_none() && cx.find("nodes-change-code").is_none());
    assert!(cx.has_text("Change a module in Explorer, Programs."));
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
fn a_scheduled_change_says_who_made_it_and_is_cancelled_by_a_vote() {
    let ahead = module_registry::Pending {
        height: 4300,
        change: Change::Set(entry("valset", CODE)),
        by: None,
    };
    let landed = module_registry::Pending {
        height: 4100,
        change: Change::Remove("chat".into()),
        by: None,
    };
    let granted = module_registry::Pending {
        height: 4800,
        change: Change::Set(entry("chat", CODE)),
        by: Some(12),
    };
    let mut cx = sheet(&THIS, 2, Vec::new(), vec![landed, ahead, granted]);
    assert_eq!(
        texts_of(&cx, "nodes-scheduled-0"),
        [
            "Set valset to code sha256:7e7e7e7e…7e7e at block 4,300 · params unchanged",
            "by vote · in 100 blocks",
            "Propose cancel"
        ]
    );
    // identity names no account 12 here
    assert_eq!(
        texts_of(&cx, "nodes-scheduled-1")[1],
        "by account 12 · validators can cancel for 10 min"
    );
    assert!(cx.find("nodes-scheduled-2").is_none());
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

/// While the node's status is unread, a scheduled change tells who made it
/// and no wait: no "0 min", no count from block 0.
#[test]
fn a_scheduled_change_tells_no_wait_before_the_status_is_read() {
    let changes = vec![
        module_registry::Pending {
            height: 4300,
            change: Change::Set(entry("valset", CODE)),
            by: None,
        },
        module_registry::Pending {
            height: 4800,
            change: Change::Set(entry("chat", CODE)),
            by: Some(12),
        },
    ];
    let mut cx = TestAppContext::new();
    node(&mut cx);
    cx.host().never::<ChainStatus>();
    let cx = sheet_over(cx, &THIS, 2, Vec::new(), changes);
    assert_eq!(texts_of(&cx, "nodes-scheduled-0")[1], "by vote");
    assert_eq!(texts_of(&cx, "nodes-scheduled-1")[1], "by account 12");
}

/// A grant to vote on reads with its account's name, as identity says it.
#[test]
fn a_grant_motion_names_its_account() {
    let op = module_registry::Op::Grant(Grant {
        scope: Scope::Module("trade".into()),
        account: 12,
    });
    let waiting = Pending {
        motion: Motion::Registry {
            op: abi::encode(&op),
        },
        voters: vec![OTHER.to_vec()],
    };
    let carol = identity::Profile {
        number: 12,
        name: "carol".into(),
        kind: identity::Kind::Person,
    };
    let mut cx = TestAppContext::new();
    node(&mut cx);
    serve_identity(&cx, Vec::new(), vec![carol]);
    let cx = sheet_over(cx, &THIS, 2, vec![waiting], Vec::new());
    assert_eq!(texts_of(&cx, "nodes-motion-0")[0], "Grant carol trade");
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
        (
            grant(false, Scope::Module("trade".into()), 12),
            "Grant carol trade",
        ),
        (
            grant(true, Scope::Module("chat".into()), 13),
            "Revoke release-bot chat",
        ),
        (
            grant(true, Scope::All, 13),
            "Revoke release-bot every module",
        ),
        (
            grant(false, Scope::All, 99),
            "Grant account 99 every module",
        ),
    ];
    let names = Names::from([(12, "carol".into()), (13, "release-bot".into())]);
    for (motion, said) in cases {
        assert_eq!(words(&motion, &roster, &names), said);
    }
}

fn grant(revoke: bool, scope: Scope, account: u64) -> Motion {
    let grant = Grant { scope, account };
    let op = match revoke {
        false => module_registry::Op::Grant(grant),
        true => module_registry::Op::Revoke(grant),
    };
    Motion::Registry {
        op: abi::encode(&op),
    }
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
