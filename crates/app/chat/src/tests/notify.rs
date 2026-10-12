//! What a post tells the people it concerns (`notify.rs`): the notices in
//! its receipt, read back as the inbox reads them.
use ::notices::{Kind, Link, Notice, NoticeEvent};

use super::channels::{edit, membership, open_dm};
use super::*;

impl Chat {
    /// The events every op since the last call emitted, decoded: chat
    /// emits notices alone.
    fn events(&self) -> Vec<NoticeEvent> {
        let events = std::mem::take(&mut self.store.borrow_mut().events);
        events
            .iter()
            .map(|bytes| ::notices::decode(bytes).expect("a notice").unwrap())
            .collect()
    }

    /// The notices the ops since the last call posted.
    fn notices(&self) -> Vec<Notice> {
        self.events()
            .into_iter()
            .map(|event| match event {
                NoticeEvent::Post(notice) => notice,
                other => panic!("a post, not {other:?}"),
            })
            .collect()
    }

    /// The one notice the ops since the last call posted.
    #[track_caller]
    fn notice(&self) -> Notice {
        let mut notices = self.notices();
        assert_eq!(notices.len(), 1, "{notices:?}");
        notices.remove(0)
    }
}

/// Identity holds `numbers` as people too, beside the suite's roster.
fn people(chat: &Chat, numbers: impl IntoIterator<Item = u64>) {
    let mut roster = roster();
    roster.extend(numbers.into_iter().map(|number| crate::Profile {
        number,
        name: format!("user{number}"),
        kind: crate::Kind::Person,
    }));
    chat.store.identity(roster);
}

fn link(program: &str, segments: &[&str]) -> Link {
    Link {
        program: program.into(),
        segments: segments.iter().map(|s| s.to_string()).collect(),
    }
}

/// A dm tells the other peer, once: naming them too is still one Direct,
/// and a third account named there is no member of the room.
#[test]
fn a_dm_tells_the_other_peer() {
    let mut chat = Chat::default();
    chat.ok(&ADA, open_dm(2));
    chat.ok(&ADA, post("dm-1-2", "m1", "hi <@2>, ask <@3>", None));
    assert_eq!(
        chat.notice(),
        Notice {
            key: "dm-1-2/1".into(),
            to: vec![2],
            from: Some(1),
            kind: Kind::Direct,
            title: "user1".into(),
            body: "hi @user2, ask @user3".into(),
            tag: "dm-1-2".into(),
            link: link("chat", &["dm-1-2", "1"]),
        }
    );
    chat.ok(&BO, post("dm-1-2", "m2", "back", None));
    assert_eq!(chat.notice().to, [1]);
}

/// In an open room a mention reaches anyone named, each once, the author
/// never; the body reads names, not tokens.
#[test]
fn a_mention_in_an_open_room_reaches_anyone_named() {
    let mut chat = Chat::with_channel(PostPolicy::Open);
    chat.post(&BO, "m1", "ask <@3> and **<@3>**, <@4> and me <@2>", None);
    assert_eq!(
        chat.notice(),
        Notice {
            key: "general/1".into(),
            to: vec![3, 4],
            from: Some(2),
            kind: Kind::Mention,
            title: "user2 mentioned you in #general".into(),
            body: "ask @user3 and @user3, @user4 and me @user2".into(),
            tag: "general".into(),
            link: link("chat", &["general", "1"]),
        }
    );
}

/// In a members-only room a mention reaches its owner and seated members:
/// naming anyone else tells no one.
#[test]
fn a_mention_in_a_members_only_room_reaches_its_members_alone() {
    let mut chat = Chat::with_channel(PostPolicy::MembersOnly);
    chat.ok(&ADA, membership(BO, true));
    chat.post(&BO, "m1", "<@1> <@3> <@2>", None);
    assert_eq!(chat.notice().to, [1]);
    chat.post(&ADA, "m2", "<@3> is not here", None);
    assert_eq!(chat.notices(), []);
    chat.post(&ADA, "m3", "but <@2> is", None);
    assert_eq!(chat.notice().to, [2]);
}

#[test]
fn naming_yourself_tells_no_one() {
    let mut chat = Chat::with_channel(PostPolicy::Open);
    chat.post(&BO, "m1", "note to <@2>", None);
    assert_eq!(chat.notices(), []);
}

/// A number no account holds has no reader: naming it tells no one, and
/// the others named are still told.
#[test]
fn naming_a_number_nobody_holds_tells_no_one() {
    let mut chat = Chat::with_channel(PostPolicy::Open);
    chat.post(&BO, "m1", "ping <@4000>", None);
    assert_eq!(chat.notices(), []);
    chat.post(&BO, "m2", "<@4000> and <@3>", None);
    assert_eq!(chat.notice().to, [3]);
}

/// A program's account has no reader: naming forge's tells no one.
#[test]
fn naming_a_programs_account_tells_no_one() {
    let mut chat = Chat::with_channel(PostPolicy::Open);
    chat.post(&BO, "m1", "ping <@900>", None);
    assert_eq!(chat.notices(), []);
}

/// D5 holds for a Reply too: a root's author unseated from a members-only
/// room hears no more of its thread.
#[test]
fn a_reply_reaches_a_members_only_rooms_members_alone() {
    let mut chat = Chat::with_channel(PostPolicy::MembersOnly);
    chat.ok(&ADA, membership(BO, true));
    chat.post(&BO, "m1", "question", None);
    chat.post(&ADA, "m2", "first", Some(1));
    assert_eq!(chat.notice().to, [2]);
    chat.ok(&ADA, membership(BO, false));
    chat.post(&ADA, "m3", "second", Some(1));
    assert_eq!(chat.notices(), []);
}

/// A post that concerns no one, an edit, and a reaction emit nothing.
#[test]
fn a_post_for_no_one_emits_nothing() {
    let mut chat = Chat::with_channel(PostPolicy::Open);
    chat.post(&BO, "m1", "hello", None);
    chat.ok(&BO, edit(1, "hello <@3>"));
    chat.ok(&CY, react(1, "👍", true));
    assert!(chat.store.borrow().events.is_empty());
}

/// A reply tells the root's author, under the thread's tag; one the root's
/// author already hears of by name is a Mention alone; a reply that names
/// someone else is a Mention to them and a Reply to the root's author,
/// both under the post's key. The root's author answering tells no one.
#[test]
fn a_reply_tells_the_roots_author() {
    let mut chat = Chat::with_channel(PostPolicy::Open);
    chat.post(&ADA, "m1", "question", None);
    assert_eq!(chat.notices(), []);
    chat.post(&BO, "m2", "sure", Some(1));
    assert_eq!(
        chat.notice(),
        Notice {
            key: "general/2".into(),
            to: vec![1],
            from: Some(2),
            kind: Kind::Reply,
            title: "user2 replied in #general".into(),
            body: "sure".into(),
            tag: "general/1".into(),
            link: link("chat", &["general", "2"]),
        }
    );
    chat.post(&CY, "m3", "<@1> see <@4>", Some(1));
    let named = chat.notice();
    assert_eq!((named.kind, named.to), (Kind::Mention, vec![1, 4]));
    chat.post(&CY, "m4", "<@4> again", Some(1));
    let kinds: Vec<(Kind, Vec<u64>, String, String)> = chat
        .notices()
        .into_iter()
        .map(|n| (n.kind, n.to, n.key, n.tag))
        .collect();
    let under = |kind, to: u64| (kind, vec![to], "general/4".into(), "general/1".into());
    assert_eq!(kinds, [under(Kind::Mention, 4), under(Kind::Reply, 1)]);
    chat.post(&ADA, "m5", "thanks", Some(1));
    assert_eq!(chat.notices(), []);
}

/// A program's line has no reader: a reply under one tells no one.
#[test]
fn a_reply_under_a_programs_root_tells_no_one() {
    let mut chat = Chat::default();
    chat.ok(&FORGE, create("forge:web:3", PostPolicy::Open));
    chat.ok(
        &FORGE,
        Op::PostMessage {
            channel_id: "forge:web:3".into(),
            message_id: "forge:0001".into(),
            blocks: vec![crate::Block::Code {
                lang: Some("forge".into()),
                text: "review 7".into(),
            }],
            thread: None,
        },
    );
    chat.ok(&BO, post("forge:web:3", "m2", "looks good", Some(1)));
    assert_eq!(chat.notices(), []);
}

/// Chat applies the same rules when the poster is a module's account
/// (forge posting into its own room): its system line names no one and
/// tells no one; a line naming an account tells it, from forge's account.
/// A notice from a program's room opens where the program shows it.
#[test]
fn a_forge_discussion_post_tells_as_any_post_does() {
    let mut chat = Chat::default();
    let room = Op::CreateChannel {
        channel_id: "forge:web:3".into(),
        name: "web#3".into(),
        post_policy: PostPolicy::Open,
    };
    chat.ok(&FORGE, room);
    chat.ok(
        &FORGE,
        post("forge:web:3", "forge:0001", "```forge\nopened\n```", None),
    );
    assert_eq!(chat.notices(), []);
    chat.ok(
        &FORGE,
        post("forge:web:3", "forge:0002", "<@1> asked", None),
    );
    let notice = chat.notice();
    assert_eq!(
        (notice.from, notice.title.as_str(), notice.to),
        (
            Some(900),
            "user900 mentioned you in web#3 on forge",
            vec![1]
        )
    );
    assert_eq!(notice.link, link("forge", &["web", "3"]));
    chat.ok(&BO, post("forge:web:3", "m3", "<@3> look", Some(2)));
    let notice = chat.notice();
    assert_eq!((notice.kind, notice.to), (Kind::Mention, vec![3]));
    assert_eq!(notice.tag, "forge:web:3/2");
    assert_eq!(notice.link, link("forge", &["web", "3"]));
}

/// A message naming more than 64 accounts is refused, untouched: no row,
/// no event. 64 is a post, and its notice reaches all of them.
#[test]
fn a_message_names_at_most_64_accounts() {
    let mut chat = Chat::with_channel(PostPolicy::Open);
    let names = |accounts: std::ops::RangeInclusive<u64>| {
        accounts
            .map(|n| format!("<@{n}>"))
            .collect::<Vec<_>>()
            .join(" ")
    };
    people(&chat, (8..=9).chain(13..=67));
    let refused = chat.refused(&BO, post("general", "m1", &names(3..=67), None));
    assert_eq!(refused, code::CAPACITY);
    chat.post(&BO, "m1", &format!("{} <@3>", names(3..=66)), None);
    assert_eq!(chat.notice().to, (3..=66).collect::<Vec<_>>());
}

/// A delete withdraws the post's notices by their key.
#[test]
fn a_delete_withdraws_the_posts_notices() {
    let mut chat = Chat::with_channel(PostPolicy::Open);
    chat.post(&BO, "m1", "<@3>", None);
    chat.notices();
    chat.ok(&BO, delete(1));
    assert_eq!(
        chat.events(),
        [NoticeEvent::Retract {
            key: "general/1".into()
        }]
    );
    chat.ok(&ADA, delete(1));
    assert_eq!(chat.events(), []);
}

/// A profile identity refuses never refuses the post: its account is
/// called by its number.
#[test]
fn a_profile_identity_refuses_falls_back_to_the_number() {
    let mut chat = Chat::with_channel(PostPolicy::Open);
    let refuses: guest::Sibling = Box::new(|_, _| Err(guest::unauthorized("not today")));
    let identity = MockHost::roles().identity;
    chat.store.borrow_mut().siblings.insert(identity, refuses);
    chat.post(&BO, "m1", "hi <@3>", None);
    let notice = chat.notice();
    assert_eq!(
        (notice.title.as_str(), notice.body.as_str()),
        ("account 2 mentioned you in #general", "hi @account-3")
    );
    // a root's author identity says nothing of is taken for a person
    chat.post(&ADA, "m2", "question", None);
    chat.post(&BO, "m3", "answer", Some(2));
    let notice = chat.notice();
    assert_eq!(
        (notice.kind, notice.to, notice.title.as_str()),
        (Kind::Reply, vec![1], "account 2 replied in #general")
    );
}

/// What a notice costs in the receipt: a typical one, and one naming 64
/// accounts with its title and body at their cap. Printed for the record
/// (`--nocapture`), bounded here.
#[test]
fn a_notice_costs_its_bytes_in_the_receipt() {
    let mut chat = Chat::with_channel(PostPolicy::Open);
    chat.post(
        &BO,
        "m1",
        "<@3> can you look at the build before the release?",
        None,
    );
    let typical = ::notices::encode(&NoticeEvent::Post(chat.notice())).len();
    people(&chat, 1000..1064);
    let names: Vec<String> = (1000..1064).map(|n| format!("<@{n}>")).collect();
    let text = format!("{} {}", names.join(" "), "word ".repeat(200));
    chat.post(&BO, "m2", &text, None);
    let mut widest = chat.notice();
    assert_eq!((widest.to.len(), widest.body.len()), (64, 512));
    widest.title = "t".repeat(::notices::MAX_TEXT_BYTES);
    let worst = ::notices::encode(&NoticeEvent::Post(widest)).len();
    println!("notice event bytes: typical {typical}, 64 recipients at the caps {worst}");
    assert!(typical < 300 && worst < 1700, "{typical} {worst}");
}
