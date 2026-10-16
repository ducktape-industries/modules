//! harness on ducktape's host with the egress role bound: founding registers harness with
//! attribution, an agent is a program account whose script calls harness and names its
//! configuration file in files, a founder node serves the model's service and the relay, and a
//! mention in chat, the model's turn and the agent's write each land in their own block while
//! the thread's loop waits parked.
use super::*;

use egress::{Answer, RELAY, Request, RequestRow, http};
use harness::{Body, Op, Query, Reply, ThreadRow};
use serde_json::{Value, json};

const NODE: u64 = 1;
const ADA: u64 = 11;
const SERVICE: &str = "ccs";
const CONFIG: &str = "/home/ada/agents/scout.md";

fn apps() -> Vec<Founding> {
    ["egress", "attribution", "harness", "files", "chat"]
        .into_iter()
        .map(|name| founding(name, &program(name)))
        .collect()
}

impl Net {
    async fn assigned(&self, service: &str) -> Vec<RequestRow> {
        let asked = egress::Query::Assigned { key: public(NODE) };
        let egress::Reply::Requests(rows) = self.ask(egress::PROGRAM, &asked).await else {
            panic!()
        };
        rows.into_iter()
            .filter(|row| row.request.service() == service)
            .collect()
    }

    async fn joined(&self, row: &RequestRow) -> Value {
        let mut body = Vec::new();
        for part in row.request.parts() {
            let framed = self.blob(part).await.expect("a part is on the network");
            let start = framed.iter().position(|byte| *byte == 0).unwrap() + 1;
            body.extend(&framed[start..]);
        }
        serde_json::from_slice(&body).expect("the parts join into one JSON request")
    }

    async fn answer(&mut self, id: u64, output: Vec<u8>) {
        let op = egress::Op::Answer {
            id,
            answer: Answer::Output(output),
        };
        self.apply(&public(NODE), egress::PROGRAM, &op).await;
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
fn a_mentioned_agent_answers_through_elected_nodes_and_writes_as_itself() {
    deterministic::Runner::default().start(|context| async move {
        let dir = tempfile::tempdir().unwrap();
        let mut net = Net::found_with(context, dir.path(), apps(), Vec::new(), MEMBER_CAP).await;
        let attribution::Reply::Registered(registered) = net
            .ask(attribution::PROGRAM, &attribution::Query::Registered)
            .await
        else {
            panic!()
        };
        assert_eq!(
            registered,
            [harness::PROGRAM.to_owned()],
            "founding ran harness's init, which registered it with attribution"
        );
        let serve = egress::Op::Serve {
            services: vec![SERVICE.into(), RELAY.into()],
        };
        net.apply(&public(NODE), egress::PROGRAM, &serve).await;
        let create = identity::Op::Create {
            name: "Ada".into(),
            scheme: Scheme::Ed25519,
        };
        net.apply(&public(ADA), identity::PROGRAM, &create).await;
        let register = egress::Op::Register {
            service: SERVICE.into(),
        };
        net.apply(&public(ADA), egress::PROGRAM, &register).await;
        let assign = egress::Op::Assign {
            service: SERVICE.into(),
            nodes: vec![public(NODE)],
        };
        net.apply(&public(ADA), egress::PROGRAM, &assign).await;
        let create = identity::Op::CreateAgent {
            name: "Scout".into(),
        };
        let agent: u64 =
            abi::decode(&net.apply(&public(ADA), identity::PROGRAM, &create).await).unwrap();
        let config = harness::Config {
            model: "claude-test".into(),
            max_tokens: 512,
            wait: None,
            system: "Keep notes in /shared.".into(),
        };
        let commit = files::Op::Commit {
            base: None,
            message: "configure Scout".into(),
            changes: vec![files::Change::Put {
                path: CONFIG.into(),
                exec: false,
                meta: Default::default(),
                content: files::Content::Inline(config.write().into_bytes()),
            }],
        };
        let snapshot: BlobId =
            abi::decode(&net.apply(&public(ADA), files::PROGRAM, &commit).await).unwrap();
        let script = identity::Op::SetScript {
            account: agent,
            script: Some(identity::Script {
                calls: harness::PROGRAM.into(),
                config: CONFIG.into(),
            }),
        };
        net.apply(&public(ADA), identity::PROGRAM, &script).await;
        let model = Op::SetModel {
            id: "claude-test".into(),
            wire: harness::Wire::AnthropicMessages,
            service: SERVICE.into(),
            name: "claude-test".into(),
        };
        net.apply(&public(ADA), harness::PROGRAM, &model).await;
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
        let [models] = net.assigned(SERVICE).await.try_into().unwrap();
        let Request::Http { head, .. } = &models.request else {
            panic!("the model is asked over http");
        };
        assert_eq!(head.target, "/v1/models/claude-test");
        let info = json!({ "type": "model", "id": "claude-test", "max_input_tokens": 200_000 });
        let window = abi::encode(&http::Response {
            status: 200,
            headers: Vec::new(),
            body: info.to_string().into_bytes(),
        });
        net.answer(models.id, window).await;
        let [asked] = net.assigned(SERVICE).await.try_into().unwrap();
        let sent = net.joined(&asked).await;
        assert_eq!(sent["max_tokens"], 512);
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
        net.answer(asked.id, model_reply(write, "tool_use")).await;
        let frames = net.frames(thread.id).await;
        assert_eq!(frames.len(), 2);
        assert!(
            matches!(&frames[1], Body::Turn { config, .. } if *config == snapshot),
            "the turn names the configuration snapshot its call read"
        );
        let [relay] = net.assigned(RELAY).await.try_into().unwrap();
        assert!(net.assigned(SERVICE).await.is_empty());
        assert_eq!(net.parked(), 1, "the thread's loop waits parked");

        net.answer(relay.id, Vec::new()).await;
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

        let [next] = net.assigned(SERVICE).await.try_into().unwrap();
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
        net.answer(
            next.id,
            model_reply(json!([{ "type": "text", "text": "Noted." }]), "end_turn"),
        )
        .await;
        assert!(net.assigned(SERVICE).await.is_empty());
        assert_eq!(net.parked(), 0, "the loop finished");
    });
}
