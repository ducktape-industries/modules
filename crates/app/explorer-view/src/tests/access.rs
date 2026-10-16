//! What a reader changes on the Programs page: by its grant, the op goes to
//! the programs role; by its voices, a motion goes to valset; with neither,
//! nothing is pressable. The ops are read as the host receives them:
//! `requests::<Submit<P>>` answers only an `op.submit` addressed to `P`,
//! decoded from the bytes the view sent.
use super::*;
use crate::acting::{self, Edit, Form, Path};
use programs::{Change, Grant, Pending, Scope};
use valset::{Motion, Voted};
use view::methods::Submit;

/// Ada (account 3) maintains chat; Scout, her agent (5), every program.
pub(super) fn grants() -> Vec<Grant> {
    vec![
        Grant {
            scope: Scope::Program("chat".into()),
            account: 3,
        },
        Grant {
            scope: Scope::All,
            account: 5,
        },
    ]
}

/// A change waiting: chat's at 700, made by Ada's grant, and forge's
/// removal at 120, made by the vote.
pub(super) fn waiting() -> Vec<Pending> {
    vec![
        Pending {
            height: 120,
            change: Change::Remove("forge".into()),
            by: None,
        },
        Pending {
            height: 700,
            change: Change::Set(entry("chat", 0x77)),
            by: Some(3),
        },
    ]
}

const CODE: [u8; 32] = [0x7e; 32];

/// The Programs page for a reader whose key is `signer`, acting as
/// `account`, voting for the validator when `votes`; the programs role holding
/// `grants` and `scheduled`, chat a hook-free program, identity a role's.
pub(super) fn reader(
    account: Option<u64>,
    signer: [u8; 32],
    votes: bool,
    grants: Vec<Grant>,
    scheduled: Vec<Pending>,
) -> TestAppContext {
    reader_on(account, signer, votes, grants, scheduled, Vec::new(), 1000)
}

/// [`reader`] on a node whose blocks come every `block_ms`, the programs role
/// listing `hooks`.
fn reader_on(
    account: Option<u64>,
    signer: [u8; 32],
    votes: bool,
    grants: Vec<Grant>,
    scheduled: Vec<Pending>,
    hooks: Vec<String>,
    block_ms: u64,
) -> TestAppContext {
    let mut cx = TestAppContext::new();
    node(&mut cx, Rc::new(RefCell::new(12)));
    cx.host().handle::<ChainStatus>(move |()| {
        Ok(NodeStatus {
            block_time_ms: block_ms,
            ..status(12)
        })
    });
    programs_granting(
        &cx,
        vec![entry("chat", 0xab), entry("identity", 0xcd)],
        vec![programs::View {
            name: "explorer".into(),
            view: BlobId::Sha256([0xef; 32]),
        }],
        scheduled,
        grants,
        hooks,
    );
    cx.host().handle::<Query<Valset>>(move |query| {
        Ok(match query {
            valset::Query::Voices { .. } => valset::Reply::Voices(if votes {
                vec![VALIDATOR.to_vec()]
            } else {
                Vec::new()
            }),
            _ => valset::Reply::Validators(vec![VALIDATOR.to_vec()]),
        })
    });
    cx.host()
        .handle::<Submit<Programs>>(|_| Ok(view::testing::applied(Vec::new())));
    cx.host().handle::<Submit<Valset>>(|_| {
        Ok(view::testing::applied(abi::encode(&Voted::Counted {
            votes: 1,
            needed: 2,
        })))
    });
    let session = cx.host().stream::<HostSession>();
    cx.open::<Explorer>();
    session.send(Session {
        connected: true,
        chain_id: "test#1".into(),
        signer: abi::hex(&signer),
        account,
        endpoint: String::new(),
    });
    cx.run_until_parked();
    cx.simulate_click("explorer-tab-programs");
    cx.run_until_parked();
    cx
}

fn maintainer() -> TestAppContext {
    reader(Some(3), ADA, false, grants(), waiting())
}

fn upgrade(height: u64) -> programs::Op {
    programs::Op::Schedule(programs::Scheduled {
        height,
        change: Change::Set(programs::Entry {
            code: BlobId::Sha256(CODE),
            ..entry("chat", 0xab)
        }),
    })
}

/// The headline: a maintainer's Upgrade lands in the programs role with no vote,
/// its params kept, at the tip + `NOTICE` + slack.
#[test]
fn a_maintainers_upgrade_goes_to_the_programs_as_it_was_typed() {
    let mut cx = maintainer();
    cx.simulate_click("explorer-upgrade-chat");
    cx.run_until_parked();
    assert!(
        cx.find("explorer-programs-list").is_some(),
        "the press opened no transactions"
    );
    assert!(cx.has_text("Upgrade chat"), "{:?}", cx.texts());
    assert!(cx.has_text("Close"), "the form closes; Cancel is a row's");
    assert!(cx.has_text("Params kept: 3 bytes, as chat runs now."));
    let earliest = 12 + 1 + programs::NOTICE;
    assert!(cx.has_text(&format!(
        "Block {earliest} or later: the validators get 60 blocks to look, and can cancel it by vote until it lands."
    )));
    cx.simulate_input("explorer-form-code", &format!("sha256:{}", abi::hex(&CODE)));
    cx.simulate_click("explorer-form-submit");
    cx.run_until_parked();
    let sent = cx.host().requests::<Submit<Programs>>();
    let height = 12 + programs::NOTICE + 30;
    assert_eq!(sent, [upgrade(height)]);
    assert_eq!(abi::encode(&sent[0]), abi::encode(&upgrade(height)));
    assert!(cx.host().requests::<Submit<Valset>>().is_empty());
    assert!(cx.find("explorer-form").is_none(), "the form closes");
    assert!(cx.has_text("Sent to the programs role: Schedule · chat."));
}

#[test]
fn a_reader_with_voices_and_no_grant_proposes_the_same_change_to_the_vote() {
    let mut cx = reader(Some(3), ADA, true, Vec::new(), waiting());
    assert!(cx.has_text("Propose upgrade") && cx.has_text("Propose remove"));
    // a role's program is never removed
    assert!(cx.find("explorer-upgrade-identity").is_some());
    assert!(cx.find("explorer-remove-identity").is_none());
    cx.simulate_click("explorer-upgrade-chat");
    cx.run_until_parked();
    assert!(cx.has_text("Propose upgrade · chat"));
    cx.simulate_input("explorer-form-code", &abi::hex(&CODE));
    cx.simulate_click("explorer-form-submit");
    cx.run_until_parked();
    // about ten minutes at the node's 1 s blocks
    let op = abi::encode(&upgrade(12 + 600));
    assert_eq!(
        cx.host().requests::<Submit<Valset>>(),
        [valset::Op::Vote(Motion::Role {
            role: abi::role::PROGRAMS.into(),
            op,
        })]
    );
    assert!(cx.host().requests::<Submit<Programs>>().is_empty());
    assert!(cx.has_text("Vote counted, 1 of 2: Schedule · chat."));
}

#[test]
fn a_reader_with_neither_a_grant_nor_voices_sees_who_and_presses_nothing() {
    let cx = reader(Some(6), STRANGER, false, grants(), waiting());
    for press in [
        "explorer-upgrade-chat",
        "explorer-remove-chat",
        "explorer-upgrade-identity",
        "explorer-upgrade-explorer",
        "explorer-add",
        "explorer-cancel-0",
        "explorer-cancel-1",
        "explorer-propose-grant",
        "explorer-revoke-0",
        "explorer-revoke-1",
    ] {
        assert!(cx.find(press).is_none(), "{press}");
    }
    assert!(cx.has_text("validators' vote"), "identity is a role's");
    assert!(cx.has_text("by vote") && cx.has_text("by Ada"));
}

#[test]
fn access_lists_every_grant_by_name_and_the_reader_steps_down_from_its_own() {
    let mut cx = maintainer();
    assert!(cx.has_text("Access"));
    assert!(cx.has_text("changes by a maintainer wait 60 blocks (about 1 min)"));
    assert!(cx.has_text("every program") && cx.has_text("Scout"));
    assert!(cx.has_text("agent"), "Scout is an agent");
    assert!(cx.has_text("Ada (you)"));
    let every = cx
        .texts()
        .iter()
        .filter(|text| *text == "every-program maintainers")
        .count();
    assert_eq!(
        every, 2,
        "beside chat's own maintainer, and explorer's none"
    );
    cx.simulate_click("explorer-revoke-0");
    cx.run_until_parked();
    assert_eq!(
        cx.host().requests::<Submit<Programs>>(),
        [programs::Op::Revoke(grants()[0].clone())]
    );
}

/// Minutes are the node's: 60 blocks at 2 s are about 2 min.
#[test]
fn the_waits_read_in_minutes_at_the_nodes_block_time() {
    let cx = reader_on(Some(3), ADA, false, grants(), waiting(), Vec::new(), 2000);
    assert!(
        cx.has_text("changes by a maintainer wait 60 blocks (about 2 min)"),
        "{:?}",
        cx.texts()
    );
    // 688 blocks at 2 s
    assert!(cx.has_text("validators can cancel for 23 min"));
}

/// A hook's code changes by grant; its removal is the vote's, which a
/// maintainer with voices proposes and one without never sees.
#[test]
fn a_hook_is_upgraded_by_its_maintainer_and_removed_only_by_vote() {
    let hooked = || vec!["chat".to_owned()];
    let cx = reader_on(Some(3), ADA, false, grants(), waiting(), hooked(), 1000);
    assert!(cx.find("explorer-hook-chat").is_some());
    assert!(cx.find("explorer-upgrade-chat").is_some());
    assert!(cx.find("explorer-remove-chat").is_none());
    let mut cx = reader_on(Some(3), ADA, true, grants(), waiting(), hooked(), 1000);
    assert!(cx.has_text("Upgrade") && cx.has_text("Propose remove"));
    cx.simulate_click("explorer-remove-chat");
    cx.run_until_parked();
    assert!(cx.has_text("Propose remove · chat"), "{:?}", cx.texts());
    cx.simulate_click("explorer-form-submit");
    cx.run_until_parked();
    let op = abi::encode(&programs::Op::Schedule(programs::Scheduled {
        height: 12 + 600,
        change: Change::Remove("chat".into()),
    }));
    assert_eq!(
        cx.host().requests::<Submit<Valset>>(),
        [valset::Op::Vote(Motion::Role {
            role: abi::role::PROGRAMS.into(),
            op,
        })]
    );
    assert!(cx.host().requests::<Submit<Programs>>().is_empty());
}

#[test]
fn a_voter_proposes_a_grant_naming_the_account_as_it_is_typed() {
    let mut cx = reader(None, VALIDATOR, true, Vec::new(), Vec::new());
    assert!(cx.has_text(
        "No account maintains a program: only the validators' vote changes the programs role."
    ));
    cx.simulate_click("explorer-propose-grant");
    cx.run_until_parked();
    cx.simulate_input("explorer-form-account", "5");
    cx.run_until_parked();
    assert!(
        cx.has_text("#5 · Scout · Agent · managed by Ada · suspended"),
        "{:?}",
        cx.texts()
    );
    cx.simulate_input("explorer-form-program", "trade");
    cx.simulate_click("explorer-form-submit");
    cx.run_until_parked();
    let op = abi::encode(&programs::Op::Grant(Grant {
        scope: Scope::Program("trade".into()),
        account: 5,
    }));
    assert_eq!(
        cx.host().requests::<Submit<Valset>>(),
        [valset::Op::Vote(Motion::Role {
            role: abi::role::PROGRAMS.into(),
            op,
        })]
    );
}

#[test]
fn a_refusal_shows_in_the_form_in_the_hosts_words() {
    let mut cx = maintainer();
    let refusal = "account 3 may not change chat: no grant, and not the validators' vote";
    cx.host()
        .refuse::<Submit<Programs>>("unauthorized", refusal);
    cx.simulate_click("explorer-remove-chat");
    cx.run_until_parked();
    cx.simulate_click("explorer-form-submit");
    cx.run_until_parked();
    assert!(
        cx.find("explorer-form-problem").is_some(),
        "the form says why"
    );
    assert!(cx.find("explorer-note").is_none(), "and nothing else does");
    assert!(
        cx.has_text(&format!("Not sent: {refusal}")),
        "{:?}",
        cx.texts()
    );
    // pressed again, the old refusal goes at the press, not when the
    // answer lands: the app's confirm card waits over the form meanwhile
    cx.host().never::<Submit<Programs>>();
    cx.simulate_click("explorer-form-submit");
    cx.run_until_parked();
    assert!(
        cx.find("explorer-form-problem").is_none(),
        "{:?}",
        cx.texts()
    );
}

#[test]
fn a_scheduled_change_says_who_made_it_and_its_maintainer_cancels_it() {
    // the vote's change of chat, made after Ada's
    let mut scheduled = waiting();
    scheduled.push(Pending {
        height: 800,
        change: Change::Set(entry("chat", 0x88)),
        by: None,
    });
    let mut cx = reader(Some(3), ADA, false, grants(), scheduled);
    assert!(cx.has_text("by Ada") && cx.has_text("by vote"));
    // 688 blocks at 1 s
    assert!(cx.has_text("validators can cancel for 12 min"));
    assert!(
        cx.find("explorer-cancel-2").is_none(),
        "the vote's change of chat is not its maintainer's to cancel"
    );
    cx.simulate_click("explorer-cancel-1");
    cx.run_until_parked();
    assert_eq!(
        cx.host().requests::<Submit<Programs>>(),
        [programs::Op::Cancel {
            height: 700,
            program: "chat".into(),
        }]
    );
    let mut cx = reader(None, VALIDATOR, true, grants(), waiting());
    cx.simulate_click("explorer-cancel-0");
    cx.run_until_parked();
    let op = abi::encode(&programs::Op::Cancel {
        height: 120,
        program: "forge".into(),
    });
    assert_eq!(
        cx.host().requests::<Submit<Valset>>(),
        [valset::Op::Vote(Motion::Role {
            role: abi::role::PROGRAMS.into(),
            op,
        })]
    );
}

#[test]
fn every_program_and_an_unseated_program_let_their_maintainer_add() {
    let cx = maintainer();
    assert!(cx.find("explorer-add").is_none(), "chat runs already");
    let trade = Grant {
        scope: Scope::Program("trade".into()),
        account: 3,
    };
    let cx = reader(Some(3), ADA, false, vec![trade], Vec::new());
    assert!(cx.find("explorer-add").is_some());
    assert!(cx.has_text("runs nothing yet: Ada may add it"));
}

/// A form's op, built from what was typed: the code, the height, the
/// params a running program keeps.
#[test]
fn a_form_builds_its_op_or_says_why_not() {
    let network = Network {
        programs: vec![entry("chat", 0xab)],
        ..Network::default()
    };
    let form = |edit: Edit, code: &str, height: &str| Form {
        edit,
        path: Path::Grant,
        code: TextField::new(code),
        height: TextField::new(height),
        name: TextField::new("trade"),
        params: TextField::new("0a0b"),
        account: TextField::new("#7"),
        view: false,
        problem: None,
    };
    let code = abi::hex(&CODE);
    let upgrade_chat = Edit::Upgrade {
        name: "chat".into(),
        view: false,
    };
    assert_eq!(
        acting::built(&form(upgrade_chat.clone(), &code, "1,000"), &network, 12),
        Ok(upgrade(1000))
    );
    for (edit, code, height, why) in [
        (
            upgrade_chat.clone(),
            "sha256:12",
            "1000",
            "A code id is sha256: and 64 hex digits.",
        ),
        (
            upgrade_chat.clone(),
            code.as_str(),
            "soon",
            "The height is a block number.",
        ),
        (
            upgrade_chat,
            code.as_str(),
            "12",
            "Block 12 has passed: pick a later one.",
        ),
        (
            Edit::Upgrade {
                name: "gone".into(),
                view: false,
            },
            code.as_str(),
            "1000",
            "gone no longer runs here.",
        ),
    ] {
        assert_eq!(
            acting::built(&form(edit, code, height), &network, 12)
                .map_err(|problem| problem.sentence),
            Err(why.to_owned())
        );
    }
    let added = acting::built(&form(Edit::Add, &code, "1000"), &network, 12).unwrap();
    assert_eq!(
        added,
        programs::Op::Schedule(programs::Scheduled {
            height: 1000,
            change: Change::Set(programs::Entry {
                program: "trade".into(),
                code: BlobId::Sha256(CODE),
                params: vec![10, 11],
            }),
        })
    );
    assert_eq!(
        acting::built(&form(Edit::Grant, "", ""), &network, 12),
        Ok(programs::Op::Grant(Grant {
            scope: Scope::Program("trade".into()),
            account: 7,
        }))
    );
    assert_eq!(acting::default_height(Path::Grant, 12, 1000), 102);
    assert_eq!(acting::default_height(Path::Vote, 12, 1000), 612);
    assert_eq!(acting::default_height(Path::Vote, 12, 2000), 312);
}
