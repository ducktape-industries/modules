//! push on a `MockChain` beside identity, valset, awaits, scheduler and egress: a program's
//! notices reach the node serving `push` as one JSON post naming each recipient's keys; a person,
//! a network with no egress role and a notice over the caps are refused; the service's answer is
//! the frame's outcome.
use ::program::{Error, MockChain, MockHost, Origin, Roles, code};
use egress::{Egress, Request, RequestRow, http};
use serde_json::{Value, json};
use valset::{Genesis, Member, Valset};

use crate::rules::{addressed, answered, checked};
use crate::*;

const NODE: [u8; 32] = [1; 32];
const ADA: &[u8] = b"ada-key";
const BO: &[u8] = b"bo-key";
const SENDER: &str = "sender";

fn signed(key: &[u8]) -> Origin {
    Origin::Signed(key.to_vec())
}

fn chain(roles: Roles) -> MockChain {
    let mut chain = MockChain::founded(roles);
    chain.seat::<identity::Identity>(identity::PROGRAM);
    chain.seat::<Valset>("valset");
    chain.seat::<awaits::Awaits>(awaits::PROGRAM);
    chain.seat::<scheduler::Scheduler>(scheduler::PROGRAM);
    chain.seat_awaiting::<Egress>(egress::PROGRAM);
    chain.seat_awaiting::<Push>(PROGRAM);
    for program in ["valset", egress::PROGRAM, PROGRAM, SENDER] {
        let register = identity::Op::RegisterProgram {
            program: program.into(),
        };
        chain
            .submit(Origin::Root, identity::PROGRAM, &register)
            .unwrap();
    }
    let genesis = Genesis {
        validators: vec![Member {
            key: NODE.to_vec(),
            address: None,
            halo: "node.example.org:9100".into(),
        }],
        member_cap: 8,
    };
    chain.init("valset", &genesis).unwrap();
    chain
}

fn person(chain: &MockChain, key: &[u8], name: &str) -> AccountNumber {
    let create = identity::Op::Create {
        name: name.into(),
        scheme: ::program::Scheme::Ed25519,
    };
    abi::decode(
        &chain
            .submit(signed(key), identity::PROGRAM, &create)
            .unwrap(),
    )
    .unwrap()
}

/// A network whose node serves `push`, Ada's registered service, and two people.
fn world() -> (MockChain, AccountNumber, AccountNumber) {
    let chain = chain(MockHost::roles());
    let serve = egress::Op::Serve {
        services: vec![SERVICE.into()],
    };
    chain
        .submit(signed(&NODE), egress::PROGRAM, &serve)
        .unwrap();
    let ada = person(&chain, ADA, "Ada");
    let bo = person(&chain, BO, "Bo");
    let register = egress::Op::Register {
        service: SERVICE.into(),
    };
    let assign = egress::Op::Assign {
        service: SERVICE.into(),
        nodes: vec![NODE.to_vec()],
    };
    for op in [register, assign] {
        chain.submit(signed(ADA), egress::PROGRAM, &op).unwrap();
    }
    (chain, ada, bo)
}

fn assigned(chain: &MockChain) -> Vec<RequestRow> {
    let asked = egress::Query::Assigned { key: NODE.to_vec() };
    let egress::Reply::Requests(rows) = chain.query(egress::PROGRAM, &asked).unwrap() else {
        panic!()
    };
    rows
}

fn body(chain: &MockChain, row: &RequestRow) -> Value {
    let host = chain.host(egress::PROGRAM).borrow();
    let mut bytes = Vec::new();
    for part in row.request.parts() {
        bytes.extend(&host.blobs[part].body);
    }
    serde_json::from_slice(&bytes).expect("the body is one JSON document")
}

fn notice(to: Vec<AccountNumber>, title: &str) -> Notice {
    Notice::new(
        to,
        title,
        "the body",
        "#general".into(),
        vec!["general".into(), "7".into()],
    )
}

fn send(notices: Vec<Notice>) -> Op {
    Op::Send { notices }
}

#[test]
fn a_programs_notices_reach_the_node_serving_push_as_one_json_post_of_keys() {
    let (chain, ada, bo) = world();
    let nobody = 99;
    let notices = vec![
        notice(vec![bo, nobody, bo], "for Bo"),
        notice(vec![nobody], "for no one"),
        notice(vec![ada], "for Ada"),
    ];
    chain
        .submit(Origin::Program(SENDER.into()), PROGRAM, &send(notices))
        .unwrap();

    let [row] = assigned(&chain).try_into().expect("one request");
    assert_eq!(row.requester, PROGRAM);
    let Request::Http { service, head, .. } = &row.request else {
        panic!("push posts over http");
    };
    assert_eq!(service, SERVICE);
    assert_eq!(
        (head.method, head.target.as_str(), head.headers.as_slice()),
        (
            http::Method::Post,
            TARGET,
            &[("content-type".to_owned(), "application/json".to_owned())][..]
        )
    );
    let link = json!(["general", "7"]);
    assert_eq!(
        body(&chain, &row),
        json!({
            "program": SENDER,
            "notices": [
                { "keys": [abi::hex(BO)], "title": "for Bo", "body": "the body", "tag": "#general", "link": link },
                { "keys": [abi::hex(ADA)], "title": "for Ada", "body": "the body", "tag": "#general", "link": link },
            ],
        })
    );
}

#[test]
fn a_send_no_key_is_left_for_posts_nothing() {
    let (chain, _, _) = world();
    let notices = vec![notice(vec![99], "for no one")];
    chain
        .submit(Origin::Program(SENDER.into()), PROGRAM, &send(notices))
        .unwrap();
    assert!(assigned(&chain).is_empty());
}

#[test]
fn only_a_program_sends() {
    let (chain, _, bo) = world();
    let refused = chain
        .submit(signed(ADA), PROGRAM, &send(vec![notice(vec![bo], "hi")]))
        .unwrap_err();
    assert_eq!(refused.code, code::UNAUTHORIZED);
    assert!(assigned(&chain).is_empty());
}

#[test]
fn a_network_binding_no_egress_role_refuses_in_a_plain_sentence() {
    let roles = MockHost::roles()
        .iter()
        .filter(|(role, _)| *role != abi::role::EGRESS)
        .collect();
    let chain = chain(roles);
    let bo = person(&chain, BO, "Bo");
    let refused = chain
        .submit(
            Origin::Program(SENDER.into()),
            PROGRAM,
            &send(vec![notice(vec![bo], "hi")]),
        )
        .unwrap_err();
    assert_eq!(refused.code, code::UNSUPPORTED);
    assert!(
        refused.message.contains("egress role"),
        "{}",
        refused.message
    );
}

#[test]
fn the_acting_account_and_an_account_with_no_key_drop_out() {
    let keys = |number: AccountNumber| -> Result<Vec<String>, Error> {
        Ok(match number {
            1 => vec!["aa".into()],
            2 => vec!["bb".into(), "cc".into()],
            _ => Vec::new(),
        })
    };
    let notices = [notice(vec![1, 2, 3], "all"), notice(vec![1], "me alone")];
    let out = addressed(&notices, Some(1), keys).unwrap();
    let [only] = &out[..] else { panic!("{out:?}") };
    assert_eq!(only["keys"], json!(["bb", "cc"]));
    assert_eq!(only["title"], "all");
}

#[test]
fn the_caps_are_refused_and_the_helper_clips_on_a_char_boundary() {
    let long = "가".repeat(200);
    let clipped = Notice::new(vec![1], &long, &long, String::new(), Vec::new());
    assert_eq!(clipped.title.len(), 510);
    assert!(long.starts_with(&clipped.body));
    assert!(checked(std::slice::from_ref(&clipped)).is_ok());

    let over = Notice {
        body: long.clone(),
        ..clipped.clone()
    };
    let too_many_accounts = Notice {
        to: (0..=MAX_RECIPIENTS as u64).collect(),
        ..clipped.clone()
    };
    let too_many_notices = vec![clipped; MAX_NOTICES + 1];
    for refused in [vec![over], vec![too_many_accounts], too_many_notices] {
        assert_eq!(checked(&refused).unwrap_err().code, code::INVALID_INPUT);
    }
}

#[test]
fn the_services_answer_is_the_frames_outcome() {
    let response = |status, body: &[u8]| {
        abi::encode(&http::Response {
            status,
            headers: Vec::new(),
            body: body.to_vec(),
        })
    };
    assert!(answered(&response(202, b"queued")).is_ok());
    let refused = answered(&response(401, &[b'x'; 300])).unwrap_err();
    assert_eq!(
        refused.message,
        format!("the push service answered 401: {}", "x".repeat(200))
    );
    assert!(answered(b"not a response").is_err());
}
