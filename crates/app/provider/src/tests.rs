//! provider on a `MockChain` beside valset, the awaits role and a program that calls it:
//! serving, the election's turns, re-election, answers that resolve the call, refusals and expiry.
use ::program::{
    Awaiting, Blob, Ctx, Error, HashKind, MockChain, Origin, Program, QueryCtx, Reply as Wanted,
    blob_id, code,
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
    Call(Request),
    Emit(Request),
}

type Answers = Vec<Result<Vec<u8>, Error>>;

const ANSWERS: Item<Answers> = Item::new("answers");

struct Asker;

impl Program for Asker {
    const NAME: &'static str = ASKER;
    type Op = Ask;
    type Query = ();
    type Reply = Answers;

    fn query(ctx: &QueryCtx, (): ()) -> Result<Answers, Error> {
        Ok(ANSWERS.get(ctx)?.unwrap_or_default())
    }
}

impl Awaiting for Asker {
    async fn execute(ctx: Ctx, ask: Ask) -> Result<(), Error> {
        let request = match ask {
            Ask::Call(request) => request,
            Ask::Emit(request) => {
                ctx.emit(PROGRAM, &Op::Request(request), Wanted::None);
                return Ok(());
            }
        };
        let answer = ctx
            .call_raw(PROGRAM, abi::encode(&Op::Request(request)))
            .await;
        let refused = answer.as_deref() == Ok(b"refuse me".as_slice());
        if refused {
            return Err(::program::invalid("the asker refuses this answer"));
        }
        let mut answers = ANSWERS.get(&ctx)?.unwrap_or_default();
        answers.push(answer);
        ANSWERS.put(&ctx, &answers);
        Ok(())
    }
}

fn chain() -> MockChain {
    let mut chain = MockChain::default();
    chain.seat::<Valset>("valset");
    chain.seat::<awaits::Awaits>(awaits::PROGRAM);
    chain.seat::<Provider>(PROGRAM);
    chain.seat_awaiting::<Asker>(ASKER);
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

fn request(service: &str, input: &[u8]) -> Request {
    Request {
        service: service.into(),
        input: input.to_vec(),
        parts: Vec::new(),
    }
}

fn ask(chain: &MockChain, service: &str, input: &[u8]) {
    let ask = Ask::Call(request(service, input));
    chain
        .submit(Origin::Signed(STRANGER.to_vec()), ASKER, &ask)
        .unwrap();
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

fn answers(chain: &MockChain) -> Answers {
    chain.query(ASKER, &()).unwrap()
}

fn ids(chain: &MockChain, key: [u8; 32]) -> Vec<u64> {
    assigned(chain, key).iter().map(|row| row.id).collect()
}

fn open_row(chain: &MockChain, id: u64) -> Option<RequestRow> {
    let Reply::Request(row) = chain.query(PROGRAM, &Query::Request { id }).unwrap() else {
        panic!()
    };
    row
}

fn reassign(chain: &MockChain, id: u64) -> Result<Vec<u8>, Error> {
    chain.submit(
        Origin::Signed(STRANGER.to_vec()),
        PROGRAM,
        &Op::Reassign { id },
    )
}

#[test]
fn the_election_takes_turns_and_only_the_elected_provider_answers() {
    let chain = chain();
    serve(&chain, ADA, &["anthropic", RELAY]).unwrap();
    serve(&chain, BO, &[RELAY]).unwrap();
    for _ in 0..3 {
        ask(&chain, RELAY, b"ping");
    }
    ask(&chain, "anthropic", b"model");
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
    let [pong, failed] = answers(&chain).try_into().unwrap();
    assert_eq!(pong.unwrap(), b"pong");
    let failed = failed.unwrap_err();
    assert_eq!(
        (failed.code.as_str(), failed.message.as_str()),
        (
            code::WRONG_STATE,
            "the anthropic provider failed: upstream timed out"
        )
    );
    assert!(assigned(&chain, BO).is_empty());
    let twice = answer(&chain, BO, 2, Answer::Output(Vec::new())).unwrap_err();
    assert_eq!(twice.code, code::NOT_FOUND);
}

#[test]
fn only_a_member_node_serves_and_only_a_program_calling_requests() {
    let chain = chain();
    assert_eq!(
        serve(&chain, STRANGER, &[RELAY]).unwrap_err().code,
        code::UNAUTHORIZED
    );
    ask(&chain, RELAY, b"");
    let [unserved] = answers(&chain).try_into().unwrap();
    assert_eq!(unserved.unwrap_err().code, code::NOT_FOUND);
    serve(&chain, ADA, &[RELAY]).unwrap();
    let direct = Op::Request(request(RELAY, b""));
    let signed = chain
        .submit(Origin::Signed(ADA.to_vec()), PROGRAM, &direct)
        .unwrap_err();
    assert_eq!(signed.code, code::UNAUTHORIZED);
    let emitted = chain
        .submit(
            Origin::Signed(STRANGER.to_vec()),
            ASKER,
            &Ask::Emit(request(RELAY, b"")),
        )
        .unwrap_err();
    assert_eq!(emitted.code, code::WRONG_STATE);
    assert!(assigned(&chain, ADA).is_empty());
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
fn an_unanswered_request_expires_after_its_window_and_answers_the_call() {
    let chain = chain();
    serve(&chain, ADA, &[RELAY]).unwrap();
    ask(&chain, RELAY, b"");
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
    let [expired] = answers(&chain).try_into().unwrap();
    let expired = expired.unwrap_err();
    assert_eq!(expired.code, code::WRONG_STATE);
    assert!(
        expired
            .message
            .ends_with(&format!("expired at height {}", 2 + EXPIRY_BLOCKS))
    );
    let Reply::Request(gone) = chain.query(PROGRAM, &Query::Request { id: 1 }).unwrap() else {
        panic!()
    };
    assert_eq!(gone, None);
}

#[test]
fn an_answer_the_caller_refuses_still_closes_the_request() {
    let chain = chain();
    serve(&chain, ADA, &[RELAY]).unwrap();
    ask(&chain, RELAY, b"");
    chain.at(2, 0);
    answer(&chain, ADA, 1, Answer::Output(b"refuse me".to_vec())).unwrap();
    assert!(answers(&chain).is_empty());
    assert!(assigned(&chain, ADA).is_empty());
    assert_eq!(chain.frames(), 0);
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
    };
    chain
        .submit(
            Origin::Signed(STRANGER.to_vec()),
            ASKER,
            &Ask::Call(with_part(part)),
        )
        .unwrap();
    let [missing] = answers(&chain).try_into().unwrap();
    assert_eq!(missing.unwrap_err().code, code::NOT_FOUND);
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
            &Ask::Call(with_part(part)),
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

#[test]
fn a_node_that_drops_a_service_hands_its_requests_to_the_next_serving_node() {
    let chain = chain();
    serve(&chain, ADA, &["anthropic", RELAY]).unwrap();
    serve(&chain, BO, &[RELAY]).unwrap();
    ask(&chain, RELAY, b"ping");
    ask(&chain, RELAY, b"ping");
    ask(&chain, "anthropic", b"model");
    assert_eq!((ids(&chain, ADA), ids(&chain, BO)), (vec![1, 3], vec![2]));
    let deadline = open_row(&chain, 1).unwrap().deadline;

    serve(&chain, ADA, &["anthropic"]).unwrap();
    assert_eq!((ids(&chain, ADA), ids(&chain, BO)), (vec![3], vec![1, 2]));
    let moved = open_row(&chain, 1).unwrap();
    assert_eq!((moved.provider, moved.deadline), (BO.to_vec(), deadline));

    chain.at(2, 0);
    let old = answer(&chain, ADA, 1, Answer::Output(Vec::new())).unwrap_err();
    assert_eq!(old.code, code::UNAUTHORIZED);
    answer(&chain, BO, 1, Answer::Output(b"pong".to_vec())).unwrap();
    let [pong] = answers(&chain).try_into().unwrap();
    assert_eq!(pong.unwrap(), b"pong");
}

#[test]
fn reassign_hands_on_the_request_of_a_node_that_left_the_member_set() {
    let chain = chain();
    serve(&chain, ADA, &[RELAY]).unwrap();
    serve(&chain, BO, &[RELAY]).unwrap();
    ask(&chain, RELAY, b"ping");
    ask(&chain, RELAY, b"ping");
    assert_eq!(ids(&chain, BO), [2]);
    let deadline = open_row(&chain, 2).unwrap().deadline;
    chain
        .submit(Origin::Signed(BO.to_vec()), "valset", &valset::Op::Leave)
        .unwrap();

    reassign(&chain, 2).unwrap();
    assert_eq!((ids(&chain, ADA), ids(&chain, BO)), (vec![1, 2], vec![]));
    assert_eq!(open_row(&chain, 2).unwrap().deadline, deadline);
    chain.at(2, 0);
    let old = answer(&chain, BO, 2, Answer::Output(Vec::new())).unwrap_err();
    assert_eq!(old.code, code::UNAUTHORIZED);
    answer(&chain, ADA, 2, Answer::Output(b"pong".to_vec())).unwrap();
    let [pong] = answers(&chain).try_into().unwrap();
    assert_eq!(pong.unwrap(), b"pong");
}

#[test]
fn reassign_is_refused_while_the_elected_node_still_serves() {
    let chain = chain();
    serve(&chain, ADA, &[RELAY]).unwrap();
    serve(&chain, BO, &[RELAY]).unwrap();
    ask(&chain, RELAY, b"ping");
    let refused = reassign(&chain, 1).unwrap_err();
    assert_eq!(refused.code, code::WRONG_STATE);
    assert_eq!(ids(&chain, ADA), [1]);
    assert_eq!(reassign(&chain, 7).unwrap_err().code, code::NOT_FOUND);
}

#[test]
fn a_request_no_member_node_serves_any_longer_fails_the_call() {
    let chain = chain();
    serve(&chain, ADA, &[RELAY]).unwrap();
    ask(&chain, RELAY, b"ping");
    serve(&chain, ADA, &[]).unwrap();
    let [unserved] = answers(&chain).try_into().unwrap();
    let unserved = unserved.unwrap_err();
    assert_eq!(
        (unserved.code.as_str(), unserved.message.as_str()),
        (code::NOT_FOUND, "no member node serves relay")
    );
    assert_eq!(open_row(&chain, 1), None);
    assert!(assigned(&chain, ADA).is_empty());
}
