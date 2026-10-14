//! provider on a `MockChain` beside valset and a program that asks: serving, the
//! election's turns, answers delivered back, refusals and expiry.
use ::program::{
    Blob, Error, ExecCtx, HashKind, MockChain, Origin, Program, QueryCtx, Reply as Wanted, blob_id,
    code,
};
use borsh::{BorshDeserialize, BorshSerialize};
use store::Item;
use valset::{Genesis, Member, Valset};

use crate::*;

const ADA: [u8; 32] = [1; 32];
const BO: [u8; 32] = [2; 32];
const STRANGER: [u8; 32] = [9; 32];
const ASKER: &str = "asker";

#[derive(Debug, BorshSerialize, BorshDeserialize)]
enum Ask {
    Ask(Request),
    Delivered(Delivery),
}

const DELIVERED: Item<Vec<Delivery>> = Item::new("delivered");

struct Asker;

impl Program for Asker {
    const NAME: &'static str = ASKER;
    type Op = Ask;
    type Query = ();
    type Reply = Vec<Delivery>;

    fn execute(ctx: &ExecCtx, op: Ask) -> Result<(), Error> {
        match op {
            Ask::Ask(request) => {
                ctx.emit(PROGRAM, &Op::Request(request), Wanted::None);
                Ok(())
            }
            Ask::Delivered(delivery) => {
                ctx.env().sent_by(PROGRAM)?;
                let fail = delivery.outcome == Outcome::Answered(b"refuse me".to_vec());
                if fail {
                    return Err(::program::invalid("the asker refuses this delivery"));
                }
                let mut seen = DELIVERED.get(ctx)?.unwrap_or_default();
                seen.push(delivery);
                DELIVERED.put(ctx, &seen);
                Ok(())
            }
        }
    }

    fn query(ctx: &QueryCtx, (): ()) -> Result<Vec<Delivery>, Error> {
        Ok(DELIVERED.get(ctx)?.unwrap_or_default())
    }
}

fn chain() -> MockChain {
    let mut chain = MockChain::default();
    chain.seat::<Valset>("valset");
    chain.seat::<Provider>(PROGRAM);
    chain.seat::<Asker>(ASKER);
    let members = [ADA, BO].map(|key| Member {
        key: key.to_vec(),
        address: None,
        halo: "node.example.org:9100".into(),
    });
    let genesis = Genesis {
        validators: members.to_vec(),
        member_cap: 8,
    };
    chain.init("valset", &genesis).unwrap();
    chain
}

fn serve(chain: &MockChain, key: [u8; 32], services: &[&str]) -> Result<Vec<u8>, Error> {
    let services = services.iter().map(|service| service.to_string()).collect();
    chain.submit(
        Origin::Signed(key.to_vec()),
        PROGRAM,
        &Op::Serve { services },
    )
}

fn ask(chain: &MockChain, service: &str, input: &[u8]) -> Result<Vec<u8>, Error> {
    let request = Request {
        service: service.into(),
        input: input.to_vec(),
        parts: Vec::new(),
        deliver: vec![1],
    };
    chain.submit(Origin::Signed(STRANGER.to_vec()), ASKER, &Ask::Ask(request))
}

fn assigned(chain: &MockChain, key: [u8; 32]) -> Vec<RequestRow> {
    let asked = Query::Assigned {
        key: key.to_vec(),
        page: PageRequest::default(),
    };
    let Reply::Requests(page) = chain.query(PROGRAM, &asked).unwrap() else {
        panic!()
    };
    page.items
}

fn answer(chain: &MockChain, key: [u8; 32], id: u64, answer: Answer) -> Result<Vec<u8>, Error> {
    chain.submit(
        Origin::Signed(key.to_vec()),
        PROGRAM,
        &Op::Answer { id, answer },
    )
}

fn delivered(chain: &MockChain) -> Vec<Delivery> {
    chain.query(ASKER, &()).unwrap()
}

#[test]
fn the_election_takes_turns_and_only_the_elected_provider_answers() {
    let chain = chain();
    serve(&chain, ADA, &["anthropic", RELAY]).unwrap();
    serve(&chain, BO, &[RELAY]).unwrap();
    for _ in 0..3 {
        ask(&chain, RELAY, b"ping").unwrap();
    }
    ask(&chain, "anthropic", b"model").unwrap();
    let ada: Vec<u64> = assigned(&chain, ADA).iter().map(|row| row.id).collect();
    let bo: Vec<u64> = assigned(&chain, BO).iter().map(|row| row.id).collect();
    assert_eq!((ada, bo), (vec![1, 3, 4], vec![2]));
    let row = &assigned(&chain, BO)[0];
    assert_eq!(
        (row.requester.as_str(), row.input.as_slice(), row.deadline),
        (ASKER, &b"ping"[..], 1 + EXPIRY_BLOCKS)
    );

    let same_block = answer(&chain, BO, 2, Answer::Output(Vec::new())).unwrap_err();
    assert_eq!(same_block.code, code::WRONG_STATE);
    chain.at(2, 0);
    let wrong = answer(&chain, ADA, 2, Answer::Output(Vec::new())).unwrap_err();
    assert_eq!(wrong.code, code::UNAUTHORIZED);
    answer(&chain, BO, 2, Answer::Output(b"pong".to_vec())).unwrap();
    answer(&chain, ADA, 4, Answer::Failed("upstream timed out".into())).unwrap();
    assert_eq!(
        delivered(&chain),
        [
            Delivery {
                id: 2,
                service: RELAY.into(),
                outcome: Outcome::Answered(b"pong".to_vec()),
            },
            Delivery {
                id: 4,
                service: "anthropic".into(),
                outcome: Outcome::Failed("upstream timed out".into()),
            },
        ]
    );
    assert!(assigned(&chain, BO).is_empty());
    let twice = answer(&chain, BO, 2, Answer::Output(Vec::new())).unwrap_err();
    assert_eq!(twice.code, code::NOT_FOUND);
}

#[test]
fn only_a_member_node_serves_and_only_a_program_requests() {
    let chain = chain();
    assert_eq!(
        serve(&chain, STRANGER, &[RELAY]).unwrap_err().code,
        code::UNAUTHORIZED
    );
    assert_eq!(ask(&chain, RELAY, b"").unwrap_err().code, code::NOT_FOUND);
    serve(&chain, ADA, &[RELAY]).unwrap();
    let direct = Op::Request(Request {
        service: RELAY.into(),
        input: Vec::new(),
        parts: Vec::new(),
        deliver: Vec::new(),
    });
    let signed = chain
        .submit(Origin::Signed(ADA.to_vec()), PROGRAM, &direct)
        .unwrap_err();
    assert_eq!(signed.code, code::UNAUTHORIZED);
    assert_eq!(
        serve(&chain, ADA, &["Bad Name"]).unwrap_err().code,
        code::INVALID_INPUT
    );
    serve(&chain, ADA, &[]).unwrap();
    let Reply::Serving(keys) = chain
        .query(
            PROGRAM,
            &Query::Serving {
                service: RELAY.into(),
            },
        )
        .unwrap()
    else {
        panic!()
    };
    assert!(keys.is_empty());
}

#[test]
fn an_unanswered_request_expires_after_its_window_and_tells_the_requester() {
    let chain = chain();
    serve(&chain, ADA, &[RELAY]).unwrap();
    ask(&chain, RELAY, b"").unwrap();
    let expire = Op::Expire { id: 1 };
    let early = chain
        .submit(Origin::Signed(STRANGER.to_vec()), PROGRAM, &expire)
        .unwrap_err();
    assert_eq!(early.code, code::WRONG_STATE);
    chain.at(1 + EXPIRY_BLOCKS, 0);
    let Reply::Requests(none) = chain
        .query(
            PROGRAM,
            &Query::Overdue {
                page: PageRequest::default(),
            },
        )
        .unwrap()
    else {
        panic!()
    };
    assert!(none.items.is_empty());
    chain.at(2 + EXPIRY_BLOCKS, 0);
    let late = answer(&chain, ADA, 1, Answer::Output(Vec::new())).unwrap_err();
    assert_eq!(late.code, code::WRONG_STATE);
    let Reply::Requests(due) = chain
        .query(
            PROGRAM,
            &Query::Overdue {
                page: PageRequest::default(),
            },
        )
        .unwrap()
    else {
        panic!()
    };
    assert_eq!(due.items.len(), 1);
    chain
        .submit(Origin::Signed(STRANGER.to_vec()), PROGRAM, &expire)
        .unwrap();
    assert_eq!(delivered(&chain)[0].outcome, Outcome::Expired);
    let Reply::Request(gone) = chain.query(PROGRAM, &Query::Request { id: 1 }).unwrap() else {
        panic!()
    };
    assert_eq!(gone, None);
}

#[test]
fn a_refused_delivery_still_closes_the_request() {
    let chain = chain();
    serve(&chain, ADA, &[RELAY]).unwrap();
    ask(&chain, RELAY, b"").unwrap();
    chain.at(2, 0);
    answer(&chain, ADA, 1, Answer::Output(b"refuse me".to_vec())).unwrap();
    assert!(delivered(&chain).is_empty());
    assert!(assigned(&chain, ADA).is_empty());
}

#[test]
fn a_request_names_parts_already_on_the_network() {
    let chain = chain();
    serve(&chain, ADA, &["anthropic"]).unwrap();
    let body = b"{\"model\":\"m\"}".to_vec();
    let part = blob_id(HashKind::Sha256, "part", &body).unwrap();
    let with_part = |part| Request {
        service: "anthropic".into(),
        input: abi::encode(&http::Head {
            method: http::Method::Post,
            path: "/v1/messages".into(),
            headers: vec![("anthropic-version".into(), "2023-06-01".into())],
        }),
        parts: vec![part],
        deliver: vec![1],
    };
    let missing = chain
        .submit(
            Origin::Signed(STRANGER.to_vec()),
            ASKER,
            &Ask::Ask(with_part(part)),
        )
        .unwrap_err();
    assert_eq!(missing.code, code::NOT_FOUND);
    chain.host(PROGRAM).borrow_mut().blobs.insert(
        part,
        Blob {
            kind: "part".into(),
            body,
        },
    );
    chain
        .submit(
            Origin::Signed(STRANGER.to_vec()),
            ASKER,
            &Ask::Ask(with_part(part)),
        )
        .unwrap();
    assert_eq!(assigned(&chain, ADA)[0].parts, [part]);
}

#[test]
fn an_http_head_withholds_what_only_the_provider_sets() {
    let head = |headers: Vec<(&str, &str)>, path: &str| http::Head {
        method: http::Method::Post,
        path: path.into(),
        headers: headers
            .into_iter()
            .map(|(name, value)| (name.into(), value.into()))
            .collect(),
    };
    assert!(
        head(vec![("anthropic-version", "2023-06-01")], "/v1/messages")
            .check()
            .is_ok()
    );
    for name in ["authorization", "x-api-key", "host"] {
        assert!(
            head(vec![(name, "x")], "/v1/messages").check().is_err(),
            "{name}"
        );
    }
    assert!(head(vec![("X-Upper", "x")], "/").check().is_err());
    assert!(head(Vec::new(), "https://elsewhere/").check().is_err());
}
