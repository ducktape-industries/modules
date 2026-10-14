//! harness on ducktape's host: a founder node serves the model and the relay through
//! provider, a manager hires the agent it handed to harness, and a mention in chat, the
//! model's context window, the model's turn and the agent's write each land in their own
//! block while the thread's loop waits parked.
use super::*;

use harness::{Body, MODEL_SERVICE, Op, Query, Reply, ThreadRow};
use provider::{Answer, RELAY, RequestRow, http};
use serde_json::{Value, json};

const NODE: u64 = 1;
const ADA: u64 = 11;

fn apps() -> Vec<Founding> {
    ["provider", "attribution", "harness", "files", "chat"]
        .into_iter()
        .map(|name| founding(name, &program(name)))
        .collect()
}

impl Net {
    async fn assigned(&self, service: &str) -> Vec<RequestRow> {
        let asked = provider::Query::Assigned {
            key: public(NODE),
            page: PageRequest::default(),
        };
        let provider::Reply::Requests(page) = self.ask(provider::PROGRAM, &asked).await else {
            panic!()
        };
        page.items
            .into_iter()
            .filter(|row| row.service == service)
            .collect()
    }

    async fn joined(&self, row: &RequestRow) -> Value {
        let mut body = Vec::new();
        for part in &row.parts {
            let framed = self.blob(part).await.expect("a part is on the network");
            let start = framed.iter().position(|byte| *byte == 0).unwrap() + 1;
            body.extend(&framed[start..]);
        }
        serde_json::from_slice(&body).expect("the parts join into one JSON request")
    }

    async fn provide(&mut self, id: u64, output: Vec<u8>) {
        let op = provider::Op::Answer {
            id,
            answer: Answer::Output(output),
        };
        self.apply(&public(NODE), provider::PROGRAM, &op).await;
    }

    async fn threads(&self, agent: u64) -> Vec<ThreadRow> {
        let asked = Query::Threads {
            agent,
            page: PageRequest::default(),
        };
        let Reply::Threads(page) = self.ask(harness::PROGRAM, &asked).await else {
            panic!()
        };
        page.items
    }

    async fn frames(&self, thread: u64) -> Vec<Body> {
        let asked = Query::Frames {
            thread,
            page: PageRequest::default(),
        };
        let Reply::Frames(page) = self.ask(harness::PROGRAM, &asked).await else {
            panic!()
        };
        page.items.into_iter().map(|frame| frame.body).collect()
    }
}

fn model_reply(content: Value, stop: &str) -> Vec<u8> {
    let body = json!({
        "model": "claude-test",
        "content": content,
        "stop_reason": stop,
        "usage": { "input_tokens": 1, "output_tokens": 1 },
    });
    abi::encode(&http::Response {
        status: 200,
        headers: Vec::new(),
        body: body.to_string().into_bytes(),
    })
}

#[test]
fn a_mentioned_agent_answers_through_elected_providers_and_writes_as_itself() {
    deterministic::Runner::default().start(|context| async move {
        let dir = tempfile::tempdir().unwrap();
        let mut net = Net::found_with(context, dir.path(), apps(), Vec::new(), MEMBER_CAP).await;
        let serve = provider::Op::Serve {
            services: vec![MODEL_SERVICE.into(), RELAY.into()],
        };
        net.apply(&public(NODE), provider::PROGRAM, &serve).await;
        let create = identity::Op::Create {
            name: "Ada".into(),
            scheme: Scheme::Ed25519,
        };
        net.apply(&public(ADA), identity::PROGRAM, &create).await;
        let create = identity::Op::CreateAgent {
            name: "Scout".into(),
        };
        let agent: u64 =
            abi::decode(&net.apply(&public(ADA), identity::PROGRAM, &create).await).unwrap();
        let hand = identity::Op::SetRunner {
            account: agent,
            runner: Some(harness::PROGRAM.into()),
        };
        net.apply(&public(ADA), identity::PROGRAM, &hand).await;
        let hire = Op::Hire {
            agent,
            system: "Keep notes in /shared.".into(),
            model: "claude-test".into(),
            max_tokens: 512,
        };
        net.apply(&public(ADA), harness::PROGRAM, &hire).await;
        let general = chat::Op::CreateChannel {
            channel_id: "general".into(),
            name: "general".into(),
            post_policy: chat::PostPolicy::Open,
        };
        net.apply(&public(ADA), chat::PROGRAM, &general).await;
        let mention = chat::Op::PostMessage {
            channel_id: "general".into(),
            message_id: "m1".into(),
            blocks: chat::parse_message(&format!("<@{agent}> note that the network is up")),
            thread: None,
        };
        net.apply(&public(ADA), chat::PROGRAM, &mention).await;

        let [thread] = net.threads(agent).await.try_into().unwrap();
        let [models] = net.assigned(MODEL_SERVICE).await.try_into().unwrap();
        let head: http::Head = abi::decode(&models.input).unwrap();
        assert_eq!(head.path, "/v1/models/claude-test");
        let info = json!({ "type": "model", "id": "claude-test", "max_input_tokens": 200_000 });
        let window = abi::encode(&http::Response {
            status: 200,
            headers: Vec::new(),
            body: info.to_string().into_bytes(),
        });
        net.provide(models.id, window).await;
        let [asked] = net.assigned(MODEL_SERVICE).await.try_into().unwrap();
        let sent = net.joined(&asked).await;
        let text = sent["messages"][0]["content"][0]["text"].as_str().unwrap();
        assert!(
            text.contains("mentioned you in chat channel general"),
            "{text}"
        );
        assert!(text.ends_with("note that the network is up"), "{text}");
        assert!(
            sent["system"][0]["text"]
                .as_str()
                .unwrap()
                .starts_with("You are Scout")
        );

        let write = json!([{
            "type": "tool_use",
            "id": "toolu_1",
            "name": "files_write",
            "input": { "path": "/shared/status.md", "content": "up" },
        }]);
        net.provide(asked.id, model_reply(write, "tool_use")).await;
        assert_eq!(net.frames(thread.id).await.len(), 2);
        let [relay] = net.assigned(RELAY).await.try_into().unwrap();
        assert!(net.assigned(MODEL_SERVICE).await.is_empty());
        assert_eq!(net.parked(), 1, "the thread's loop waits parked");

        net.provide(relay.id, Vec::new()).await;
        let asked = files::Query::Read {
            path: "/shared/status.md".into(),
            at: None,
            offset: 0,
            len: 16,
        };
        let files::Reply::Contents(contents) = net.ask(files::PROGRAM, &asked).await else {
            panic!()
        };
        assert_eq!(contents.bytes, b"up");
        let files::Reply::Head(Some(head)) = net.ask(files::PROGRAM, &files::Query::Head).await
        else {
            panic!()
        };
        assert_eq!(
            head.author,
            program::Principal::Account(agent),
            "the agent is the writer"
        );

        let [next] = net.assigned(MODEL_SERVICE).await.try_into().unwrap();
        let messages = net.joined(&next).await["messages"]
            .as_array()
            .unwrap()
            .clone();
        assert_eq!(messages.len(), 3);
        let result = &messages[2]["content"][0];
        assert_eq!(
            (&result["type"], &result["tool_use_id"], &result["is_error"]),
            (&json!("tool_result"), &json!("toolu_1"), &json!(false))
        );
        net.provide(
            next.id,
            model_reply(json!([{ "type": "text", "text": "Noted." }]), "end_turn"),
        )
        .await;
        assert!(net.assigned(MODEL_SERVICE).await.is_empty());
        assert_eq!(net.parked(), 0, "the loop finished");
    });
}
