//! The agents on a `MockChain` with identity, attribution, awaits, scheduler, chat, files, egress
//! and valset seated: a mention or a direct message reaches a hired agent, the node serving the
//! model's egress service answers as the model would, and every tool call runs once its relay
//! comes back, as the agent.
use ::program::{Error, MockChain, Origin, Principal, code};
use egress::{Answer, DEFAULT_WAIT, Egress, RELAY, Request, RequestRow, http};
use serde_json::{Value, json};
use valset::{Genesis, Member, Valset};

use crate::*;

const NODE: [u8; 32] = [1; 32];
const WINDOW: u64 = 200_000;
const SERVICE: &str = "ccs";
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
        service: SERVICE.into(),
        max_tokens: 1024,
    }
}

fn world() -> World {
    let mut chain = MockChain::default();
    chain.seat::<identity::Identity>(identity::PROGRAM);
    chain.seat::<Valset>("valset");
    chain.seat::<awaits::Awaits>(awaits::PROGRAM);
    chain.seat::<scheduler::Scheduler>(scheduler::PROGRAM);
    chain.seat_awaiting::<Egress>(egress::PROGRAM);
    chain.seat::<attribution::Attribution>(attribution::PROGRAM);
    chain.seat_awaiting::<Harness>(PROGRAM);
    chain.seat::<files::Files>(files::PROGRAM);
    chain.seat::<chat::Chat>(chat::PROGRAM);
    for program in [
        "valset",
        egress::PROGRAM,
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
    let serve = egress::Op::Serve {
        services: vec![SERVICE.into(), RELAY.into()],
    };
    chain
        .submit(signed(&NODE), egress::PROGRAM, &serve)
        .unwrap();
    let person = |key, name: &str| {
        let create = identity::Op::Create {
            name: name.into(),
            scheme: ::program::Scheme::Ed25519,
        };
        number(identity_op(&chain, key, create).unwrap())
    };
    let ada = person(ADA, "Ada");
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

    fn opened(&self, id: &str, text: &str, window: Option<u64>) -> u64 {
        let thread = self.mention(id, text);
        self.chain.at(2, 0);
        learn(&self.chain, window);
        thread
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
    let asked = egress::Query::Assigned { key: NODE.to_vec() };
    let egress::Reply::Requests(rows) = chain.query(egress::PROGRAM, &asked).unwrap() else {
        panic!()
    };
    rows.into_iter()
        .filter(|row| row.request.service() == service)
        .collect()
}

fn body(chain: &MockChain, row: &RequestRow) -> Value {
    let host = chain.host(egress::PROGRAM).borrow();
    let mut bytes = Vec::new();
    for part in row.request.parts() {
        bytes.extend(&host.blobs[part].body);
    }
    serde_json::from_slice(&bytes).expect("the parts join into one JSON request")
}

fn response(status: u16, body: Value) -> Vec<u8> {
    abi::encode(&http::Response {
        status,
        headers: Vec::new(),
        body: body.to_string().into_bytes(),
    })
}

fn reply(content: Value, stop: &str, input_tokens: u64) -> Vec<u8> {
    let body = json!({
        "id": "msg_1",
        "type": "message",
        "role": "assistant",
        "model": "claude-test",
        "content": content,
        "stop_reason": stop,
        "usage": { "input_tokens": input_tokens, "output_tokens": 5, "cache_read_input_tokens": 3 },
    });
    response(200, body)
}

fn open_request(chain: &MockChain) -> (RequestRow, http::Head) {
    let [request] = assigned(chain, SERVICE)
        .try_into()
        .expect("one model call open");
    let Request::Http { head, .. } = request.request.clone() else {
        panic!("the model is asked over http");
    };
    (request, head)
}

fn path(head: &http::Head) -> &str {
    &head.target
}

fn learn(chain: &MockChain, window: Option<u64>) {
    let (request, head) = open_request(chain);
    assert_eq!(
        (head.method, path(&head)),
        (http::Method::Get, "/v1/models/claude-test")
    );
    let info = json!({ "type": "model", "id": "claude-test", "max_input_tokens": window });
    answer(chain, request.id, Answer::Output(response(200, info)));
}

fn answer(chain: &MockChain, id: u64, answer: Answer) {
    let op = egress::Op::Answer { id, answer };
    chain
        .submit(Origin::Signed(NODE.to_vec()), egress::PROGRAM, &op)
        .unwrap();
}

fn model_says(chain: &MockChain, content: Value, stop: &str) {
    model_says_after(chain, content, stop, 10);
}

fn model_says_after(chain: &MockChain, content: Value, stop: &str, input_tokens: u64) {
    let (request, head) = open_request(chain);
    assert_eq!(path(&head), "/v1/messages");
    let said = reply(content, stop, input_tokens);
    answer(chain, request.id, Answer::Output(said));
}

fn relay_back(chain: &MockChain, call: &str) {
    let rows = assigned(chain, RELAY);
    let row = rows
        .iter()
        .find(|row| {
            row.request
                == Request::Relay {
                    note: call.as_bytes().to_vec(),
                }
        })
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
    let thread = world.opened("m1", "note that the network is up", Some(WINDOW));
    let (request, head) = open_request(chain);
    assert_eq!(
        (head.method, path(&head)),
        (http::Method::Post, "/v1/messages")
    );
    head.check_path().unwrap();
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
        assigned(chain, SERVICE).len(),
        1,
        "one model call at a time"
    );
    assert!(
        matches!(&held(chain, thread)[..], [Body::Post { text, .. }] if text.ends_with("still there?"))
    );
    chain.at(3, 0);
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
        assigned(chain, SERVICE).len(),
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
    for service in egress::RESERVED {
        let Op::Hire {
            agent,
            system,
            model,
            max_tokens,
            ..
        } = hire(world.agent)
        else {
            panic!()
        };
        let reserved = Op::Hire {
            agent,
            system,
            model,
            service: service.into(),
            max_tokens,
        };
        let refused = chain.submit(signed(ADA), PROGRAM, &reserved).unwrap_err();
        assert_eq!(refused.code, code::INVALID_INPUT, "{service}");
    }

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
    let thread = world.opened("m1", "note it", Some(WINDOW));
    chain.at(3, 0);
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
        assigned(chain, SERVICE).is_empty(),
        "the model waits for the results"
    );

    let same_block = egress::Op::Answer {
        id: assigned(chain, RELAY)[0].id,
        answer: Answer::Output(Vec::new()),
    };
    let refused = chain.submit(signed(&NODE), egress::PROGRAM, &same_block);
    assert_eq!(refused.unwrap_err().code, code::WRONG_STATE);

    chain.at(4, 0);
    relay_back(chain, "toolu_l");
    let (listed, failed) = result_of(chain, thread, "toolu_l");
    assert!(!failed);
    assert_eq!(
        serde_json::from_str::<Value>(&listed).unwrap()["entries"],
        json!([])
    );
    assert!(assigned(chain, SERVICE).is_empty());
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

    let [next] = assigned(chain, SERVICE).try_into().unwrap();
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
    let thread = world.opened("m1", "look", Some(WINDOW));
    chain.at(3, 0);
    let call = tool_use("toolu_l", "files_list", json!({ "path": "/" }));
    model_says(chain, json!([call]), "tool_use");
    world.mention("m2", "and also");
    assert_eq!(held(chain, thread).len(), 1);
    chain.at(4, 0);
    relay_back(chain, "toolu_l");
    let [again] = assigned(chain, SERVICE).try_into().unwrap();
    let sent = body(chain, &again);
    let last = sent["messages"].as_array().unwrap().last().unwrap().clone();
    assert_eq!(last["role"], "user");
    assert_eq!(last["content"][0]["type"], "tool_result");
    let text = last["content"][1]["text"].as_str().unwrap();
    assert!(text.ends_with("and also"), "{text}");
    chain.at(5, 0);
    model_says(
        chain,
        json!([{ "type": "text", "text": "Done." }]),
        "end_turn",
    );
    assert!(assigned(chain, SERVICE).is_empty());
    let row = thread_row(chain, thread);
    assert_eq!((row.asking, row.due()), (Asking::Idle, false));
    assert_eq!(chain.frames(), 0);
}

#[test]
fn a_call_whose_relay_fails_fails_once_and_the_model_hears_it() {
    let world = world();
    let chain = &world.chain;
    let thread = world.opened("m1", "look", Some(WINDOW));
    chain.at(3, 0);
    let call = tool_use("toolu_l", "files_list", json!({ "path": "/" }));
    model_says(chain, json!([call]), "tool_use");
    chain.at(4, 0);
    let [row] = assigned(chain, RELAY).try_into().unwrap();
    answer(chain, row.id, Answer::Failed("no route".into()));
    assert!(assigned(chain, RELAY).is_empty(), "a relay is not repeated");
    let (text, failed) = result_of(chain, thread, "toolu_l");
    assert!(failed && text.contains("the relay failed"), "{text}");
    assert!(text.contains("no route"), "{text}");
    assert_eq!(assigned(chain, SERVICE).len(), 1);
}

#[test]
fn a_call_in_flight_when_its_agent_is_dismissed_does_not_run() {
    let world = world();
    let chain = &world.chain;
    let thread = world.opened("m1", "write", Some(WINDOW));
    chain.at(3, 0);
    let write = tool_use(
        "toolu_w",
        "files_write",
        json!({ "path": "/shared/late.md", "content": "x" }),
    );
    model_says(chain, json!([write]), "tool_use");
    chain
        .submit(signed(ADA), PROGRAM, &Op::Dismiss { agent: world.agent })
        .unwrap();
    chain.at(4, 0);
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
    assert!(assigned(chain, SERVICE).is_empty());
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
    let thread = world.opened("m1", "post", Some(WINDOW));
    chain.at(3, 0);
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
    chain.at(4, 0);
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
    chain.at(5, 0);
    let again = tool_use(
        "toolu_q",
        "chat_post",
        json!({ "channel": "general", "text": "still me?" }),
    );
    model_says(chain, json!([again]), "tool_use");
    chain.at(6, 0);
    relay_back(chain, "toolu_q");
    let (taken, failed) = result_of(chain, thread, "toolu_q");
    assert!(failed && taken.starts_with(code::UNAUTHORIZED), "{taken}");
}

fn notices(chain: &MockChain, thread: u64) -> Vec<String> {
    frames(chain, thread)
        .into_iter()
        .filter_map(|body| match body {
            Body::Notice { text } => Some(text),
            _ => None,
        })
        .collect()
}

#[test]
fn a_failed_model_call_is_noticed_once_and_the_next_input_asks_again() {
    let world = world();
    let chain = &world.chain;
    let thread = world.opened("m1", "hello", Some(WINDOW));
    chain.at(3, 0);
    let (request, _) = open_request(chain);
    let overloaded = json!({ "type": "error", "error": { "type": "overloaded_error", "message": "Overloaded" } });
    answer(chain, request.id, Answer::Output(response(529, overloaded)));
    assert!(
        assigned(chain, SERVICE).is_empty(),
        "a failed call is not repeated"
    );
    let told = notices(chain, thread);
    assert_eq!(told.len(), 1);
    assert!(
        told[0].contains("overloaded_error: Overloaded"),
        "{}",
        told[0]
    );
    world.mention("m2", "again");
    let (_, head) = open_request(chain);
    assert_eq!(path(&head), "/v1/messages");
}

#[test]
fn a_model_call_that_expires_ends_the_turn_with_the_expired_error() {
    let world = world();
    let chain = &world.chain;
    let thread = world.opened("m1", "hello", Some(WINDOW));
    let (request, head) = open_request(chain);
    assert_eq!(path(&head), "/v1/messages");
    chain.at(request.made + DEFAULT_WAIT, 0);
    assert!(notices(chain, thread).is_empty());
    chain.at(request.made + DEFAULT_WAIT + 1, 0);
    assert!(
        assigned(chain, SERVICE).is_empty(),
        "an expired call is not repeated"
    );
    let told = notices(chain, thread);
    let expired = format!("request {} expired after {DEFAULT_WAIT} blocks", request.id);
    assert_eq!(told.len(), 1);
    assert!(told[0].contains(&expired), "{}", told[0]);
    let late = egress::Op::Answer {
        id: request.id,
        answer: Answer::Output(reply(json!([]), "end_turn", 10)),
    };
    let refused = chain
        .submit(Origin::Signed(NODE.to_vec()), egress::PROGRAM, &late)
        .unwrap_err();
    assert_eq!(refused.code, code::NOT_FOUND);
    world.mention("m2", "again");
    let (_, head) = open_request(chain);
    assert_eq!(path(&head), "/v1/messages");
}

fn beta(head: &http::Head) -> Option<&str> {
    head.headers
        .iter()
        .find(|(name, _)| name == "anthropic-beta")
        .map(|(_, value)| value.as_str())
}

fn too_long() -> Vec<u8> {
    let error = json!({
        "type": "error",
        "error": { "type": "invalid_request_error", "message": "prompt is too long: 1000002 tokens > 1000000 maximum" },
    });
    abi::encode(&http::Response {
        status: 400,
        headers: Vec::new(),
        body: error.to_string().into_bytes(),
    })
}

fn refused_as_too_long(chain: &MockChain) {
    let (request, _) = open_request(chain);
    answer(chain, request.id, Answer::Output(too_long()));
}

fn sent_messages(chain: &MockChain) -> Vec<Value> {
    let (request, _) = open_request(chain);
    body(chain, &request)["messages"]
        .as_array()
        .unwrap()
        .clone()
}

fn summarized(chain: &MockChain, summary: &str) {
    let (request, head) = open_request(chain);
    assert_eq!(beta(&head), Some(anthropic::COMPACTION_BETA));
    let body = json!({
        "type": "message",
        "role": "assistant",
        "content": [{ "type": "compaction", "content": summary, "signature": "sig", "citations": null }],
        "stop_reason": "compaction",
        "usage": {
            "input_tokens": 0,
            "output_tokens": 0,
            "iterations": [{ "type": "compaction", "input_tokens": 1500, "output_tokens": 276 }],
        },
    });
    answer(chain, request.id, Answer::Output(response(200, body)));
}

#[test]
fn a_thread_whose_next_answer_would_not_fit_the_window_is_summarized_first() {
    let world = world();
    let chain = &world.chain;
    let thread = world.opened("m1", "remember the plan", Some(2_000));
    chain.at(3, 0);
    model_says_after(
        chain,
        json!([{ "type": "text", "text": "Noted." }]),
        "end_turn",
        1000,
    );
    assert_eq!(
        thread_row(chain, thread).context,
        Context::Measured(1008),
        "input, cache reads and output all count"
    );
    world.mention("m2", "and now?");
    let (request, head) = open_request(chain);
    assert_eq!(beta(&head), Some(anthropic::COMPACTION_BETA));
    let sent = body(chain, &request);
    assert_eq!(sent["compaction"], json!({ "type": "summarize" }));
    let messages = sent["messages"].as_array().unwrap();
    assert_eq!(messages.len(), 1, "the summary covers what the model saw");
    assert!(
        messages[0]["content"][0]["text"]
            .as_str()
            .unwrap()
            .ends_with("remember the plan")
    );
    assert_eq!(
        thread_row(chain, thread).asking,
        Asking::Compacting { through: 1 }
    );

    chain.at(4, 0);
    summarized(chain, "Ada set out the plan.");
    let row = thread_row(chain, thread);
    assert_eq!(row.context, Context::Measured(276));
    let summary = row.summary.expect("the thread keeps its summary");
    assert!(matches!(
        &frames(chain, thread)[summary as usize - 1],
        Body::Compaction { block } if block.contains("Ada set out the plan.")
    ));
    let (request, head) = open_request(chain);
    assert_eq!(path(&head), "/v1/messages");
    assert_eq!(beta(&head), Some(anthropic::COMPACTION_BETA));
    let sent = body(chain, &request);
    assert!(sent.get("compaction").is_none());
    let messages = sent["messages"].as_array().unwrap();
    assert_eq!(
        messages[0],
        json!({
            "role": "assistant",
            "content": [{ "type": "compaction", "content": "Ada set out the plan.", "signature": "sig" }],
        }),
        "the summary stands in for what it covers, with no null fields"
    );
    assert_eq!(messages[1]["role"], "assistant");
    assert_eq!(messages[1]["content"][0]["text"], "Noted.");
    assert_eq!(messages[2]["role"], "user");
    assert!(
        messages[2]["content"][0]["text"]
            .as_str()
            .unwrap()
            .ends_with("and now?")
    );
    assert_eq!(messages.len(), 3);

    chain.at(5, 0);
    model_says(
        chain,
        json!([{ "type": "text", "text": "Still on it." }]),
        "end_turn",
    );
    world.mention("m3", "once more");
    let (request, _) = open_request(chain);
    let sent = body(chain, &request);
    let messages = sent["messages"].as_array().unwrap();
    assert_eq!(
        messages[0]["content"][0]["type"], "compaction",
        "every later request carries the summary"
    );
    assert_eq!(messages.len(), 5);
}

#[test]
fn a_request_the_model_finds_too_long_is_summarized_even_with_no_window_known() {
    let world = world();
    let chain = &world.chain;
    let thread = world.opened("m1", "first", None);
    chain.at(3, 0);
    model_says_after(
        chain,
        json!([{ "type": "text", "text": "Ok." }]),
        "end_turn",
        1_000_000,
    );
    world.mention("m2", "second");
    let (request, head) = open_request(chain);
    assert_eq!(
        (path(&head), beta(&head)),
        ("/v1/messages", None),
        "no window, no early summary"
    );
    chain.at(4, 0);
    answer(chain, request.id, Answer::Output(too_long()));
    assert_eq!(thread_row(chain, thread).context, Context::Overflowed);
    let (request, _) = open_request(chain);
    assert_eq!(
        body(chain, &request)["compaction"],
        json!({ "type": "summarize" })
    );
    chain.at(5, 0);
    summarized(chain, "First, then Ok.");
    chain.at(6, 0);
    let (request, _) = open_request(chain);
    let cut_short = json!({
        "type": "message",
        "role": "assistant",
        "model": "claude-test",
        "content": [{ "type": "text", "text": "I was say" }],
        "stop_reason": "model_context_window_exceeded",
        "usage": { "input_tokens": 999_000, "output_tokens": 1000 },
    });
    answer(chain, request.id, Answer::Output(response(200, cut_short)));
    assert!(
        !frames(chain, thread)
            .iter()
            .any(|body| matches!(body, Body::Turn { said, .. } if said.contains(&Said::Text("I was say".into())))),
        "a turn cut short by the window is not kept"
    );
    let messages = sent_messages(chain);
    let last = &messages[messages.len() - 1];
    assert!(
        last["content"][0]["text"]
            .as_str()
            .unwrap()
            .starts_with("left out of your context: frame 3 is"),
        "with nothing new to summarize, the newest input is left out: {last}"
    );
    chain.at(7, 0);
    refused_as_too_long(chain);
    assert!(
        assigned(chain, SERVICE).is_empty(),
        "with everything new left out and still too long, the thread gives up"
    );
    let told = notices(chain, thread);
    assert!(
        told.last().unwrap().contains("everything new left out"),
        "{told:?}"
    );
    let row = thread_row(chain, thread);
    assert_eq!(row.asking, Asking::Idle);
    assert_eq!(
        row.context,
        Context::Measured(0),
        "the next input is asked plainly"
    );
    world.mention("m3", "third");
    let (_, head) = open_request(chain);
    assert_eq!(path(&head), "/v1/messages");
    let messages = sent_messages(chain);
    let last = &messages[messages.len() - 1];
    assert!(
        last["content"][1]["text"]
            .as_str()
            .unwrap()
            .ends_with("third"),
        "{last}"
    );
}

#[test]
fn a_result_that_alone_overflows_the_window_is_left_out_and_the_model_searches_and_reads_it_in_parts()
 {
    let world = world();
    let chain = &world.chain;
    let thread = world.opened("m1", "read it", Some(WINDOW));
    chain.at(3, 0);
    let call = tool_use("toolu_l", "files_list", json!({ "path": "/" }));
    model_says(chain, json!([call]), "tool_use");
    chain.at(4, 0);
    relay_back(chain, "toolu_l");
    chain.at(5, 0);
    refused_as_too_long(chain);
    chain.at(6, 0);
    summarized(chain, "Ada asked to read it.");
    chain.at(7, 0);
    refused_as_too_long(chain);
    let messages = sent_messages(chain);
    let result = &messages[messages.len() - 1]["content"][0];
    assert_eq!(result["type"], "tool_result");
    assert_eq!(result["tool_use_id"], "toolu_l");
    assert_eq!(result["is_error"], false);
    let (text, failed) = result_of(chain, thread, "toolu_l");
    assert!(!failed, "the transcript keeps the result as it came");
    let note = format!(
        "left out of your context: frame 3 is {} bytes in 1 lines, too large for your context window alone. Find lines in it with thread_search (frame 3) and read parts of it with thread_read (frame 3, an offset and a length).",
        text.len()
    );
    assert_eq!(result["content"], note);
    let told = notices(chain, thread);
    assert!(
        told.last()
            .unwrap()
            .starts_with("the result of files_list call toolu_l alone does not fit"),
        "{told:?}"
    );

    chain.at(8, 0);
    let wanted = &text[2..6];
    let calls = json!([
        tool_use(
            "toolu_s",
            "thread_search",
            json!({ "pattern": wanted, "frame": 3, "around": 1 })
        ),
        tool_use(
            "toolu_r",
            "thread_read",
            json!({ "frame": 3, "offset": 2, "length": 4 })
        ),
        tool_use("toolu_n", "thread_read", json!({ "frame": 99 })),
    ]);
    model_says(chain, calls, "tool_use");
    chain.at(9, 0);
    for call in ["toolu_s", "toolu_r", "toolu_n"] {
        relay_back(chain, call);
    }
    let (searched, failed) = result_of(chain, thread, "toolu_s");
    assert!(!failed, "{searched}");
    let searched: Value = serde_json::from_str(&searched).unwrap();
    assert_eq!(searched["hits"][0]["frame"], 3);
    assert_eq!(searched["hits"][0]["line"], 1);
    assert_eq!(searched["hits"][0]["offsets"][0], 2);
    assert_eq!(searched["hits"][0]["excerpts"][0], &text[1..7]);
    let (read, failed) = result_of(chain, thread, "toolu_r");
    assert!(!failed, "{read}");
    let read: Value = serde_json::from_str(&read).unwrap();
    assert_eq!(
        read,
        json!({ "frame": 3, "offset": 2, "size": text.len(), "text": wanted, "rest": text.len() - 6 })
    );
    let (missing, failed) = result_of(chain, thread, "toolu_n");
    assert!(failed && missing.contains("no frame 99"), "{missing}");
    chain.at(10, 0);

    model_says(
        chain,
        json!([{ "type": "text", "text": "Found it." }]),
        "end_turn",
    );
    let row = thread_row(chain, thread);
    assert_eq!(row.context, Context::Measured(18));
    assert_eq!(row.asking, Asking::Idle);
}

#[test]
fn a_thread_search_finds_what_a_summary_covers_and_reads_whole_lines_on_a_char_boundary() {
    let world = world();
    let chain = &world.chain;
    let thread = world.opened("m1", "the key is é-42", Some(2_000));
    chain.at(3, 0);
    model_says_after(
        chain,
        json!([{ "type": "text", "text": "Noted." }]),
        "end_turn",
        1000,
    );
    world.mention("m2", "what was the key?");
    chain.at(4, 0);
    summarized(chain, "Ada gave a key.");
    chain.at(5, 0);
    let calls = json!([
        tool_use("toolu_s", "thread_search", json!({ "pattern": "key is" })),
        tool_use(
            "toolu_r",
            "thread_read",
            json!({ "frame": 1, "offset": 0, "length": 1 })
        ),
    ]);
    model_says(chain, calls, "tool_use");
    chain.at(6, 0);
    relay_back(chain, "toolu_s");
    relay_back(chain, "toolu_r");
    let (searched, _) = result_of(chain, thread, "toolu_s");
    let searched: Value = serde_json::from_str(&searched).unwrap();
    let hits = searched["hits"].as_array().unwrap();
    assert_eq!(hits.len(), 2, "{searched}");
    assert_eq!(
        hits[1]["text"], "thread_search {\"pattern\":\"key is\"}",
        "the model's own turns are searched too"
    );
    assert_eq!(hits[0]["frame"], 1);
    assert!(
        hits[0]["text"]
            .as_str()
            .unwrap()
            .ends_with("the key is é-42"),
        "{searched}"
    );
    let (read, _) = result_of(chain, thread, "toolu_r");
    let read: Value = serde_json::from_str(&read).unwrap();
    assert_eq!(read["offset"], 0);
    assert_eq!(read["rest"], read["size"].as_u64().unwrap() - 1);
    let first = frames(chain, thread)[0].text();
    let middle = first.find('é').unwrap() + 1;
    let (got, _) = {
        chain.at(7, 0);
        let call = tool_use(
            "toolu_m",
            "thread_read",
            json!({ "frame": 1, "offset": middle, "length": 1 }),
        );
        model_says(chain, json!([call]), "tool_use");
        chain.at(8, 0);
        relay_back(chain, "toolu_m");
        result_of(chain, thread, "toolu_m")
    };
    let got: Value = serde_json::from_str(&got).unwrap();
    assert_eq!(got["offset"], middle - 1, "a read starts on a character");
    assert_eq!(got["text"], "é");
}

#[test]
fn a_summary_refused_as_too_long_covers_less_and_never_ends_on_the_models_turn() {
    let world = world();
    let chain = &world.chain;
    let thread = world.opened("m1", "one", Some(WINDOW));
    chain.at(3, 0);
    model_says(
        chain,
        json!([{ "type": "text", "text": "Ok one." }]),
        "end_turn",
    );
    world.mention("m2", "two");
    chain.at(4, 0);
    model_says(
        chain,
        json!([{ "type": "text", "text": "Ok two." }]),
        "end_turn",
    );
    world.mention("m3", "three");
    chain.at(5, 0);
    refused_as_too_long(chain);
    assert_eq!(
        sent_messages(chain).len(),
        3,
        "the summary covers what the model answered"
    );
    chain.at(6, 0);
    refused_as_too_long(chain);
    assert_eq!(thread_row(chain, thread).compact_before, Some(3));
    let messages = sent_messages(chain);
    assert_eq!(
        messages.len(),
        1,
        "one message fewer would end on the model's turn, so the summary covers the first input alone"
    );
    assert_eq!(messages[0]["role"], "user");
    chain.at(7, 0);
    summarized(chain, "Ada said one.");
    assert_eq!(thread_row(chain, thread).compact_before, None);
    let messages = sent_messages(chain);
    let roles: Vec<&str> = messages
        .iter()
        .map(|message| message["role"].as_str().unwrap())
        .collect();
    assert_eq!(
        roles,
        ["assistant", "assistant", "user", "assistant", "user"]
    );
    assert_eq!(messages[0]["content"][0]["type"], "compaction");
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
fn with_no_node_serving_the_model_a_mention_says_so() {
    let world = world();
    let chain = &world.chain;
    let stop = egress::Op::Serve {
        services: Vec::new(),
    };
    chain.submit(signed(&NODE), egress::PROGRAM, &stop).unwrap();
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
