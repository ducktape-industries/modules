// The program on a `MockChain`, with a real valset bound to the validators role: what the
// validators' vote opens, sets and closes, and what the table refuses.

use ::program::{Error, MockChain, Origin, code};
use valset::{Member, Motion, Valset, Voted};

use crate::{Class, Founded, Genesis, Halo, Lane, LaneId, MAX_NAME, Op, PROGRAM, Query, Reply};

const VALIDATORS: [u8; 2] = [1, 2];

fn key(n: u8) -> Vec<u8> {
    vec![n; 32]
}

fn founding(name: &str, cap: Class, pinned: bool) -> Founded {
    Founded {
        name: name.into(),
        cap,
        pinned,
    }
}

/// valset seating validators 1 and 2, and halo founded with `blobs` at 0, pinned, and `live`
/// at 1.
fn chain() -> MockChain {
    let mut chain = MockChain::default();
    chain.seat::<Valset>(valset::PROGRAM);
    chain.seat::<Halo>(PROGRAM);
    let validators = VALIDATORS
        .iter()
        .map(|n| Member {
            key: key(*n),
            address: None,
            halo: format!("node-{n}.example.org:9100"),
        })
        .collect();
    let seated = valset::Genesis {
        validators,
        member_cap: 8,
    };
    chain.init(valset::PROGRAM, &seated).unwrap();
    let lanes = Genesis {
        lanes: vec![
            founding("blobs", Class::Critical, true),
            founding("live", Class::Client, false),
        ],
    };
    chain.init(PROGRAM, &lanes).unwrap();
    chain
}

fn lane(name: &str, id: LaneId, cap: Class, since: u64) -> Lane {
    Lane {
        name: name.into(),
        id,
        cap,
        enabled: true,
        pinned: false,
        since,
    }
}

fn founded() -> Vec<Lane> {
    let blobs = Lane {
        pinned: true,
        ..lane("blobs", 0, Class::Critical, 0)
    };
    vec![blobs, lane("live", 1, Class::Client, 0)]
}

fn lanes(chain: &MockChain) -> Vec<Lane> {
    match chain.query(PROGRAM, &Query::Lanes).unwrap() {
        Reply::Lanes(lanes) => lanes,
        other => panic!("{other:?}"),
    }
}

fn vote(chain: &MockChain, voter: u8, op: &Op) -> Result<Voted, Error> {
    let motion = Motion::Halo {
        op: abi::encode(op),
    };
    let output = chain.submit(
        Origin::Signed(key(voter)),
        valset::PROGRAM,
        &valset::Op::Vote(motion),
    )?;
    Ok(abi::decode(&output).unwrap())
}

/// `op` passed by both validators.
fn pass(chain: &MockChain, op: &Op) {
    assert!(matches!(vote(chain, 1, op), Ok(Voted::Counted { .. })));
    assert_eq!(vote(chain, 2, op), Ok(Voted::Enacted));
}

/// `op` as the validators' program sends it once a vote passes, refused or not.
fn enacted(chain: &MockChain, op: &Op) -> Result<(), Error> {
    let by_validators = Origin::Program(valset::PROGRAM.into());
    chain.submit(by_validators, PROGRAM, op).map(drop)
}

fn open(name: &str, cap: Class) -> Op {
    Op::Open {
        name: name.into(),
        cap,
    }
}

#[test]
fn founding_lists_its_lanes_in_order_from_id_0() {
    let chain = chain();
    assert_eq!(lanes(&chain), founded());
    let asked = Query::Lane {
        name: "live".into(),
    };
    let Reply::Lane(live) = chain.query(PROGRAM, &asked).unwrap() else {
        panic!("Lane answers Lane");
    };
    assert_eq!(live, Some(founded()[1].clone()));
}

#[test]
fn founding_refuses_a_repeated_name() {
    let mut chain = MockChain::default();
    chain.seat::<Halo>(PROGRAM);
    let repeated = Genesis {
        lanes: vec![
            founding("blobs", Class::Critical, true),
            founding("blobs", Class::Client, false),
        ],
    };
    let refused = chain.init(PROGRAM, &repeated).unwrap_err();
    assert_eq!(refused.code, code::ALREADY_EXISTS);
}

#[test]
fn the_validators_vote_opens_a_lane_at_the_next_id() {
    let chain = chain();
    chain.at(7, 0);
    pass(&chain, &open("chat", Class::Member));
    let opened = lane("chat", 2, Class::Member, 7);
    assert_eq!(lanes(&chain), [founded(), vec![opened]].concat());
}

#[test]
fn a_signed_frame_changes_no_lane() {
    let chain = chain();
    let refused = chain
        .submit(
            Origin::Signed(key(1)),
            PROGRAM,
            &open("chat", Class::Member),
        )
        .unwrap_err();
    assert_eq!(refused.code, code::UNAUTHORIZED);
    assert_eq!(lanes(&chain), founded());
}

#[test]
fn a_listed_name_is_not_opened_twice_and_an_unlisted_one_is_not_changed() {
    let chain = chain();
    let twice = enacted(&chain, &open("live", Class::Member)).unwrap_err();
    assert_eq!(twice.code, code::ALREADY_EXISTS);
    let set = Op::Set {
        name: "chat".into(),
        cap: Class::Member,
        enabled: false,
    };
    let close = Op::Close {
        name: "chat".into(),
    };
    for unlisted in [set, close] {
        let refused = enacted(&chain, &unlisted).unwrap_err();
        assert_eq!(refused.code, code::NOT_FOUND, "{unlisted:?}");
    }
    assert_eq!(lanes(&chain), founded());
}

#[test]
fn a_lane_is_named_by_1_to_max_name_printable_ascii_bytes() {
    let chain = chain();
    for unnamed in [
        String::new(),
        "a".repeat(MAX_NAME + 1),
        "two words".into(),
        "line\nbreak".into(),
        "é".into(),
    ] {
        let refused = enacted(&chain, &open(&unnamed, Class::Client)).unwrap_err();
        assert_eq!(refused.code, code::INVALID_INPUT, "{unnamed:?}");
    }
    enacted(&chain, &open(&"a".repeat(MAX_NAME), Class::Client)).unwrap();
}

#[test]
fn set_keeps_the_id_and_a_paused_lane_resumes_under_it() {
    let chain = chain();
    let pause = Op::Set {
        name: "live".into(),
        cap: Class::Member,
        enabled: false,
    };
    enacted(&chain, &pause).unwrap();
    let paused = Lane {
        cap: Class::Member,
        enabled: false,
        ..founded()[1].clone()
    };
    assert_eq!(lanes(&chain)[1], paused);
    let resume = Op::Set {
        name: "live".into(),
        cap: Class::Client,
        enabled: true,
    };
    enacted(&chain, &resume).unwrap();
    assert_eq!(lanes(&chain), founded());
}

#[test]
fn a_closed_name_reopens_at_a_fresh_id() {
    let chain = chain();
    let close = Op::Close {
        name: "live".into(),
    };
    enacted(&chain, &close).unwrap();
    assert_eq!(lanes(&chain), founded()[..1]);
    chain.at(4, 0);
    enacted(&chain, &open("live", Class::Client)).unwrap();
    let reopened = lane("live", 2, Class::Client, 4);
    assert_eq!(lanes(&chain), [&founded()[..1], &[reopened]].concat());
}

#[test]
fn a_pinned_lane_is_never_paused_or_closed_and_its_cap_still_moves() {
    let chain = chain();
    let pause = Op::Set {
        name: "blobs".into(),
        cap: Class::Critical,
        enabled: false,
    };
    let close = Op::Close {
        name: "blobs".into(),
    };
    for refused in [&pause, &close] {
        // at the first vote, before it counts
        let voted = vote(&chain, 1, refused).unwrap_err();
        assert_eq!(voted.code, code::INVALID_INPUT, "{refused:?}");
        // and by the program, whoever enacts it
        let enacted = enacted(&chain, refused).unwrap_err();
        assert_eq!(enacted.code, code::INVALID_INPUT, "{refused:?}");
    }
    assert_eq!(lanes(&chain), founded());
    let capped = Op::Set {
        name: "blobs".into(),
        cap: Class::Member,
        enabled: true,
    };
    pass(&chain, &capped);
    let moved = Lane {
        cap: Class::Member,
        ..founded()[0].clone()
    };
    assert_eq!(lanes(&chain)[0], moved);
}

#[test]
fn every_id_is_handed_out_once_and_then_no_lane_opens() {
    let chain = chain();
    for n in 2..=LaneId::MAX {
        enacted(&chain, &open(&format!("lane-{n}"), Class::Client)).unwrap();
    }
    let spent = enacted(&chain, &open("one-more", Class::Client)).unwrap_err();
    assert_eq!(spent.code, code::CAPACITY);
    let close = Op::Close {
        name: "lane-255".into(),
    };
    enacted(&chain, &close).unwrap();
    let still = enacted(&chain, &open("one-more", Class::Client)).unwrap_err();
    assert_eq!(
        still.code,
        code::CAPACITY,
        "a closed lane's id is not handed out again"
    );
}

#[test]
fn an_op_halo_would_refuse_is_refused_at_the_first_vote() {
    let chain = chain();
    let set = Op::Set {
        name: "chat".into(),
        cap: Class::Member,
        enabled: true,
    };
    for (refused, why) in [
        (open("live", Class::Member), code::ALREADY_EXISTS),
        (open("", Class::Member), code::INVALID_INPUT),
        (set, code::NOT_FOUND),
    ] {
        assert_eq!(
            vote(&chain, 1, &refused).unwrap_err().code,
            why,
            "{refused:?}"
        );
    }
    let garbage = valset::Op::Vote(Motion::Halo {
        op: b"not an op".to_vec(),
    });
    let refused = chain
        .submit(Origin::Signed(key(1)), valset::PROGRAM, &garbage)
        .unwrap_err();
    assert_eq!(refused.code, code::INVALID_INPUT);
}

/// The halo motions valset lists as waiting for votes, in no order of theirs.
fn waiting(chain: &MockChain) -> Vec<Op> {
    let asked = valset::Query::Motions {
        page: store::PageRequest::first(16),
    };
    let valset::Reply::Motions { page, .. } = chain.query(valset::PROGRAM, &asked).unwrap() else {
        panic!("valset answers Motions with Motions");
    };
    page.items
        .into_iter()
        .map(|pending| match pending.motion {
            Motion::Halo { op } => abi::decode(&op).unwrap(),
            other => panic!("{other:?}"),
        })
        .collect()
}

/// `ops` in one order, so two lists compare as sets.
fn sorted(mut ops: Vec<Op>) -> Vec<Op> {
    ops.sort_by_key(abi::encode);
    ops
}

#[test]
fn a_motion_an_enacted_one_overtakes_is_cleared_with_it() {
    let chain = chain();
    let set = |name: &str, cap, enabled| Op::Set {
        name: name.into(),
        cap,
        enabled,
    };
    let close = Op::Close {
        name: "live".into(),
    };
    // another lane's change: no vote below overtakes it
    let kept = set("blobs", Class::Member, true);
    for op in [
        open("chat", Class::Member),
        set("live", Class::Member, true),
        set("live", Class::Critical, false),
        kept.clone(),
    ] {
        vote(&chain, 1, &op).unwrap();
    }
    pass(&chain, &open("chat", Class::Client));
    // chat at Member can never pass now that chat is listed
    assert_eq!(
        sorted(waiting(&chain)),
        sorted(vec![
            set("live", Class::Member, true),
            set("live", Class::Critical, false),
            kept.clone()
        ])
    );
    pass(&chain, &close);
    // nor can a change to live once it is closed
    assert_eq!(waiting(&chain), [kept]);
}

#[test]
fn an_op_is_described_by_its_lane() {
    let titles: Vec<String> = [
        open("chat", Class::Member),
        Op::Set {
            name: "chat".into(),
            cap: Class::Client,
            enabled: false,
        },
        Op::Close {
            name: "chat".into(),
        },
    ]
    .iter()
    .map(|op| crate::describe(op).title)
    .collect();
    assert_eq!(
        titles,
        ["Open lane · chat", "Set lane · chat", "Close lane · chat"]
    );
}
