//! push on ducktape's host with chat and the egress role: a mention posted in chat reaches the
//! founder node serving `push` as one JSON post naming the mentioned account's key, and the
//! service's answer other than 2xx is the push frame's refusal; on a network founded without
//! push, the same post stands.
use super::*;

use egress::{Answer, Request, RequestRow, http};
use serde_json::{Value, json};

const NODE: u64 = 1;
const ADA: u64 = 11;
const BO: u64 = 12;

fn apps(names: &[&str]) -> Vec<Founding> {
    names
        .iter()
        .map(|name| founding(name, &program(name)))
        .collect()
}

impl Net {
    async fn people(&mut self) -> (AccountNumber, AccountNumber) {
        let mut numbers = Vec::new();
        for (seed, name) in [(ADA, "Ada"), (BO, "Bo")] {
            let create = identity::Op::Create {
                name: name.into(),
                scheme: Scheme::Ed25519,
            };
            let output = self.apply(&public(seed), identity::PROGRAM, &create).await;
            numbers.push(abi::decode(&output).unwrap());
        }
        let general = chat::Op::CreateChannel {
            channel_id: "general".into(),
            name: "general".into(),
            post_policy: chat::PostPolicy::Open,
        };
        self.apply(&public(ADA), chat::PROGRAM, &general).await;
        (numbers[0], numbers[1])
    }

    async fn mention(&mut self, bo: AccountNumber) -> Receipt {
        let post = chat::Op::PostMessage {
            channel_id: "general".into(),
            message_id: "m1".into(),
            blocks: chat::parse_message(&format!("hi <@{bo}>")),
            thread: None,
        };
        self.submit(&public(ADA), chat::PROGRAM, &post).await
    }

    async fn pushes(&self) -> Vec<RequestRow> {
        let asked = egress::Query::Assigned { key: public(NODE) };
        let egress::Reply::Requests(rows) = self.ask(egress::PROGRAM, &asked).await else {
            panic!()
        };
        rows.into_iter()
            .filter(|row| row.request.service() == push::SERVICE)
            .collect()
    }

    async fn sent(&self, row: &RequestRow) -> Value {
        let [part] = row.request.parts() else {
            panic!("one part")
        };
        let framed = self.blob(part).await.expect("the body is on the network");
        let start = framed.iter().position(|byte| *byte == 0).unwrap() + 1;
        serde_json::from_slice(&framed[start..]).expect("the body is JSON")
    }
}

/// The receipt of `program`'s run anywhere under `receipt`.
fn run_of<'a>(receipt: &'a Receipt, program: &str) -> Option<&'a Receipt> {
    if receipt.program == program {
        return Some(receipt);
    }
    receipt
        .nested
        .iter()
        .find_map(|nested| run_of(nested, program))
}

#[test]
fn a_mention_reaches_the_node_serving_push_and_a_refused_post_is_the_push_frames_refusal() {
    deterministic::Runner::default().start(|context| async move {
        let dir = tempfile::tempdir().unwrap();
        let apps = apps(&["egress", "chat", "push"]);
        let mut net = Net::found_with(context, dir.path(), apps, Vec::new(), MEMBER_CAP).await;
        let serve = egress::Op::Serve {
            services: vec![push::SERVICE.into()],
        };
        net.apply(&public(NODE), egress::PROGRAM, &serve).await;
        let (_, bo) = net.people().await;
        let register = egress::Op::Register {
            service: push::SERVICE.into(),
        };
        net.apply(&public(ADA), egress::PROGRAM, &register).await;
        let assign = egress::Op::Assign {
            service: push::SERVICE.into(),
            nodes: vec![public(NODE)],
        };
        net.apply(&public(ADA), egress::PROGRAM, &assign).await;

        output_of(&net.mention(bo).await);
        let [row] = net.pushes().await.try_into().expect("one push request");
        assert_eq!(row.requester, push::PROGRAM);
        let Request::Http { head, .. } = &row.request else {
            panic!("push posts over http")
        };
        assert_eq!(
            (head.method, head.target.as_str()),
            (http::Method::Post, push::TARGET)
        );
        assert_eq!(
            net.sent(&row).await,
            json!({
                "program": "chat",
                "notices": [{
                    "keys": [abi::hex(&public(BO))],
                    "title": "Ada mentioned you",
                    "body": "hi @Bo",
                    "tag": "#general",
                    "link": ["general", "1"],
                }],
            })
        );
        assert_eq!(net.parked(), 1, "push waits parked for the node");

        let refused = http::Response {
            status: 401,
            headers: Vec::new(),
            body: b"unknown network token".to_vec(),
        };
        let answer = egress::Op::Answer {
            id: row.id,
            answer: Answer::Output(abi::encode(&refused)),
        };
        let receipt = net.submit(&public(NODE), egress::PROGRAM, &answer).await;
        let pushed = run_of(&receipt, push::PROGRAM).expect("the answer resumes push");
        let Outcome::Rejected(refusal) = &pushed.outcome else {
            panic!("push applied: {:?}", pushed.outcome)
        };
        assert_eq!(
            refusal.sentence,
            "the push service answered 401: unknown network token"
        );
        assert_eq!(net.parked(), 0);
    });
}

#[test]
fn a_mention_stands_on_a_network_founded_without_push() {
    deterministic::Runner::default().start(|context| async move {
        let dir = tempfile::tempdir().unwrap();
        let apps = apps(&["chat"]);
        let mut net = Net::found_with(context, dir.path(), apps, Vec::new(), MEMBER_CAP).await;
        let (_, bo) = net.people().await;
        let receipt = net.mention(bo).await;
        output_of(&receipt);
        let pushed = run_of(&receipt, push::PROGRAM).expect("chat sent to push");
        assert!(matches!(pushed.outcome, Outcome::Rejected(_)));
        let asked = chat::Query::MessageById {
            message_id: "m1".into(),
        };
        let chat::Reply::Message(Some(row)) = net.ask(chat::PROGRAM, &asked).await else {
            panic!("the post landed")
        };
        assert_eq!(row.seq, 1);
    });
}
