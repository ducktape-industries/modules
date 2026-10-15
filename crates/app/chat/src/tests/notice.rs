//! What chat hands `push` as a message is posted, with a sink seated as `push` on a `MockChain`:
//! the notices of a mention and of a direct message, nothing on an edit or a delete, and a post
//! that stands whether `push` refuses or is not on the network at all.
use ::program::{Error, ExecCtx, MockChain, Origin, Program, QueryCtx, invalid};
use abi::role::identity::{Kind, Profile};
use push::Notice;

use crate::notice::of;
use crate::view::Names;
use crate::{Block, Mark, MsgRow, Principal, Span};

const ADA: &[u8] = b"ada-key";
const BO: &[u8] = b"bo-key";

/// `push` as chat sees it: every `Send` kept, in order.
struct Sink;

impl Program for Sink {
    const NAME: &'static str = push::PROGRAM;
    type Op = push::Op;
    type Query = ();
    type Reply = Vec<push::Op>;

    fn execute(ctx: &ExecCtx, op: push::Op) -> Result<(), Error> {
        let mut sent: Vec<push::Op> = ctx.record("sent")?.unwrap_or_default();
        sent.push(op);
        ctx.put("sent", &sent);
        Ok(())
    }

    fn query(ctx: &QueryCtx, (): ()) -> Result<Vec<push::Op>, Error> {
        Ok(ctx.record("sent")?.unwrap_or_default())
    }
}

/// A `push` that refuses every send.
struct Refusing;

impl Program for Refusing {
    const NAME: &'static str = push::PROGRAM;
    type Op = push::Op;
    type Query = ();
    type Reply = ();

    fn execute(_: &ExecCtx, _: push::Op) -> Result<(), Error> {
        Err(invalid("no"))
    }

    fn query(_: &QueryCtx, (): ()) -> Result<(), Error> {
        Ok(())
    }
}

fn chain(seat: impl FnOnce(&mut MockChain)) -> MockChain {
    let mut chain = MockChain::default();
    chain.seat::<crate::Chat>(crate::PROGRAM);
    seat(&mut chain);
    chain.register(crate::PROGRAM, 901);
    for (key, number, name) in [(ADA, 1, "Ada"), (BO, 2, "Bo")] {
        chain.hold(key.to_vec(), number);
        chain.profile(Profile {
            number,
            name: name.into(),
            kind: Kind::User,
        });
    }
    let create = crate::Op::CreateChannel {
        channel_id: "general".into(),
        name: "general".into(),
        post_policy: crate::PostPolicy::Open,
    };
    chain.submit(signed(ADA), crate::PROGRAM, &create).unwrap();
    chain
}

fn with_sink() -> MockChain {
    chain(|chain| {
        chain.seat::<Sink>(push::PROGRAM);
    })
}

fn signed(key: &[u8]) -> Origin {
    Origin::Signed(key.to_vec())
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

fn sent(chain: &MockChain) -> Vec<Vec<Notice>> {
    let sent: Vec<push::Op> = chain.query(push::PROGRAM, &()).unwrap();
    sent.into_iter()
        .map(|push::Op::Send { notices }| notices)
        .collect()
}

fn notice(to: Vec<u64>, title: &str, body: &str, tag: &str, link: [&str; 2]) -> Notice {
    Notice::new(to, title, body, tag.into(), link.map(String::from).to_vec())
}

#[test]
fn a_mention_sends_one_notice_to_the_mentioned_account_not_the_author() {
    let chain = with_sink();
    post(&chain, ADA, "general", "plain", "no one in particular");
    assert!(sent(&chain).is_empty());
    post(&chain, ADA, "general", "m1", "hi <@2> and <@1>");
    let words = "hi @Bo and @Ada";
    let mention = notice(
        vec![2],
        "Ada mentioned you",
        words,
        "#general",
        ["general", "2"],
    );
    assert_eq!(sent(&chain), [vec![mention]]);
}

#[test]
fn a_direct_message_sends_its_counterpart_the_direct_notice_alone() {
    let chain = with_sink();
    let dm = crate::Op::CreateDmChannel {
        counterpart: 2,
        name: "us".into(),
    };
    chain.submit(signed(ADA), crate::PROGRAM, &dm).unwrap();
    let room = crate::dm_channel_id(1, 2);
    post(&chain, BO, &room, "d1", "back to you, <@1>");
    let direct = notice(vec![1], "Bo", "back to you, @Ada", "@Bo", [&room, "1"]);
    assert_eq!(sent(&chain), [vec![direct]]);
}

#[test]
fn an_edit_or_a_delete_sends_nothing() {
    let chain = with_sink();
    post(&chain, ADA, "general", "m1", "hi");
    let edit = crate::Op::EditMessage {
        channel_id: "general".into(),
        seq: 1,
        blocks: crate::parse_message("now you, <@2>"),
        base_rev: Some(0),
    };
    chain.submit(signed(ADA), crate::PROGRAM, &edit).unwrap();
    let delete = crate::Op::DeleteMessage {
        channel_id: "general".into(),
        seq: 1,
    };
    chain.submit(signed(ADA), crate::PROGRAM, &delete).unwrap();
    assert!(sent(&chain).is_empty());
}

#[test]
fn a_post_stands_when_push_refuses_or_is_not_on_the_network() {
    let refusing = chain(|chain| {
        chain.seat::<Refusing>(push::PROGRAM);
    });
    let absent = chain(|_| {});
    for chain in [refusing, absent] {
        post(&chain, ADA, "general", "m1", "hi <@2>");
        let asked = crate::Query::MessageById {
            message_id: "m1".into(),
        };
        let crate::Reply::Message(Some(row)) = chain.query(crate::PROGRAM, &asked).unwrap() else {
            panic!("the post landed");
        };
        assert_eq!(row.seq, 1);
    }
}

#[test]
fn a_deleted_row_and_the_authors_own_mention_make_no_notice() {
    let me = |number| Mark::Mention(Principal::Account(number));
    let row = MsgRow {
        channel_id: "general".into(),
        seq: 7,
        blocks: vec![Block::Paragraph(vec![Span {
            text: "@me".into(),
            marks: vec![me(1)],
        }])],
        ..MsgRow::by(Principal::Account(1))
    };
    let names = Names::default();
    assert!(of(&row, "general", &names).is_empty());
    let deleted = MsgRow {
        deleted: true,
        blocks: vec![Block::Paragraph(vec![Span {
            text: "@you".into(),
            marks: vec![me(2)],
        }])],
        ..row
    };
    assert!(of(&deleted, "general", &names).is_empty());
}
