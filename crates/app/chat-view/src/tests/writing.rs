//! Sending, and creating channels.
use super::*;

#[test]
fn a_send_shows_pending_then_lands_and_a_refusal_is_a_banner() {
    let (mut cx, view) = opened();
    let pending = MsgRow {
        message_id: "p1".into(),
        blocks: vec![chat::Block::paragraph("on its way")],
        ..MsgRow::by(Principal::Account(7))
    };
    cx.update(&view, |chat, _, cx| {
        chat.room.as_mut().unwrap().pending.push(pending.clone());
        cx.notify();
    });
    cx.run_until_parked();
    assert!(cx.has_text("on its way") && cx.has_text("sending…"));
    cx.update(&view, |chat, _, cx| {
        let room = chat.room.as_mut().unwrap();
        room.messages
            .ready_mut()
            .unwrap()
            .push(MsgRow { seq: 3, ..pending });
        room.settle();
        cx.notify();
    });
    cx.run_until_parked();
    assert!(cx.has_text("on its way") && !cx.has_text("sending…"));
    cx.simulate_input("chat-sidebar-search", "hello");
    cx.simulate_submit("chat-sidebar-search");
    cx.run_until_parked();
    assert!(cx.has_text("2 results for “hello”"), "{:?}", cx.texts());
    // the clear control is a glyph named in words
    cx.simulate_click("chat-sidebar-clear-search");
    view.read(|chat| assert!(chat.search.query.is_empty()));
    cx.host().handle::<HostId>(|kind| {
        assert_eq!(kind, "channel");
        Ok("chan-1".into())
    });
    cx.host().refuse::<Submit<::chat::Chat>>("no", "no");
    cx.simulate_click("chat-sidebar-new-channel");
    assert!(cx.has_text("Create a channel"));
    cx.simulate_input("chat-create-name", "random");
    cx.simulate_click("chat-create-members");
    cx.simulate_submit("chat-create-name");
    cx.run_until_parked();
    assert!(cx.host().requests::<Submit<::chat::Chat>>().iter().any(|op| matches!(op, Op::CreateChannel { name, post_policy: PostPolicy::MembersOnly, .. } if name == "random")));
    assert!(cx.has_text("Couldn’t create this channel: no"));
    let bytes = cx.snapshot().unwrap();
    let mut restored = TestAppContext::new();
    configure(&mut restored);
    let view = restored.restore::<Chat>(&bytes).unwrap();
    restored.run_until_parked();
    view.read(|chat| assert_eq!(chat.room.as_ref().unwrap().id, "general"));
    assert!(restored.host().requests::<Ask<::chat::Chat>>().len() >= 2);
}

#[test]
fn channel_create_preserves_busy_and_account_gates() {
    fn disabled(cx: &TestAppContext, id: &str) -> bool {
        let interactivity = cx.interactivity(id);
        interactivity.aria.disabled == Some(true) && interactivity.on_click.is_none()
    }

    let (mut cx, view) = opened();
    cx.simulate_click("chat-sidebar-new-channel");
    cx.update(&view, |chat, _, cx| {
        chat.create.as_mut().unwrap().busy = true;
        cx.notify();
    });
    cx.run_until_parked();
    let Some(wire::Node::Field {
        options, on_submit, ..
    }) = cx.find("chat-create-name")
    else {
        panic!("channel name input")
    };
    assert!(options.disabled);
    assert!(on_submit.is_none());
    for id in [
        "chat-create-members",
        "chat-create-cancel",
        "chat-create-submit",
    ] {
        assert!(disabled(&cx, id), "{id} must stay inert while busy");
    }

    cx.update(&view, |chat, _, cx| {
        let create = chat.create.as_mut().unwrap();
        create.busy = false;
        chat.session.account = None;
        cx.notify();
    });
    cx.run_until_parked();
    assert!(disabled(&cx, "chat-create-submit"));
    assert!(cx.has_text("Create an account to create a channel"));
    assert!(!disabled(&cx, "chat-create-cancel"));
    let submitted = cx.host().requests::<Submit<::chat::Chat>>().len();
    cx.update(&view, |chat, _, cx| chat.create_channel(cx));
    cx.run_until_parked();
    assert_eq!(
        cx.host().requests::<Submit<::chat::Chat>>().len(),
        submitted
    );

    cx.update(&view, |chat, _, cx| {
        chat.session.account = Some(7);
        chat.session.connected = false;
        cx.notify();
    });
    cx.run_until_parked();
    assert!(disabled(&cx, "chat-create-submit"));
}

/// The composer follows the program's rule for a members-only room: its
/// owner writes without a seat, a seated member writes, a stranger reads.
#[test]
fn a_members_only_room_takes_its_owner_and_its_members() {
    let (mut cx, view) = opened();
    let gate = |cx: &mut TestAppContext, owner: u64, seated: bool| {
        cx.update(&view, |chat, _, cx| {
            let general = chat
                .channels
                .ready_mut()
                .unwrap()
                .iter_mut()
                .find(|info| info.channel.id == "general")
                .unwrap();
            general.channel.post_policy = PostPolicy::MembersOnly;
            general.channel.owner = Principal::Account(owner);
            let seats = if seated {
                vec![chat::MemberRow {
                    principal: Principal::Account(7),
                    height: 1,
                    time: 1,
                }]
            } else {
                Vec::new()
            };
            chat.room.as_mut().unwrap().members = Loadable::Ready(seats);
            cx.notify();
        });
        view.read(|chat| chat.write_gate())
    };
    assert_eq!(gate(&mut cx, 7, false), None, "the owner needs no seat");
    assert_eq!(gate(&mut cx, 8, true), None, "a seated member writes");
    assert_eq!(
        gate(&mut cx, 8, false),
        Some(crate::session::Gate::NotMember),
        "a stranger reads"
    );
}

/// Hits in two channels at the same seq are two rows the host can tell
/// apart; one identity for both stopped the view.
#[test]
fn search_hits_in_two_channels_at_one_seq_are_two_rows() {
    let (mut cx, _) = opened();
    cx.simulate_input("chat-sidebar-search", "hello");
    cx.simulate_submit("chat-sidebar-search");
    cx.run_until_parked();
    assert!(cx.find("chat-search-hit-general-1").is_some());
    assert!(cx.find("chat-search-hit-dm-7-8-1").is_some());
    // the host's sanitizer refuses duplicate typed identities among siblings
    cx.frame_bytes();
}

#[test]
fn a_first_post_to_a_dm_opens_it_and_a_listed_one_does_not() {
    let (mut cx, view) = opened();
    cx.run_until_parked();
    let post = |channel: &str| crate::composer::Target::Post {
        channel: channel.into(),
        thread: None,
    };
    view.read(|chat| {
        assert!(matches!(
            chat.dm_to_open(&post("dm-7-9")),
            Some(Op::CreateDmChannel { counterpart: 9, .. })
        ));
        assert!(chat.dm_to_open(&post("dm-7-8")).is_none(), "already open");
        assert!(chat.dm_to_open(&post("general")).is_none(), "not a dm");
    });
}

/// Nodes one composer keystroke lowers: the root's own, the composer with
/// it. Pinned from the first measurement; the rooms and the pane's rows
/// stand in.
const KEYSTROKE_NODES: usize = 40;

/// The root and its three cached children.
fn panes(view: &Entity<Chat>) -> (Entity<Rooms>, Entity<Timeline>, Entity<Timeline>) {
    view.read(|chat| {
        (
            chat.rooms().clone(),
            chat.timeline(Pane::Timeline).clone(),
            chat.timeline(Pane::Thread).clone(),
        )
    })
}

/// A character typed into the composer renders the root alone: the rooms
/// and the timeline stand in, the frame is one `Props` on the field, and
/// the root lowers no more than [`KEYSTROKE_NODES`].
#[test]
fn a_keystroke_lowers_the_root_and_the_composer_alone() {
    let (mut cx, view) = opened();
    let (rooms, timeline, _) = panes(&view);
    let before = (cx.lowered(&rooms), cx.lowered(&timeline));
    cx.simulate_input("draft-general/editor", "a");
    assert_eq!((cx.lowered(&rooms), cx.lowered(&timeline)), before);
    let report = &cx.reports()[0];
    assert!(report.rendered, "{report:?}");
    assert!(
        report.lowered <= KEYSTROKE_NODES,
        "the root lowered {} nodes, over the pin {KEYSTROKE_NODES}",
        report.lowered
    );
    assert_eq!(report.patches, 1, "{report:?}");
    assert!(
        matches!(
            &cx.last_frame().patches[..],
            [wire::Patch::Props {
                node: wire::Node::Field { .. },
                ..
            }]
        ),
        "{:#?}",
        cx.last_frame().patches
    );
}

/// A row landing in the open room renders the timeline, not the rooms (no
/// room row changed); a row landing in another room renders the rooms,
/// whose unread dot flips, not the timeline.
#[test]
fn a_landing_row_lowers_the_timeline_not_the_rooms() {
    let (mut cx, view) = opened();
    let (rooms, timeline, _) = panes(&view);
    let before = (cx.lowered(&rooms), cx.lowered(&timeline));
    cx.update(&view, |chat, _, cx| {
        let room = chat.room.as_mut().unwrap();
        room.messages
            .ready_mut()
            .unwrap()
            .push(row(3, 8, "a new one"));
        cx.notify();
    });
    cx.run_until_parked();
    assert!(cx.has_text("a new one"));
    assert_eq!(
        (cx.lowered(&rooms), cx.lowered(&timeline)),
        (before.0, before.1 + 1)
    );
    cx.update(&view, |chat, _, cx| {
        let dm = chat
            .channels
            .ready_mut()
            .unwrap()
            .iter_mut()
            .find(|info| info.channel.id == "dm-7-8")
            .unwrap();
        // the cursor was seated at the head when the room was listed
        dm.head_seq += 1;
        cx.notify();
    });
    cx.run_until_parked();
    assert!(
        cx.find("chat-sidebar-dm-8-unread").is_some(),
        "{:?}",
        cx.texts()
    );
    assert_eq!(
        (cx.lowered(&rooms), cx.lowered(&timeline)),
        (before.0 + 1, before.1 + 1)
    );
}

/// Typing in the search field renders the root alone.
#[test]
fn search_typing_keeps_the_rooms_and_the_timeline() {
    let (mut cx, view) = opened();
    let (rooms, timeline, _) = panes(&view);
    let before = (cx.lowered(&rooms), cx.lowered(&timeline));
    cx.simulate_input("chat-sidebar-search", "h");
    assert!(cx.reports()[0].rendered);
    assert_eq!((cx.lowered(&rooms), cx.lowered(&timeline)), before);
}

/// The sidebar's divider moves `layout.sidebar`, the root's: a `Props` on
/// `chat-sidebar`, nothing under the rooms box, the rooms not rendered.
#[test]
fn a_sidebar_drag_keeps_the_rooms() {
    let (mut cx, view) = opened();
    let (rooms, timeline, _) = panes(&view);
    let before = (cx.lowered(&rooms), cx.lowered(&timeline));
    cx.simulate_drag("chat-sidebar-resize", 18., 0.);
    assert_eq!((cx.lowered(&rooms), cx.lowered(&timeline)), before);
    let patches = &cx.last_frame().patches;
    // the one box that moved; the untouched composer's field also crosses,
    // its empty draft being made afresh by every render of the root
    let boxes: Vec<_> = patches
        .iter()
        .filter_map(|patch| match patch {
            wire::Patch::Props {
                node: wire::Node::Container(wire::ContainerNode { id, .. }),
                ..
            } => Some(id.clone()),
            wire::Patch::Props { .. } => None,
            other => panic!("not a props patch: {other:#?}"),
        })
        .collect();
    assert_eq!(
        boxes,
        [Some(wire::ElementIdWire::Name("chat-sidebar".into()))],
        "{patches:#?}"
    );
}

/// `hovered` is one field per pane: the pointer over a room row shows its
/// strip, over a thread row the thread's; a row's strip goes when the host
/// says the pointer left it, and each hover renders its own pane alone.
#[test]
fn a_hover_crosses_the_panes() {
    let (mut cx, view) = opened();
    cx.update(&view, |chat, _, cx| {
        let room = chat.room.as_mut().unwrap();
        room.messages.ready_mut().unwrap()[0].reply_count = 1;
        room.thread = Some(Thread {
            root: 1,
            replies: Loadable::Ready(vec![MsgRow {
                thread: Some(1),
                ..row(3, 8, "a reply")
            }]),
            ..Thread::default()
        });
        cx.notify();
    });
    cx.run_until_parked();
    let (rooms, timeline, thread) = panes(&view);
    let before = [
        cx.lowered(&rooms),
        cx.lowered(&timeline),
        cx.lowered(&thread),
    ];
    cx.simulate_hover("chat-message-m2-row", true);
    assert!(
        cx.find("chat-message-m2-more").is_some(),
        "the room row's strip"
    );
    assert_eq!(
        [
            cx.lowered(&rooms),
            cx.lowered(&timeline),
            cx.lowered(&thread)
        ],
        [before[0], before[1] + 1, before[2]]
    );
    cx.simulate_hover("chat-message-m3-row", true);
    assert!(
        cx.find("chat-message-m3-more").is_some(),
        "the thread row's strip"
    );
    assert!(
        cx.find("chat-message-m2-more").is_some(),
        "the room row's strip stays until the host says the pointer left it"
    );
    cx.simulate_hover("chat-message-m2-row", false);
    assert!(cx.find("chat-message-m2-more").is_none());
    assert!(cx.find("chat-message-m3-more").is_some());
    assert_eq!(
        [
            cx.lowered(&rooms),
            cx.lowered(&timeline),
            cx.lowered(&thread)
        ],
        [before[0], before[1] + 2, before[2] + 1]
    );
}
