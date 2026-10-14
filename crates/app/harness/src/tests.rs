//! The agents on a `MockChain` with identity, attribution, awaits, chat, files, provider and
//! valset seated: a mention or a direct message reaches a hired agent, a provider node answers
//! as the model would, and every tool call runs once its relay comes back, as the agent.
use ::program::{Error, MockChain, Origin, Principal, code};
use provider::{Answer, Provider, RELAY, RequestRow, http};
use serde_json::{Value, json};
use valset::{Genesis, Member, Valset};

use crate::*;

const NODE: [u8; 32] = [1; 32];
const ADA: &[u8] = b"ada-key";
const BO: &[u8] = b"bo-key";

struct World {
    chain: MockChain,
    ada: AccountNumber,
    bo: AccountNumber,
    agent: AccountNumber,
}

fn signed(key: &[u8]) -> Origin {
    Origin::Signed(key.to_vec())
}

fn number(output: Vec<u8>) -> AccountNumber {
    abi::decode(&output).unwrap()
}

fn identity_op(chain: &MockChain, key: &[u8], op: identity::Op) -> Result<Vec<u8>, Error> {
    chain.submit(signed(key), identity::PROGRAM, &op)
}

fn hire(agent: AccountNumber) -> Op {
    Op::Hire {
        agent,
        system: "Keep notes in /shared.".into(),
        model: "claude-test".into(),
        max_tokens: 1024,
    }
}

fn world() -> World {
    let mut chain = MockChain::default();
    chain.seat::<identity::Identity>(identity::PROGRAM);
    chain.seat::<Valset>("valset");
    chain.seat::<awaits::Awaits>(awaits::PROGRAM);
    chain.seat::<Provider>(provider::PROGRAM);
    chain.seat::<attribution::Attribution>(attribution::PROGRAM);
    chain.seat_awaiting::<Harness>(PROGRAM);
    chain.seat::<files::Files>(files::PROGRAM);
    chain.seat::<chat::Chat>(chat::PROGRAM);
    for program in [
        "valset",
        provider::PROGRAM,
        attribution::PROGRAM,
        PROGRAM,
        files::PROGRAM,
        chat::PROGRAM,
    ] {
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
    let serve = provider::Op::Serve {
        services: vec![MODEL_SERVICE.into(), RELAY.into()],
    };
    chain
        .submit(signed(&NODE), provider::PROGRAM, &serve)
        .unwrap();
    let person = |key, name: &str| {
        let create = identity::Op::Create {
            name: name.into(),
            scheme: ::program::Scheme::Ed25519,
        };
        number(identity_op(&chain, key, create).unwrap())
    };
    let ada = person(ADA, "Ada");
    let bo = person(BO, "Bo");
    let create = identity::Op::CreateAgent {
        name: "Scout".into(),
    };
    let agent = number(identity_op(&chain, ADA, create).unwrap());
    let hand = identity::Op::SetRunner {
        account: agent,
        runner: Some(PROGRAM.into()),
    };
    identity_op(&chain, ADA, hand).unwrap();
    chain.submit(signed(ADA), PROGRAM, &hire(agent)).unwrap();
    let general = chat::Op::CreateChannel {
        channel_id: "general".into(),
        name: "general".into(),
        post_policy: chat::PostPolicy::Open,
    };
    chain.submit(signed(ADA), chat::PROGRAM, &general).unwrap();
    World {
        chain,
        ada,
        bo,
        agent,
    }
}

impl World {
    fn post(&self, key: &[u8], channel: &str, id: &str, text: &str) {
        let op = chat::Op::PostMessage {
            channel_id: channel.into(),
            message_id: id.into(),
            blocks: chat::parse_message(text),
            thread: None,
        };
        self.chain.submit(signed(key), chat::PROGRAM, &op).unwrap();
    }

    fn mention(&self, id: &str, text: &str) -> u64 {
        self.post(ADA, "general", id, &format!("<@{}> {text}", self.agent));
        self.threads()[0].id
    }

    fn threads(&self) -> Vec<ThreadRow> {
        let asked = Query::Threads {
            agent: self.agent,
            page: PageRequest::default(),
        };
        let Reply::Threads(page) = self.chain.query(PROGRAM, &asked).unwrap() else {
            panic!()
        };
        page.items
    }
}

fn assigned(chain: &MockChain, service: &str) -> Vec<RequestRow> {
    let asked = provider::Query::Assigned {
        key: NODE.to_vec(),
        page: PageRequest::default(),
    };
    let provider::Reply::Requests(page) = chain.query(provider::PROGRAM, &asked).unwrap() else {
        panic!()
    };
    page.items
        .into_iter()
        .filter(|row| row.service == service)
        .collect()
}

fn body(chain: &MockChain, row: &RequestRow) -> Value {
    let host = chain.host(provider::PROGRAM).borrow();
    let mut bytes = Vec::new();
    for part in &row.parts {
        bytes.extend(&host.blobs[part].body);
    }
    serde_json::from_slice(&bytes).expect("the parts join into one JSON request")
}

fn reply(content: Value, stop: &str) -> Vec<u8> {
    let body = json!({
        "id": "msg_1",
        "type": "message",
        "role": "assistant",
        "model": "claude-test",
        "content": content,
        "stop_reason": stop,
        "usage": { "input_tokens": 10, "output_tokens": 5, "cache_read_input_tokens": 3 },
    });
    abi::encode(&http::Response {
        status: 200,
        headers: Vec::new(),
        body: body.to_string().into_bytes(),
    })
}

fn answer(chain: &MockChain, id: u64, answer: Answer) {
    let op = provider::Op::Answer { id, answer };
    chain
        .submit(Origin::Signed(NODE.to_vec()), provider::PROGRAM, &op)
        .unwrap();
}

fn model_says(chain: &MockChain, content: Value, stop: &str) {
    let [request] = assigned(chain, MODEL_SERVICE)
        .try_into()
        .expect("one model call open");
    answer(chain, request.id, Answer::Output(reply(content, stop)));
}

fn relay_back(chain: &MockChain, call: &str) {
    let rows = assigned(chain, RELAY);
    let row = rows
        .iter()
        .find(|row| row.input == call.as_bytes())
        .expect("the call is relaying");
    answer(chain, row.id, Answer::Output(Vec::new()));
}

fn frames(chain: &MockChain, thread: u64) -> Vec<Body> {
    let asked = Query::Frames {
        thread,
        page: PageRequest::default(),
    };
    let Reply::Frames(page) = chain.query(PROGRAM, &asked).unwrap() else {
        panic!()
    };
    page.items.into_iter().map(|frame| frame.body).collect()
}

fn held(chain: &MockChain, thread: u64) -> Vec<Body> {
    let Reply::Held(held) = chain.query(PROGRAM, &Query::Held { thread }).unwrap() else {
        panic!()
    };
    held
}

fn result_of(chain: &MockChain, thread: u64, call: &str) -> (String, bool) {
    frames(chain, thread)
        .into_iter()
        .chain(held(chain, thread))
        .find_map(|body| match body {
            Body::Result {
                call: done,
                text,
                failed,
                ..
            } if done == call => Some((text, failed)),
            _ => None,
        })
        .expect("the call has a result")
}

fn thread_row(chain: &MockChain, thread: u64) -> ThreadRow {
    let Reply::Thread(Some(row)) = chain.query(PROGRAM, &Query::Thread { thread }).unwrap() else {
        panic!()
    };
    row
}

fn tool_use(id: &str, name: &str, input: Value) -> Value {
    json!({ "type": "tool_use", "id": id, "name": name, "input": input })
}

#[test]
fn a_mention_by_its_manager_asks_the_model_in_the_agents_thread_for_that_channel() {
    let world = world();
    let chain = &world.chain;
    let thread = world.mention("m1", "note that the network is up");
    let [request] = assigned(chain, MODEL_SERVICE).try_into().unwrap();
    let head: http::Head = abi::decode(&request.input).unwrap();
    assert_eq!(
        (head.method, head.path.as_str()),
        (http::Method::Post, "/v1/messages")
    );
    head.check().unwrap();
    let sent = body(chain, &request);
    assert_eq!(sent["model"], "claude-test");
    let system = sent["system"][0]["text"].as_str().unwrap();
    assert!(
        system.starts_with(&format!(
            "You are Scout, an agent on a ducktape network: account {}",
            world.agent
        )),
        "{system}"
    );
    assert!(system.ends_with("Keep notes in /shared."), "{system}");
    assert_eq!(
        sent["tools"].as_array().unwrap().len(),
        tools::CATALOG.len()
    );
    let text = sent["messages"][0]["content"][0]["text"].as_str().unwrap();
    assert!(
        text.starts_with(&format!(
            "Ada (account {}) mentioned you in chat channel general, message 1",
            world.ada
        )),
        "{text}"
    );
    assert!(text.ends_with("note that the network is up"), "{text}");
    let row = thread_row(chain, thread);
    assert_eq!(
        (row.agent, row.manager, row.place.as_str()),
        (world.agent, world.ada, "chat/general")
    );

    world.mention("m2", "still there?");
    assert_eq!(world.threads().len(), 1, "one thread per channel");
    assert_eq!(
        assigned(chain, MODEL_SERVICE).len(),
        1,
        "one model call at a time"
    );
    assert!(
        matches!(&held(chain, thread)[..], [Body::Post { text, .. }] if text.ends_with("still there?"))
    );
    chain.at(2, 0);
    model_says(
        chain,
        json!([{ "type": "text", "text": "Hi." }]),
        "end_turn",
    );
    let order: Vec<Option<Role>> = frames(chain, thread).iter().map(Body::role).collect();
    assert_eq!(
        order,
        [Some(Role::User), Some(Role::Assistant), Some(Role::User)]
    );
    assert_eq!(
        assigned(chain, MODEL_SERVICE).len(),
        1,
        "the held mention asks again"
    );
}

#[test]
fn a_direct_message_reaches_the_agent_and_only_its_audience_is_heard() {
    let world = world();
    let chain = &world.chain;
    world.post(
        BO,
        "general",
        "b1",
        &format!("<@{}> do my bidding", world.agent),
    );
    assert!(world.threads().is_empty(), "Bo is not in Scout's audience");

    let open = chat::Op::CreateDmChannel {
        counterpart: world.agent,
        name: "scout".into(),
    };
    chain.submit(signed(ADA), chat::PROGRAM, &open).unwrap();
    let room = chat::dm_channel_id(world.ada, world.agent);
    world.post(ADA, &room, "d1", &format!("a word, <@{}>", world.agent));
    let [thread] = world.threads().try_into().unwrap();
    assert_eq!(thread.place, format!("chat/{room}"));
    let posts: Vec<String> = frames(chain, thread.id)
        .into_iter()
        .filter_map(|body| match body {
            Body::Post { text, .. } => Some(text),
            _ => None,
        })
        .collect();
    assert_eq!(
        posts.len(),
        1,
        "a mention in a direct message is heard once"
    );
    assert!(posts[0].contains("mentioned you"), "{}", posts[0]);

    let audience = identity::Op::SetAudience {
        account: world.agent,
        audience: identity::Audience::Accounts(vec![world.bo]),
    };
    identity_op(chain, ADA, audience).unwrap();
    world.post(BO, "general", "b2", &format!("<@{}> now?", world.agent));
    assert_eq!(world.threads().len(), 2);
}

#[test]
fn only_its_manager_hires_an_agent_handed_to_harness_and_a_dismissed_one_hears_nothing() {
    let world = world();
    let chain = &world.chain;
    let stranger = chain.submit(signed(BO), PROGRAM, &hire(world.agent));
    assert_eq!(stranger.unwrap_err().code, code::UNAUTHORIZED);
    let person = chain.submit(signed(ADA), PROGRAM, &hire(world.bo));
    assert_eq!(person.unwrap_err().code, code::WRONG_STATE);
    let create = identity::Op::CreateAgent {
        name: "Idle".into(),
    };
    let idle = number(identity_op(chain, ADA, create).unwrap());
    let unhanded = chain.submit(signed(ADA), PROGRAM, &hire(idle));
    assert_eq!(unhanded.unwrap_err().code, code::WRONG_STATE);

    let dismissed = chain.submit(signed(BO), PROGRAM, &Op::Dismiss { agent: world.agent });
    assert_eq!(dismissed.unwrap_err().code, code::UNAUTHORIZED);
    chain
        .submit(signed(ADA), PROGRAM, &Op::Dismiss { agent: world.agent })
        .unwrap();
    world.post(ADA, "general", "m1", &format!("<@{}> hello?", world.agent));
    assert!(world.threads().is_empty());
}

#[test]
fn every_tool_call_runs_a_block_later_and_the_model_is_asked_once_all_results_are_back() {
    let world = world();
    let chain = &world.chain;
    let thread = world.mention("m1", "note it");
    chain.at(2, 0);
    let calls = json!([
        { "type": "text", "text": "On it." },
        tool_use("toolu_w", "files_write", json!({ "path": "/shared/note.md", "content": "hello" })),
        tool_use("toolu_l", "files_list", json!({ "path": "/shared" })),
        tool_use("toolu_x", "no_such_tool", json!({})),
        tool_use("toolu_bad", "files_read", json!({ "offset": 3 })),
    ]);
    model_says(chain, calls, "tool_use");
    let bodies = frames(chain, thread);
    assert!(matches!(&bodies[2], Body::Refused { call, .. } if call == "toolu_x"));
    assert!(matches!(&bodies[3], Body::Refused { problem, .. } if problem.contains("path")));
    assert_eq!(bodies.len(), 4);
    assert_eq!(assigned(chain, RELAY).len(), 2);
    assert!(
        assigned(chain, MODEL_SERVICE).is_empty(),
        "the model waits for the results"
    );

    let same_block = provider::Op::Answer {
        id: assigned(chain, RELAY)[0].id,
        answer: Answer::Output(Vec::new()),
    };
    let refused = chain.submit(signed(&NODE), provider::PROGRAM, &same_block);
    assert_eq!(refused.unwrap_err().code, code::WRONG_STATE);

    chain.at(3, 0);
    relay_back(chain, "toolu_l");
    let (listed, failed) = result_of(chain, thread, "toolu_l");
    assert!(!failed);
    assert_eq!(
        serde_json::from_str::<Value>(&listed).unwrap()["entries"],
        json!([])
    );
    assert!(assigned(chain, MODEL_SERVICE).is_empty());
    relay_back(chain, "toolu_w");
    let (written, failed) = result_of(chain, thread, "toolu_w");
    assert!(!failed, "{written}");
    let asked = files::Query::Read {
        path: "/shared/note.md".into(),
        at: None,
        offset: 0,
        len: 100,
    };
    let files::Reply::Contents(contents) = chain.query(files::PROGRAM, &asked).unwrap() else {
        panic!()
    };
    assert_eq!(contents.bytes, b"hello");
    let files::Reply::Head(Some(head)) = chain.query(files::PROGRAM, &files::Query::Head).unwrap()
    else {
        panic!()
    };
    assert_eq!(
        head.author,
        Principal::Account(world.agent),
        "the agent is the writer"
    );
    assert_eq!(
        serde_json::from_str::<Value>(&written).unwrap()["snapshot"],
        abi::hex(head.id.digest())
    );
    assert_eq!(thread_row(chain, thread).open_calls, 0);

    let [next] = assigned(chain, MODEL_SERVICE).try_into().unwrap();
    let sent = body(chain, &next);
    let messages = sent["messages"].as_array().unwrap();
    assert_eq!(messages.len(), 3);
    assert_eq!(messages[1]["content"][1]["type"], "tool_use");
    let results = messages[2]["content"].as_array().unwrap();
    let answered: Vec<(&str, bool)> = results
        .iter()
        .map(|block| {
            assert_eq!(block["type"], "tool_result");
            (
                block["tool_use_id"].as_str().unwrap(),
                block["is_error"].as_bool().unwrap(),
            )
        })
        .collect();
    assert_eq!(
        answered,
        [
            ("toolu_x", true),
            ("toolu_bad", true),
            ("toolu_l", false),
            ("toolu_w", false)
        ]
    );
}

#[test]
fn input_that_arrives_while_a_call_runs_is_held_and_follows_its_result() {
    let world = world();
    let chain = &world.chain;
    let thread = world.mention("m1", "look");
    chain.at(2, 0);
    let call = tool_use("toolu_l", "files_list", json!({ "path": "/" }));
    model_says(chain, json!([call]), "tool_use");
    world.mention("m2", "and also");
    assert_eq!(held(chain, thread).len(), 1);
    chain.at(3, 0);
    relay_back(chain, "toolu_l");
    let [again] = assigned(chain, MODEL_SERVICE).try_into().unwrap();
    let sent = body(chain, &again);
    let last = sent["messages"].as_array().unwrap().last().unwrap().clone();
    assert_eq!(last["role"], "user");
    assert_eq!(last["content"][0]["type"], "tool_result");
    let text = last["content"][1]["text"].as_str().unwrap();
    assert!(text.ends_with("and also"), "{text}");
    chain.at(4, 0);
    model_says(
        chain,
        json!([{ "type": "text", "text": "Done." }]),
        "end_turn",
    );
    assert!(assigned(chain, MODEL_SERVICE).is_empty());
    let row = thread_row(chain, thread);
    assert_eq!((row.asking, row.due()), (Asking::Idle, false));
    assert_eq!(chain.frames(), 0);
}

#[test]
fn a_call_whose_relay_never_comes_back_fails_after_the_attempts() {
    let world = world();
    let chain = &world.chain;
    let thread = world.mention("m1", "look");
    chain.at(2, 0);
    let call = tool_use("toolu_l", "files_list", json!({ "path": "/" }));
    model_says(chain, json!([call]), "tool_use");
    for attempt in 0..MAX_ATTEMPTS {
        chain.at(3 + u64::from(attempt), 0);
        let [row] = assigned(chain, RELAY).try_into().unwrap();
        answer(chain, row.id, Answer::Failed("no route".into()));
    }
    assert!(assigned(chain, RELAY).is_empty());
    let (text, failed) = result_of(chain, thread, "toolu_l");
    assert!(failed && text.contains("no provider relayed"), "{text}");
    assert_eq!(assigned(chain, MODEL_SERVICE).len(), 1);
}

#[test]
fn a_call_in_flight_when_its_agent_is_dismissed_does_not_run() {
    let world = world();
    let chain = &world.chain;
    let thread = world.mention("m1", "write");
    chain.at(2, 0);
    let write = tool_use(
        "toolu_w",
        "files_write",
        json!({ "path": "/shared/late.md", "content": "x" }),
    );
    model_says(chain, json!([write]), "tool_use");
    chain
        .submit(signed(ADA), PROGRAM, &Op::Dismiss { agent: world.agent })
        .unwrap();
    chain.at(3, 0);
    relay_back(chain, "toolu_w");
    let (text, failed) = result_of(chain, thread, "toolu_w");
    assert!(failed && text.contains("dismissed"), "{text}");
    let asked = files::Query::Stat {
        path: "/shared/late.md".into(),
        at: None,
    };
    let files::Reply::Stat { entry, .. } = chain.query(files::PROGRAM, &asked).unwrap() else {
        panic!()
    };
    assert_eq!(entry, None, "the dismissed agent wrote nothing");
    assert!(assigned(chain, MODEL_SERVICE).is_empty());
    assert_eq!(chain.frames(), 0);
}

#[test]
fn a_write_is_the_agents_and_reaches_only_where_the_agent_may_write() {
    let world = world();
    let chain = &world.chain;
    let closed = chat::Op::CreateChannel {
        channel_id: "closed".into(),
        name: "closed".into(),
        post_policy: chat::PostPolicy::MembersOnly,
    };
    chain.submit(signed(BO), chat::PROGRAM, &closed).unwrap();
    let thread = world.mention("m1", "post");
    chain.at(2, 0);
    let calls = json!([
        tool_use(
            "toolu_p",
            "chat_post",
            json!({ "channel": "general", "text": "hi **all**", "reply_to": 1 })
        ),
        tool_use(
            "toolu_c",
            "chat_post",
            json!({ "channel": "closed", "text": "hi" })
        ),
        tool_use(
            "toolu_e",
            "files_write",
            json!({ "path": "/etc/motd", "content": "x" })
        ),
    ]);
    model_says(chain, calls, "tool_use");
    chain.at(3, 0);
    for call in ["toolu_p", "toolu_c", "toolu_e"] {
        relay_back(chain, call);
    }
    assert!(!result_of(chain, thread, "toolu_p").1);
    let (closed, failed) = result_of(chain, thread, "toolu_c");
    assert!(failed && closed.starts_with(code::UNAUTHORIZED), "{closed}");
    let (etc, failed) = result_of(chain, thread, "toolu_e");
    assert!(failed && etc.starts_with(code::UNAUTHORIZED), "{etc}");
    let asked = chat::Query::MessageById {
        message_id: format!("harness-{thread}-toolu_p"),
    };
    let chat::Reply::Message(Some(posted)) = chain.query(chat::PROGRAM, &asked).unwrap() else {
        panic!()
    };
    assert_eq!(posted.author, Principal::Account(world.agent));
    assert_eq!((posted.text.as_str(), posted.thread), ("hi all", Some(1)));

    let take_back = identity::Op::SetRunner {
        account: world.agent,
        runner: None,
    };
    identity_op(chain, ADA, take_back).unwrap();
    chain.at(4, 0);
    let again = tool_use(
        "toolu_q",
        "chat_post",
        json!({ "channel": "general", "text": "still me?" }),
    );
    model_says(chain, json!([again]), "tool_use");
    chain.at(5, 0);
    relay_back(chain, "toolu_q");
    let (taken, failed) = result_of(chain, thread, "toolu_q");
    assert!(failed && taken.starts_with(code::UNAUTHORIZED), "{taken}");
}

#[test]
fn a_failed_model_call_is_noticed_and_tried_again_up_to_the_limit() {
    let world = world();
    let chain = &world.chain;
    let thread = world.mention("m1", "hello");
    for attempt in 0..MAX_ATTEMPTS {
        chain.at(2 + u64::from(attempt), 0);
        let [request] = assigned(chain, MODEL_SERVICE).try_into().unwrap();
        let overloaded = http::Response {
            status: 529,
            headers: Vec::new(),
            body: br#"{"type":"error","error":{"type":"overloaded_error","message":"Overloaded"}}"#
                .to_vec(),
        };
        answer(chain, request.id, Answer::Output(abi::encode(&overloaded)));
    }
    assert!(assigned(chain, MODEL_SERVICE).is_empty());
    let notices: Vec<String> = frames(chain, thread)
        .into_iter()
        .filter_map(|body| match body {
            Body::Notice { text } => Some(text),
            _ => None,
        })
        .collect();
    assert_eq!(notices.len(), MAX_ATTEMPTS as usize);
    assert!(
        notices[0].contains("overloaded_error: Overloaded"),
        "{}",
        notices[0]
    );
    world.mention("m2", "again");
    assert_eq!(assigned(chain, MODEL_SERVICE).len(), 1);
}

#[test]
fn only_attribution_delivers_and_only_the_manager_closes() {
    let world = world();
    let chain = &world.chain;
    let thread = world.mention("m1", "hi");
    let change = attribution::Change {
        seq: 99,
        source: attribution::Source {
            program: chat::PROGRAM.into(),
            object: attribution::Object {
                kind: "message".into(),
                id: "m1".into(),
            },
        },
        revision: 9,
        recipient: world.agent,
        reason: attribution::Reason::Mention,
        kind: attribution::Kind::Added,
        detail: Vec::new(),
        actor: attribution::Actor::Account(world.ada),
        cause: attribution::Cause::Direct,
        height: 1,
    };
    let forged = Op::Attributed(Box::new(change));
    assert_eq!(
        chain
            .submit(signed(ADA), PROGRAM, &forged)
            .unwrap_err()
            .code,
        code::UNAUTHORIZED
    );
    let close = Op::Close { thread };
    assert_eq!(
        chain.submit(signed(BO), PROGRAM, &close).unwrap_err().code,
        code::UNAUTHORIZED
    );
    chain.submit(signed(ADA), PROGRAM, &close).unwrap();
    let thread = world.mention("m2", "a fresh start");
    assert_eq!(
        world.threads().len(),
        2,
        "a closed thread's place opens a new one"
    );
    assert_eq!(frames(chain, thread).len(), 1);
}

#[test]
fn with_no_provider_serving_the_model_a_mention_says_so() {
    let world = world();
    let chain = &world.chain;
    let stop = provider::Op::Serve {
        services: Vec::new(),
    };
    chain
        .submit(signed(&NODE), provider::PROGRAM, &stop)
        .unwrap();
    let thread = world.mention("m1", "hello");
    assert!(
        matches!(&frames(chain, thread)[1], Body::Notice { text } if text.contains("no member node serves"))
    );
}

#[test]
fn every_tool_schema_is_an_object_and_names_its_required_fields() {
    for entry in tools::CATALOG.iter() {
        let schema = (entry.schema)();
        assert_eq!(schema["type"], "object", "{}", entry.name);
        let empty = (entry.check)(&json!({}));
        let required = schema["required"].as_array().unwrap();
        assert_eq!(empty.is_ok(), required.is_empty(), "{}", entry.name);
        assert!(entry.name.len() <= 64 && !entry.description.is_empty());
    }
}
