//! attribution founded beside chat on ducktape's host: a mention and a direct message are
//! recorded in the frame that posts them, and a post stands where nothing is recorded.
use super::*;

use attribution::{Kind, Object, Query, Reason, Reply, Source};

const ADA: u64 = 11;
const BO: u64 = 12;

fn apps() -> Vec<Founding> {
    ["chat", "attribution"]
        .into_iter()
        .map(|name| founding(name, &program(name)))
        .collect()
}

fn message(id: &str) -> Source {
    Source {
        program: chat::MODULE.into(),
        object: Object {
            kind: chat::attributed::MESSAGE.into(),
            id: id.into(),
        },
    }
}

fn post(channel: &str, id: &str, text: &str) -> chat::Op {
    chat::Op::PostMessage {
        channel_id: channel.into(),
        message_id: id.into(),
        blocks: chat::parse_message(text),
        thread: None,
    }
}

#[test]
fn chat_reports_mentions_and_direct_messages_to_attribution_as_they_are_posted() {
    deterministic::Runner::default().start(|context| async move {
        let dir = tempfile::tempdir().unwrap();
        let mut net = Net::found_with(context, dir.path(), apps(), Vec::new(), MEMBER_CAP).await;
        let mut accounts = Vec::new();
        for (seed, name) in [(ADA, "Ada"), (BO, "Bo")] {
            let create = identity::Op::Create {
                name: name.into(),
                scheme: Scheme::Ed25519,
            };
            let output = net.apply(&public(seed), identity::MODULE, &create).await;
            accounts.push(abi::decode::<u64>(&output).unwrap());
        }
        let [ada, bo] = accounts[..] else { panic!() };
        let create = chat::Op::CreateChannel {
            channel_id: "general".into(),
            name: "general".into(),
            post_policy: chat::PostPolicy::Open,
        };
        net.apply(&public(ADA), chat::MODULE, &create).await;
        net.apply(
            &public(ADA),
            chat::MODULE,
            &post("general", "m1", &format!("ping <@{bo}>")),
        )
        .await;
        net.apply(&public(ADA), chat::MODULE, &post("general", "m2", "nobody"))
            .await;
        let dm = chat::Op::CreateDmChannel {
            counterpart: bo,
            name: "us".into(),
        };
        net.apply(&public(ADA), chat::MODULE, &dm).await;
        net.apply(
            &public(BO),
            chat::MODULE,
            &post(&chat::dm_channel_id(ada, bo), "d1", "hi"),
        )
        .await;

        let held = |source| {
            let net = &net;
            async move {
                let Reply::Relations(relations) = net
                    .ask(attribution::MODULE, &Query::Relations { source })
                    .await
                else {
                    panic!()
                };
                relations.map(|row| {
                    row.relations
                        .into_iter()
                        .map(|relation| (relation.recipient, relation.reason))
                        .collect::<Vec<_>>()
                })
            }
        };
        assert_eq!(held(message("m1")).await, Some(vec![(bo, Reason::Mention)]));
        assert_eq!(held(message("m2")).await, None);
        let direct = Reason::Defined(chat::attributed::DIRECT.into());
        assert_eq!(held(message("d1")).await, Some(vec![(ada, direct)]));
        let asked = Query::ChangesTo {
            recipient: bo,
            page: PageRequest::default(),
        };
        let Reply::Changes(page) = net.ask(attribution::MODULE, &asked).await else {
            panic!()
        };
        let kinds: Vec<Kind> = page.items.into_iter().map(|change| change.kind).collect();
        assert_eq!(kinds, [Kind::Added]);
    });
}
