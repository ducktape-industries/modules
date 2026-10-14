//! What chat tells attribution, with attribution seated beside it on a `MockChain`: each
//! message's mentions, and a direct message's counterpart.
use ::program::{MockChain, Origin};
use abi::role::identity::{Kind, Profile};
use attribution::{
    Actor, Attribution, Cause, Change, Kind as Changed, Object, PageRequest, Reason, Relations,
    Source,
};

use crate::attributed::{DIRECT, MESSAGE};

const ADA: &[u8] = b"ada-key";
const BO: &[u8] = b"bo-key";

fn chain() -> MockChain {
    let mut chain = MockChain::default();
    chain.seat::<Attribution>(attribution::PROGRAM);
    chain.seat::<crate::Chat>(crate::PROGRAM);
    chain.register(attribution::PROGRAM, 900);
    chain.register(crate::PROGRAM, 901);
    for (key, number, name) in [(ADA, 1, "Ada"), (BO, 2, "Bo")] {
        chain.hold(key.to_vec(), number);
        chain.profile(Profile {
            number,
            name: name.into(),
            kind: Kind::Person,
        });
    }
    chain
}

fn signed(key: &[u8]) -> Origin {
    Origin::Signed(key.to_vec())
}

fn chat_source(message: &str) -> Source {
    Source {
        program: crate::PROGRAM.into(),
        object: Object {
            kind: MESSAGE.into(),
            id: message.into(),
        },
    }
}

fn relations(chain: &MockChain, source: Source) -> Option<Relations> {
    let asked = attribution::Query::Relations { source };
    let attribution::Reply::Relations(relations) =
        chain.query(attribution::PROGRAM, &asked).unwrap()
    else {
        panic!()
    };
    relations
}

fn held(chain: &MockChain, source: Source) -> Vec<(u64, Reason)> {
    relations(chain, source)
        .map(|row| {
            row.relations
                .into_iter()
                .map(|relation| (relation.recipient, relation.reason))
                .collect()
        })
        .unwrap_or_default()
}

fn changes(chain: &MockChain, asked: attribution::Query) -> Vec<Change> {
    let attribution::Reply::Changes(page) = chain.query(attribution::PROGRAM, &asked).unwrap()
    else {
        panic!()
    };
    page.items
}

fn changes_to(chain: &MockChain, recipient: u64) -> Vec<(String, Reason, Changed)> {
    let asked = attribution::Query::ChangesTo {
        recipient,
        page: PageRequest::default(),
    };
    changes(chain, asked)
        .into_iter()
        .map(|change| (change.source.object.id, change.reason, change.kind))
        .collect()
}

fn post(chain: &MockChain, key: &[u8], channel: &str, id: &str, text: &str) {
    let op = crate::Op::PostMessage {
        channel_id: channel.into(),
        message_id: id.into(),
        blocks: crate::parse_message(text),
        thread: None,
    };
    chain.submit(signed(key), crate::PROGRAM, &op).unwrap();
}

fn direct() -> Reason {
    Reason::Defined(DIRECT.into())
}

#[test]
fn chat_reports_each_messages_mentions_and_a_direct_messages_counterpart() {
    let chain = chain();
    let create = crate::Op::CreateChannel {
        channel_id: "general".into(),
        name: "general".into(),
        post_policy: crate::PostPolicy::Open,
    };
    chain.submit(signed(ADA), crate::PROGRAM, &create).unwrap();
    post(&chain, ADA, "general", "plain", "no one in particular");
    assert_eq!(relations(&chain, chat_source("plain")), None);

    post(&chain, ADA, "general", "m1", "hi <@2> and <@1>");
    assert_eq!(held(&chain, chat_source("m1")), [(2, Reason::Mention)]);
    let edit = crate::Op::EditMessage {
        channel_id: "general".into(),
        seq: 2,
        blocks: crate::parse_message("only you, <@3>"),
        base_rev: Some(0),
    };
    chain.submit(signed(ADA), crate::PROGRAM, &edit).unwrap();
    assert_eq!(held(&chain, chat_source("m1")), [(3, Reason::Mention)]);
    let delete = crate::Op::DeleteMessage {
        channel_id: "general".into(),
        seq: 2,
    };
    chain.submit(signed(ADA), crate::PROGRAM, &delete).unwrap();
    let row = relations(&chain, chat_source("m1")).unwrap();
    assert_eq!((row.revision, row.relations), (3, Vec::new()));
    assert_eq!(
        changes_to(&chain, 2),
        [
            ("m1".into(), Reason::Mention, Changed::Added),
            ("m1".into(), Reason::Mention, Changed::Withdrawn)
        ]
    );
    assert_eq!(
        changes_to(&chain, 3),
        [
            ("m1".into(), Reason::Mention, Changed::Added),
            ("m1".into(), Reason::Mention, Changed::Withdrawn)
        ]
    );

    let dm = crate::Op::CreateDmChannel {
        counterpart: 2,
        name: "us".into(),
    };
    chain.submit(signed(ADA), crate::PROGRAM, &dm).unwrap();
    let room = crate::dm_channel_id(1, 2);
    post(&chain, ADA, &room, "d1", "a word");
    post(&chain, BO, &room, "d2", "back to you, <@1>");
    assert_eq!(held(&chain, chat_source("d1")), [(2, direct())]);
    assert_eq!(
        held(&chain, chat_source("d2")),
        [(1, Reason::Mention), (1, direct())]
    );
    let asked = attribution::Query::ChangesOf {
        source: chat_source("d2"),
        page: PageRequest::default(),
    };
    let Change { actor, cause, .. } = changes(&chain, asked).remove(0);
    assert_eq!(actor, Actor::Account(2));
    assert!(matches!(cause, Cause::Message(_)));
}

#[test]
fn a_post_stands_when_no_attribution_is_seated() {
    let mut chain = MockChain::default();
    chain.seat::<crate::Chat>(crate::PROGRAM);
    chain.hold(ADA.to_vec(), 1);
    let create = crate::Op::CreateChannel {
        channel_id: "general".into(),
        name: "general".into(),
        post_policy: crate::PostPolicy::Open,
    };
    chain.submit(signed(ADA), crate::PROGRAM, &create).unwrap();
    post(&chain, ADA, "general", "m1", "hi <@2>");
    let asked = crate::Query::MessageById {
        message_id: "m1".into(),
    };
    let crate::Reply::Message(Some(_)) = chain.query(crate::PROGRAM, &asked).unwrap() else {
        panic!()
    };
}
