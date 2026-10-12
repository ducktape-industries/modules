//! The tab badge, the read cursors kept between runs, and how far the reader
//! has been shown each tag (`notify.seen`).
use super::*;
use ducktape_view_guest::methods::{NotifySeen, Seen};

fn seen(tag: &str, through_height: u64) -> Seen {
    Seen {
        tag: tag.into(),
        through_height,
    }
}

/// `row` as a block at `height` wrote it.
fn at(height: u64, row: MsgRow) -> MsgRow {
    MsgRow { height, ..row }
}

/// A reply to root 1 of `general`.
fn reply(seq: u64, height: u64) -> MsgRow {
    MsgRow {
        thread: Some(1),
        ..at(height, row(seq, 8, "re"))
    }
}

/// Chat answers `general` with `roots`, and root 1's thread with
/// `replies`, a page at a time: each page but the last says more remain.
fn answers(cx: &mut TestAppContext, roots: Vec<MsgRow>, replies: Vec<Vec<MsgRow>>) {
    cx.host().handle::<Ask<::chat::Chat>>(move |query| {
        Ok(match query {
            Query::Channels { .. } => Reply::Channels(page(vec![
                channel("general", "General", 3),
                channel("dm-7-8", "dm", 1),
            ])),
            Query::Roots { .. } => Reply::Roots(page(roots.clone())),
            Query::Members { .. } => Reply::Members(page(Vec::new())),
            Query::Thread {
                root_seq: 1,
                page: asked,
                ..
            } => {
                let at = asked.after.map_or(0, |after| usize::from(after[0]));
                let next = (at + 1 < replies.len()).then(|| vec![at as u8 + 1]);
                Reply::Thread {
                    root: Some(row(1, 7, "hello")),
                    replies: ::chat::PageResponse {
                        height: 1,
                        items: replies[at].clone(),
                        next,
                    },
                }
            }
            query => panic!("unexpected chat query: {query:?}"),
        })
    });
}

/// The timeline opened at its head tells the host how far the reader has been
/// shown the room: its newest root's height under `<channel>`, again each
/// time a newer root lands, never twice for the same height (a frame drawn
/// again, the same rows landing again).
#[test]
fn the_timeline_marks_the_room_through_its_newest_root() {
    let (mut cx, view) = opened();
    assert_eq!(cx.host().requests::<NotifySeen>(), [seen("general", 1)]);
    let roots = vec![row(1, 7, "hello"), at(9, row(3, 8, "new"))];
    answers(&mut cx, roots, vec![Vec::new()]);
    let changes = cx.host().stream::<Changes<::chat::Chat>>();
    changes.send(block(9, posted("general")));
    cx.run_until_parked();
    changes.send(block(10, posted("general")));
    cx.run_until_parked();
    cx.update(&view, |_, _, cx| cx.notify());
    cx.run_until_parked();
    assert_eq!(
        cx.host().requests::<NotifySeen>(),
        [seen("general", 1), seen("general", 9)]
    );
}

/// The thread pane tells the host how far the reader has been shown the
/// thread, under `<channel>/<root>`, once it shows the newest replies: not
/// while a later page is still unread. Neither pane's watermark names the
/// other's tag.
#[test]
fn the_thread_marks_its_tag_through_its_newest_reply() {
    let (mut cx, view) = opened();
    let pages = vec![vec![reply(2, 4)], vec![reply(3, 6)]];
    answers(&mut cx, vec![row(1, 7, "hello")], pages);
    thread(&mut cx, &view, 1);
    assert_eq!(cx.host().requests::<NotifySeen>(), [seen("general", 1)]);
    cx.update(&view, |chat, _, cx| {
        cx.notify();
        chat.load_more_replies(cx);
    });
    cx.run_until_parked();
    assert_eq!(
        cx.host().requests::<NotifySeen>(),
        [seen("general", 1), seen("general/1", 6)]
    );
}

/// A room that is not on screen tells the host nothing, however its rows
/// move; brought back in front, it marks what it shows.
#[test]
fn a_room_not_in_front_marks_nothing() {
    let (mut cx, _) = opened();
    cx.host().stream::<HostVisible>().send(false);
    cx.run_until_parked();
    answers(&mut cx, vec![at(9, row(3, 8, "new"))], vec![Vec::new()]);
    cx.host()
        .stream::<Changes<::chat::Chat>>()
        .send(block(9, posted("general")));
    cx.run_until_parked();
    assert_eq!(cx.host().requests::<NotifySeen>(), [seen("general", 1)]);
    cx.host().stream::<HostVisible>().send(true);
    cx.run_until_parked();
    assert_eq!(
        cx.host().requests::<NotifySeen>(),
        [seen("general", 1), seen("general", 9)]
    );
}

/// A watermark is the reader's: another account seated in the view marks
/// what it is shown from none.
#[test]
fn another_reader_marks_from_none() {
    let (mut cx, _) = opened();
    cx.host().stream::<HostSession>().send(Session {
        account: Some(8),
        ..session()
    });
    cx.run_until_parked();
    assert_eq!(
        cx.host().requests::<NotifySeen>(),
        [seen("general", 1), seen("general", 1)]
    );
}

/// A direct message landing in a room the reader is not in is counted on
/// the tab until they open the room.
#[test]
fn a_direct_message_elsewhere_is_a_badge_until_read() {
    use ducktape_view_guest::methods::HostBadge;
    let (mut cx, view) = opened();
    cx.host().handle::<Ask<::chat::Chat>>(|query| {
        Ok(match query {
            Query::MessagesAround { channel_id, .. } => {
                assert_eq!(channel_id, "dm-7-8");
                let mut ping = row(2, 8, "ping");
                ping.channel_id = channel_id;
                Reply::Messages(vec![row(1, 7, "old"), ping])
            }
            Query::Roots { .. } => Reply::Roots(page(Vec::new())),
            Query::Members { .. } => Reply::Members(page(Vec::new())),
            query => panic!("unexpected chat query: {query:?}"),
        })
    });
    cx.update(&view, |chat, _, cx| {
        cx.notify();
        chat.channels_landed(
            vec![channel("general", "General", 3), channel("dm-7-8", "dm", 2)],
            cx,
        );
    });
    cx.run_until_parked();
    assert_eq!(cx.host().requests::<HostBadge>().last(), Some(&1));
    cx.update(&view, |chat, _, cx| {
        cx.notify();
        chat.choose("dm-7-8".into(), cx)
    });
    cx.run_until_parked();
    assert_eq!(cx.host().requests::<HostBadge>().last(), Some(&0));
}

/// A reload carries the read cursors but not the count: the first list after
/// it counts again what is meant for the reader in rooms still unread.
#[test]
fn the_badge_is_counted_again_from_the_read_cursors() {
    use ducktape_view_guest::methods::HostBadge;
    let (mut cx, view) = opened();
    cx.host().handle::<Ask<::chat::Chat>>(|query| {
        Ok(match query {
            Query::MessagesAround { channel_id, .. } => {
                let mut ping = row(2, 8, "ping");
                ping.channel_id = channel_id;
                Reply::Messages(vec![row(1, 7, "old"), ping])
            }
            Query::Roots { .. } => Reply::Roots(page(Vec::new())),
            Query::Members { .. } => Reply::Members(page(Vec::new())),
            query => panic!("unexpected chat query: {query:?}"),
        })
    });
    let rooms = || vec![channel("general", "General", 3), channel("dm-7-8", "dm", 2)];
    cx.update(&view, |chat, _, cx| {
        cx.notify();
        // as a reload leaves it: the rooms and the cursors, no count
        chat.channels = Loadable::Ready(rooms());
        chat.reads.cursors.insert("general".into(), 3);
        chat.reads.cursors.insert("dm-7-8".into(), 1);
        chat.attention.clear();
        chat.badge = None;
        chat.recounted = false;
        chat.channels_landed(rooms(), cx);
    });
    cx.run_until_parked();
    assert_eq!(cx.host().requests::<HostBadge>().last(), Some(&1));
}

/// A relaunch starts with no cursors in memory: the ones kept on the device
/// come back, so a mention and a direct message still unread count on the
/// badge and dot their rooms, and reading a room keeps its new cursor.
#[test]
fn kept_cursors_bring_the_badge_back_after_a_relaunch() {
    use ducktape_view_guest::methods::{self, HostBadge, StoreGet, StoreSet};
    use std::collections::BTreeMap;
    let mut cx = TestAppContext::new();
    configure(&mut cx);
    cx.host().handle::<StoreGet>(|key| {
        Ok((key == "reads/0102").then(|| {
            methods::encode(&BTreeMap::from([
                ("general".to_owned(), 1u64),
                ("dm-7-8".to_owned(), 0),
                ("gone".to_owned(), 4),
            ]))
        }))
    });
    cx.host().handle::<Ask<::chat::Chat>>(|query| {
        Ok(match query {
            Query::Accounts { .. } => Reply::Accounts(page(Vec::new())),
            Query::Channels { .. } => Reply::Channels(page(vec![
                channel("general", "General", 3),
                channel("dm-7-8", "dm", 1),
            ])),
            Query::MessagesAround { channel_id, .. } => {
                let mut ping = row(2, 8, "@eddy");
                ping.blocks = vec![chat::Block::Paragraph(vec![chat::Span {
                    text: "@eddy".into(),
                    marks: vec![chat::Mark::Mention(chat::Principal::Account(7))],
                }])];
                if channel_id == "dm-7-8" {
                    ping = row(1, 8, "hi");
                }
                ping.channel_id = channel_id;
                Reply::Messages(vec![ping])
            }
            Query::Roots { .. } => Reply::Roots(page(Vec::new())),
            Query::Members { .. } => Reply::Members(page(Vec::new())),
            query => panic!("unexpected chat query: {query:?}"),
        })
    });
    cx.open::<Chat>();
    seat(&mut cx);
    assert_eq!(cx.host().requests::<HostBadge>().last(), Some(&2));
    assert!(cx.find("chat-sidebar-channel-general-unread").is_some());
    assert!(cx.find("chat-sidebar-dm-8-unread").is_some());

    cx.simulate_click("chat-sidebar-channel-general");
    cx.run_until_parked();
    assert_eq!(cx.host().requests::<HostBadge>().last(), Some(&1));
    let (key, kept) = cx
        .host()
        .requests::<StoreSet>()
        .pop()
        .expect("the read is kept");
    assert_eq!(key, "reads/0102");
    let kept: BTreeMap<String, u64> = methods::decode(&kept.unwrap()).unwrap();
    assert_eq!(
        kept,
        BTreeMap::from([("general".into(), 3), ("dm-7-8".into(), 0)]),
        "a room no longer listed is not kept"
    );
}

/// On a relaunch the kept cursors and the room list can land before the
/// host names the reader's account: the recount waits for their account, then
/// runs without another chat write to prompt it.
#[test]
fn the_relaunch_recount_waits_for_the_readers_account() {
    use ducktape_view_guest::methods::{self, HostBadge, StoreGet};
    use std::collections::BTreeMap;
    let mut cx = TestAppContext::new();
    configure(&mut cx);
    cx.host().handle::<StoreGet>(|key| {
        Ok((key == "reads/0102")
            .then(|| methods::encode(&BTreeMap::from([("dm-7-8".to_owned(), 0u64)]))))
    });
    cx.host().handle::<Ask<::chat::Chat>>(|query| {
        Ok(match query {
            Query::Accounts { .. } => Reply::Accounts(page(Vec::new())),
            Query::Channels { .. } => Reply::Channels(page(vec![channel("dm-7-8", "dm", 1)])),
            Query::MessagesAround { channel_id, .. } => {
                let mut hi = row(1, 8, "hi");
                hi.channel_id = channel_id;
                Reply::Messages(vec![hi])
            }
            query => panic!("unexpected chat query: {query:?}"),
        })
    });
    let props = cx.host().stream::<HostSession>();
    cx.open::<Chat>();
    props.send(Session {
        account: None,
        ..session()
    });
    cx.host().stream::<HostVisible>().send(true);
    cx.run_until_parked();
    assert_ne!(cx.host().requests::<HostBadge>().last(), Some(&1));

    // the host names their account
    props.send(session());
    cx.run_until_parked();
    assert_eq!(cx.host().requests::<HostBadge>().last(), Some(&1));
}

/// A device store that refuses the kept cursors is asked again, then chat
/// starts from what it sees: reading a room still keeps its cursor.
#[test]
fn a_refused_store_read_still_keeps_cursors() {
    use ducktape_view_guest::host::malformed;
    use ducktape_view_guest::methods::{self, StoreGet, StoreSet};
    use std::collections::BTreeMap;
    let mut cx = TestAppContext::new();
    configure(&mut cx);
    cx.host().handle::<StoreGet>(|key| match key.as_str() {
        "reads/0102" => Err(malformed("the store is unavailable".into())),
        _ => Ok(None),
    });
    cx.host().handle::<Ask<::chat::Chat>>(|query| {
        Ok(match query {
            Query::Accounts { .. } => Reply::Accounts(page(Vec::new())),
            Query::Channels { .. } => Reply::Channels(page(vec![channel("general", "General", 3)])),
            Query::Roots { .. } => Reply::Roots(page(Vec::new())),
            Query::Members { .. } => Reply::Members(page(Vec::new())),
            query => panic!("unexpected chat query: {query:?}"),
        })
    });
    cx.open::<Chat>();
    seat(&mut cx);
    let asked = cx.host().requests::<StoreGet>();
    assert!(
        asked.iter().filter(|key| *key == "reads/0102").count() > 1,
        "asked again before giving up: {asked:?}"
    );
    cx.simulate_click("chat-sidebar-channel-general");
    cx.run_until_parked();
    let (_, kept) = cx
        .host()
        .requests::<StoreSet>()
        .into_iter()
        .rfind(|(key, _)| key == "reads/0102")
        .expect("the read is kept");
    let kept: BTreeMap<String, u64> = methods::decode(&kept.unwrap()).unwrap();
    assert_eq!(kept, BTreeMap::from([("general".into(), 3)]));
}
