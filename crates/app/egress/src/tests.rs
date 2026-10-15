//! egress on a `MockChain` beside identity, valset, the awaits role and a program that calls it:
//! registering and assigning a service, serving, the election's turns, answers that resolve the
//! call, refusals and expiry.
use ::program::{
    Awaiting, Blob, Ctx, Error, HashKind, MockChain, Origin, Program, QueryCtx, Reply as Wanted,
    Scheme, blob_id, code,
};
use borsh::{BorshDeserialize, BorshSerialize};
use store::Item;
use valset::{Genesis, Member, Valset};

use crate::*;

const ADA: [u8; 32] = [1; 32];
const BO: [u8; 32] = [2; 32];
const STRANGER: [u8; 32] = [9; 32];
const OWNER: &[u8] = b"owner-key";
const OTHER: &[u8] = b"other-key";
const ASKER: &str = "asker";
const SERVICE: &str = "ccs";
const MESSAGES: &str = "https://api.anthropic.com/v1/messages";

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
    chain.seat::<identity::Identity>(identity::PROGRAM);
    chain.seat::<Valset>("valset");
    chain.seat::<awaits::Awaits>(awaits::PROGRAM);
    chain.seat::<Egress>(PROGRAM);
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
    for (key, name) in [(OWNER, "Owner"), (OTHER, "Other")] {
        let create = identity::Op::Create {
            name: name.into(),
            scheme: Scheme::Ed25519,
        };
        chain
            .submit(Origin::Signed(key.to_vec()), identity::PROGRAM, &create)
            .unwrap();
    }
    chain
}

fn submit(chain: &MockChain, key: &[u8], op: &Op) -> Result<Vec<u8>, Error> {
    chain.submit(Origin::Signed(key.to_vec()), PROGRAM, op)
}

fn serve(chain: &MockChain, key: [u8; 32], services: &[&str]) -> Result<Vec<u8>, Error> {
    let services = services.iter().map(|service| service.to_string()).collect();
    submit(chain, &key, &Op::Serve { services })
}

fn relay(note: &[u8]) -> Request {
    Request::Relay {
        note: note.to_vec(),
    }
}

fn http_request(service: &str, target: &str, body: Vec<BlobId>) -> Request {
    Request::Http {
        service: service.into(),
        head: http::Head {
            method: http::Method::Post,
            target: target.into(),
            headers: vec![("anthropic-version".into(), "2023-06-01".into())],
        },
        body,
    }
}

fn ask(chain: &MockChain, request: Request) {
    chain
        .submit(
            Origin::Signed(STRANGER.to_vec()),
            ASKER,
            &Ask::Call(request),
        )
        .unwrap();
}

fn assigned(chain: &MockChain, key: [u8; 32]) -> Vec<RequestRow> {
    let asked = Query::Assigned { key: key.to_vec() };
    let Reply::Requests(rows) = chain.query(PROGRAM, &asked).unwrap() else {
        panic!()
    };
    rows
}

fn answer(chain: &MockChain, key: [u8; 32], id: u64, answer: Answer) -> Result<Vec<u8>, Error> {
    submit(chain, &key, &Op::Answer { id, answer })
}

fn answers(chain: &MockChain) -> Answers {
    chain.query(ASKER, &()).unwrap()
}

fn serving(chain: &MockChain, service: &str) -> Vec<Vec<u8>> {
    let asked = Query::Serving {
        service: service.into(),
    };
    let Reply::Serving(keys) = chain.query(PROGRAM, &asked).unwrap() else {
        panic!()
    };
    keys
}

#[test]
fn the_election_takes_turns_and_only_the_elected_node_answers() {
    let chain = chain();
    serve(&chain, ADA, &[HTTP, RELAY]).unwrap();
    serve(&chain, BO, &[RELAY]).unwrap();
    for _ in 0..3 {
        ask(&chain, relay(b"ping"));
    }
    ask(&chain, http_request(HTTP, MESSAGES, Vec::new()));
    let ada: Vec<u64> = assigned(&chain, ADA).iter().map(|row| row.id).collect();
    let bo: Vec<u64> = assigned(&chain, BO).iter().map(|row| row.id).collect();
    assert_eq!((ada, bo), (vec![1, 3, 4], vec![2]));
    let row = &assigned(&chain, BO)[0];
    assert_eq!(
        (row.requester.as_str(), &row.request, row.deadline),
        (ASKER, &relay(b"ping"), 1 + EXPIRY_BLOCKS)
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
            "the http service failed: upstream timed out"
        )
    );
    assert!(assigned(&chain, BO).is_empty());
    let twice = answer(&chain, BO, 2, Answer::Output(Vec::new())).unwrap_err();
    assert_eq!(twice.code, code::NOT_FOUND);
}

#[test]
fn a_registered_service_is_served_only_by_the_nodes_its_owner_assigns() {
    let chain = chain();
    let register = Op::Register {
        service: SERVICE.into(),
    };
    submit(&chain, OWNER, &register).unwrap();
    let again = submit(&chain, OTHER, &register).unwrap_err();
    assert_eq!(again.code, code::WRONG_STATE);
    for reserved in RESERVED {
        let taken = Op::Register {
            service: reserved.into(),
        };
        assert_eq!(
            submit(&chain, OWNER, &taken).unwrap_err().code,
            code::INVALID_INPUT,
            "{reserved}"
        );
    }
    let by_node = submit(&chain, &ADA, &register).unwrap_err();
    assert_eq!(by_node.code, code::UNAUTHORIZED);

    serve(&chain, ADA, &[SERVICE]).unwrap();
    serve(&chain, BO, &[SERVICE]).unwrap();
    assert!(
        serving(&chain, SERVICE).is_empty(),
        "nobody is assigned yet"
    );
    ask(&chain, http_request(SERVICE, "/v1/messages", Vec::new()));
    ask(&chain, http_request("nope", "/v1/messages", Vec::new()));
    let [unassigned, unregistered] = answers(&chain).try_into().unwrap();
    assert_eq!(unassigned.unwrap_err().code, code::NOT_FOUND);
    assert_eq!(unregistered.unwrap_err().code, code::NOT_FOUND);

    let assign = Op::Assign {
        service: SERVICE.into(),
        nodes: vec![ADA.to_vec(), STRANGER.to_vec()],
    };
    let not_owner = submit(&chain, OTHER, &assign).unwrap_err();
    assert_eq!(not_owner.code, code::UNAUTHORIZED);
    submit(&chain, OWNER, &assign).unwrap();
    assert_eq!(serving(&chain, SERVICE), [ADA.to_vec()]);
    for _ in 0..2 {
        ask(&chain, http_request(SERVICE, "/v1/messages", Vec::new()));
    }
    assert_eq!(assigned(&chain, ADA).len(), 2);
    assert!(assigned(&chain, BO).is_empty());
    let Reply::Service(Some(row)) = chain
        .query(
            PROGRAM,
            &Query::Service {
                name: SERVICE.into(),
            },
        )
        .unwrap()
    else {
        panic!()
    };
    assert_eq!(row.nodes, [ADA.to_vec(), STRANGER.to_vec()]);

    let full_url = http_request(SERVICE, MESSAGES, Vec::new());
    let relayed_as_http = http_request(RELAY, "/", Vec::new());
    let path_to_http = http_request(HTTP, "/v1/messages", Vec::new());
    for request in [full_url, relayed_as_http, path_to_http] {
        ask(&chain, request);
    }
    let refused = &answers(&chain)[2..];
    assert!(
        refused
            .iter()
            .all(|answer| answer.as_ref().unwrap_err().code == code::INVALID_INPUT),
        "{refused:?}"
    );
}

#[test]
fn only_a_member_node_serves_and_only_a_program_calling_requests() {
    let chain = chain();
    assert_eq!(
        serve(&chain, STRANGER, &[RELAY]).unwrap_err().code,
        code::UNAUTHORIZED
    );
    ask(&chain, relay(b""));
    let [unserved] = answers(&chain).try_into().unwrap();
    assert_eq!(unserved.unwrap_err().code, code::NOT_FOUND);
    serve(&chain, ADA, &[RELAY]).unwrap();
    let direct = Op::Request(relay(b""));
    let signed = submit(&chain, &ADA, &direct).unwrap_err();
    assert_eq!(signed.code, code::UNAUTHORIZED);
    let emitted = chain
        .submit(
            Origin::Signed(STRANGER.to_vec()),
            ASKER,
            &Ask::Emit(relay(b"")),
        )
        .unwrap_err();
    assert_eq!(emitted.code, code::WRONG_STATE);
    assert!(assigned(&chain, ADA).is_empty());
    assert_eq!(
        serve(&chain, ADA, &["Bad Name"]).unwrap_err().code,
        code::INVALID_INPUT
    );
    serve(&chain, ADA, &[]).unwrap();
    assert!(serving(&chain, RELAY).is_empty());
}

#[test]
fn an_unanswered_request_expires_after_its_window_and_answers_the_call() {
    let chain = chain();
    serve(&chain, ADA, &[RELAY]).unwrap();
    ask(&chain, relay(b""));
    let expire = Op::Expire { id: 1 };
    let early = submit(&chain, &STRANGER, &expire).unwrap_err();
    assert_eq!(early.code, code::WRONG_STATE);
    let overdue = |chain: &MockChain| {
        let Reply::Requests(due) = chain.query(PROGRAM, &Query::Overdue).unwrap() else {
            panic!()
        };
        due
    };
    chain.at(1 + EXPIRY_BLOCKS, 0);
    assert!(overdue(&chain).is_empty());
    chain.at(2 + EXPIRY_BLOCKS, 0);
    let late = answer(&chain, ADA, 1, Answer::Output(Vec::new())).unwrap_err();
    assert_eq!(late.code, code::WRONG_STATE);
    assert_eq!(overdue(&chain).len(), 1);
    submit(&chain, &STRANGER, &expire).unwrap();
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
    ask(&chain, relay(b""));
    chain.at(2, 0);
    answer(&chain, ADA, 1, Answer::Output(b"refuse me".to_vec())).unwrap();
    assert!(answers(&chain).is_empty());
    assert!(assigned(&chain, ADA).is_empty());
    assert_eq!(chain.frames(), 0);
}

#[test]
fn a_request_names_parts_already_on_the_network() {
    let chain = chain();
    serve(&chain, ADA, &[HTTP]).unwrap();
    let body = b"{\"model\":\"m\"}".to_vec();
    let part = blob_id(HashKind::Sha256, "part", &body).unwrap();
    let with_part = |part| http_request(HTTP, MESSAGES, vec![part]);
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
    assert_eq!(assigned(&chain, ADA)[0].request.parts(), [part]);
}

#[test]
fn a_head_names_a_url_or_a_path_and_leaves_framing_and_identity_to_the_node() {
    let head = |headers: Vec<(&str, &str)>, target: &str| http::Head {
        method: http::Method::Post,
        target: target.into(),
        headers: headers
            .into_iter()
            .map(|(name, value)| (name.into(), value.into()))
            .collect(),
    };
    for url in [MESSAGES, "http://127.0.0.1:8080/x?y=1"] {
        assert!(head(Vec::new(), url).check_url().is_ok(), "{url}");
    }
    for url in [
        "/v1/messages",
        "ftp://host/",
        "https:///path",
        "https://host/a b",
    ] {
        assert!(head(Vec::new(), url).check_url().is_err(), "{url}");
    }
    for path in [
        "/",
        "/v1/messages",
        "/v1/models/claude-test?beta=true",
        "/a..b/c.",
    ] {
        assert!(head(Vec::new(), path).check_path().is_ok(), "{path}");
    }
    for path in [
        "v1/messages",
        MESSAGES,
        "/../admin",
        "/v1/./x",
        "/v1/%2E%2e/x",
        "/v1/..",
        "/v1\\..\\x",
        "/a b",
    ] {
        assert!(head(Vec::new(), path).check_path().is_err(), "{path}");
    }
    for name in ["authorization", "x-api-key", "cookie"] {
        assert!(head(vec![(name, "x")], "/").check_path().is_ok(), "{name}");
    }
    for name in ["host", "content-length", "connection", "ducktape-program"] {
        assert!(head(vec![(name, "x")], "/").check_path().is_err(), "{name}");
    }
    assert!(head(vec![("X-Upper", "x")], MESSAGES).check_url().is_err());
}
