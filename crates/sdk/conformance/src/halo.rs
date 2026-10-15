//! The halo role: the lane table the node serves halo's lanes by, each name to the wire id
//! no other lane ever had. [`run`] checks every rule below.

use abi::role::halo::{Class, Genesis, Lane, Op, Query, Reply};
use program::{Error, MockHost, Origin, Principal, Program, code};

use crate::{ask, env, execute, init, same_bytes};

pub trait Fixture {
    type Program: Program;

    fn host(&self) -> MockHost;
}

pub fn run(fixture: &impl Fixture) {
    the_role_is_the_first_variants(fixture);
    genesis_lists_its_lanes_in_order_from_id_0(fixture);
    only_the_validators_vote_changes_the_table(fixture);
    open_lists_a_name_at_the_next_id(fixture);
    set_keeps_the_id_and_a_paused_lane_stays_listed(fixture);
    a_closed_lanes_id_is_never_handed_out_again(fixture);
}

const HEIGHT: u64 = 10;

fn program() -> String {
    MockHost::roles().halo
}

/// The founding lanes: `blobs` at 0, `live` at 1.
fn genesis() -> Genesis {
    Genesis {
        lanes: vec![
            ("blobs".into(), Class::Critical),
            ("live".into(), Class::Client),
        ],
    }
}

fn lane(name: &str, id: u8, cap: Class, enabled: bool, since: u64) -> Lane {
    Lane {
        name: name.into(),
        id,
        cap,
        enabled,
        since,
    }
}

fn founded<F: Fixture>(fixture: &F) -> MockHost {
    let host = fixture.host();
    init::<F::Program>(&host, &program(), &genesis());
    host
}

/// `op` from `origin` at `height`, its writes kept only when it is taken.
fn send<F: Fixture>(host: &MockHost, origin: Origin, height: u64, op: &Op) -> Result<(), Error> {
    let sender = match origin {
        Origin::Root => Principal::Root,
        _ => Principal::Account(7),
    };
    let env = env(&program(), height, origin, Some(sender));
    host.attempt(|| execute::<F::Program>(host, env, op))
}

/// `op` as the validators' vote enacts it: sent by the program bound to the validators role.
fn voted<F: Fixture>(host: &MockHost, height: u64, op: &Op) -> Result<(), Error> {
    let validators = Origin::Program(MockHost::roles().validators);
    send::<F>(host, validators, height, op)
}

fn lanes<F: Fixture>(host: &MockHost) -> Vec<Lane> {
    match ask::<F::Program, Reply>(host, &program(), HEIGHT, &Query::Lanes) {
        Ok(Reply::Lanes(lanes)) => lanes,
        other => panic!("halo: Lanes answers Lanes, not {other:?}"),
    }
}

fn one<F: Fixture>(host: &MockHost, name: &str) -> Option<Lane> {
    let query = Query::Lane { name: name.into() };
    match ask::<F::Program, Reply>(host, &program(), HEIGHT, &query) {
        Ok(Reply::Lane(lane)) => lane,
        other => panic!("halo: Lane({name}) answers Lane, not {other:?}"),
    }
}

fn open(name: &str, cap: Class) -> Op {
    Op::Open {
        name: name.into(),
        cap,
    }
}

/// The role's ops, queries and replies are the program's first variants, byte for byte.
pub fn the_role_is_the_first_variants<F: Fixture>(_: &F) {
    type M<F> = <F as Fixture>::Program;
    let m = program();
    for op in [
        open("chat", Class::Member),
        Op::Set {
            name: "chat".into(),
            cap: Class::Client,
            enabled: false,
        },
        Op::Close {
            name: "chat".into(),
        },
    ] {
        same_bytes::<<M<F> as Program>::Op>(&m, "Op", &op);
    }
    for query in [
        Query::Lanes,
        Query::Lane {
            name: "chat".into(),
        },
    ] {
        same_bytes::<<M<F> as Program>::Query>(&m, "Query", &query);
    }
    let chat = lane("chat", 2, Class::Member, true, HEIGHT);
    for reply in [Reply::Lanes(vec![chat.clone()]), Reply::Lane(Some(chat))] {
        same_bytes::<<M<F> as Program>::Reply>(&m, "Reply", &reply);
    }
}

/// Init with the role's genesis lists its lanes in its order, from id 0, each served, from
/// block 0; `Lanes` answers them in id order and `Lane` each by name.
pub fn genesis_lists_its_lanes_in_order_from_id_0<F: Fixture>(fixture: &F) {
    let host = founded(fixture);
    let founded = [
        lane("blobs", 0, Class::Critical, true, 0),
        lane("live", 1, Class::Client, true, 0),
    ];
    assert_eq!(
        lanes::<F>(&host),
        founded,
        "halo: genesis lists its lanes in order from id 0"
    );
    assert_eq!(
        one::<F>(&host, "live"),
        Some(founded[1].clone()),
        "halo: Lane answers a listed lane by name"
    );
    assert_eq!(
        one::<F>(&host, "chat"),
        None,
        "halo: Lane of an unlisted name is None"
    );
}

/// Every op is the validators' vote: a signed frame, the kernel and any other program are
/// refused `unauthorized` and change nothing.
pub fn only_the_validators_vote_changes_the_table<F: Fixture>(fixture: &F) {
    let host = founded(fixture);
    let before = lanes::<F>(&host);
    for origin in [
        Origin::Signed(b"anyone".to_vec()),
        Origin::Root,
        Origin::Program("chat".into()),
    ] {
        let refused = send::<F>(&host, origin.clone(), HEIGHT, &open("chat", Class::Member))
            .expect_err("halo: only the validators' vote opens a lane");
        assert_eq!(
            refused.code,
            code::UNAUTHORIZED,
            "halo: an op from {origin:?} is refused unauthorized"
        );
    }
    assert_eq!(
        lanes::<F>(&host),
        before,
        "halo: a refused op changes no lane"
    );
}

/// `Open` lists a name at the next id, served, from the block that opened it; a name listed
/// already is refused and keeps its lane.
pub fn open_lists_a_name_at_the_next_id<F: Fixture>(fixture: &F) {
    let host = founded(fixture);
    voted::<F>(&host, 7, &open("chat", Class::Member))
        .unwrap_or_else(|e| panic!("halo: the vote opens a lane: {e:?}"));
    let chat = lane("chat", 2, Class::Member, true, 7);
    assert_eq!(
        one::<F>(&host, "chat"),
        Some(chat),
        "halo: Open lists the name at the next id"
    );
    assert!(
        voted::<F>(&host, 8, &open("live", Class::Member)).is_err(),
        "halo: Open of a listed name is refused"
    );
    assert_eq!(
        one::<F>(&host, "live"),
        Some(lane("live", 1, Class::Client, true, 0)),
        "halo: a refused Open keeps the lane it named"
    );
}

/// `Set` changes a lane's cap and whether it is served, under the same id; a paused lane is
/// still listed, `enabled: false`, and served again by the next `Set`. An unlisted name is
/// refused.
pub fn set_keeps_the_id_and_a_paused_lane_stays_listed<F: Fixture>(fixture: &F) {
    let host = founded(fixture);
    let set = |cap, enabled| Op::Set {
        name: "live".into(),
        cap,
        enabled,
    };
    voted::<F>(&host, 7, &set(Class::Member, false))
        .unwrap_or_else(|e| panic!("halo: the vote pauses a lane: {e:?}"));
    assert_eq!(
        lanes::<F>(&host),
        [
            lane("blobs", 0, Class::Critical, true, 0),
            lane("live", 1, Class::Member, false, 0),
        ],
        "halo: a paused lane stays listed at its id, enabled: false"
    );
    voted::<F>(&host, 8, &set(Class::Client, true))
        .unwrap_or_else(|e| panic!("halo: the vote serves a paused lane again: {e:?}"));
    assert_eq!(
        one::<F>(&host, "live"),
        Some(lane("live", 1, Class::Client, true, 0)),
        "halo: Set keeps the id"
    );
    let unlisted = Op::Set {
        name: "chat".into(),
        cap: Class::Client,
        enabled: true,
    };
    assert!(
        voted::<F>(&host, 9, &unlisted).is_err(),
        "halo: Set of an unlisted name is refused"
    );
}

/// `Close` unlists a lane; its id is spent, so the name opened again takes the next id, not
/// the one it had.
pub fn a_closed_lanes_id_is_never_handed_out_again<F: Fixture>(fixture: &F) {
    let host = founded(fixture);
    let close = Op::Close {
        name: "live".into(),
    };
    voted::<F>(&host, 7, &close).unwrap_or_else(|e| panic!("halo: the vote closes a lane: {e:?}"));
    assert_eq!(
        lanes::<F>(&host),
        [lane("blobs", 0, Class::Critical, true, 0)],
        "halo: a closed lane is not listed"
    );
    assert!(
        voted::<F>(&host, 8, &close).is_err(),
        "halo: Close of an unlisted name is refused"
    );
    voted::<F>(&host, 9, &open("live", Class::Client))
        .unwrap_or_else(|e| panic!("halo: the vote opens a closed name again: {e:?}"));
    assert_eq!(
        one::<F>(&host, "live"),
        Some(lane("live", 2, Class::Client, true, 9)),
        "halo: a name opened again takes the next id, never its old one"
    );
}
