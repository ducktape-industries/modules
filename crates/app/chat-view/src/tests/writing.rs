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
    let (restored, view) = restored(&cx);
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

/// What the composer of the draft `key` holds: the frame's text, which the
/// host adopted or the view followed.
fn held(cx: &TestAppContext, key: &str) -> String {
    let Some(wire::Node::Field { value, .. }) = cx.find(&format!("{key}/editor")) else {
        panic!("no composer for {key}");
    };
    value.clone()
}

/// "a" then "b" as two keys, each at the end of what the field holds: what
/// the field shows after them, and what Send would send.
fn a_then_b(cx: &mut TestAppContext, view: &Entity<Chat>, key: &str) -> (String, String) {
    for typed in ["a", "b"] {
        let text = format!("{}{typed}", held(cx, key));
        cx.simulate_input(&format!("{key}/editor"), &text);
    }
    let body = view.read(|chat| chat.drafts.get(key).map(|draft| draft.body()));
    (held(cx, key), body.unwrap_or_default())
}

/// The first thing typed into a room nobody wrote in yet stays: the draft
/// the frame drew is the one that hears the change.
#[test]
fn the_first_key_in_a_room_is_kept() {
    let (mut cx, view) = opened();
    assert_eq!(held(&cx, "draft-general"), "", "nothing written here yet");
    let (shown, sent) = a_then_b(&mut cx, &view, "draft-general");
    assert_eq!((shown.as_str(), sent.as_str()), ("ab", "ab"));
}

#[test]
fn the_first_key_in_a_thread_reply_is_kept() {
    let (mut cx, view) = opened();
    thread(&mut cx, &view, 1);
    let (shown, sent) = a_then_b(&mut cx, &view, "draft-general-1");
    assert_eq!((shown.as_str(), sent.as_str()), ("ab", "ab"));
}

#[test]
fn a_key_in_the_edit_field_is_kept() {
    let (mut cx, view) = opened();
    edit(&mut cx, &view, 1);
    let (shown, sent) = a_then_b(&mut cx, &view, "edit-general-1");
    assert_eq!((shown.as_str(), sent.as_str()), ("helloab", "helloab"));
}

#[test]
fn a_key_after_a_saved_draft_is_kept() {
    let (mut cx, view) = opened();
    cx.update(&view, |chat, _, cx| {
        cx.notify();
        chat.drafts.insert(
            "draft-general".into(),
            crate::composer::Draft::from_body("x", &[]),
        );
    });
    cx.run_until_parked();
    let (shown, sent) = a_then_b(&mut cx, &view, "draft-general");
    assert_eq!((shown.as_str(), sent.as_str()), ("xab", "xab"));
}

/// A room typed in before keeps every key typed in it again, and after a
/// restore with its draft.
#[test]
fn a_room_typed_in_again_and_after_a_restore_keeps_every_key() {
    let (mut cx, view) = opened();
    let (first, _) = a_then_b(&mut cx, &view, "draft-general");
    let (shown, sent) = a_then_b(&mut cx, &view, "draft-general");
    assert_eq!(
        (&shown, &sent),
        (&format!("{first}ab"), &format!("{first}ab"))
    );
    let (mut restored, view) = restored(&cx);
    let (shown, sent) = a_then_b(&mut restored, &view, "draft-general");
    assert_eq!(
        (&shown, &sent),
        (&format!("{first}abab"), &format!("{first}abab")),
        "restored with its draft"
    );
}

#[test]
fn the_first_key_after_a_restore_of_a_room_never_typed_in_is_kept() {
    let (cx, _) = opened();
    let (mut restored, view) = restored(&cx);
    let (shown, sent) = a_then_b(&mut restored, &view, "draft-general");
    assert_eq!((shown.as_str(), sent.as_str()), ("ab", "ab"));
}

/// `text` typed into the composer of the draft `key` in one change: the
/// field shows it and Send would send it.
fn types(cx: &mut TestAppContext, view: &Entity<Chat>, key: &str, text: &str) {
    cx.simulate_input(&format!("{key}/editor"), text);
    let body = view.read(|chat| chat.drafts.get(key).map(|draft| draft.body()));
    assert_eq!(
        (held(cx, key).as_str(), body.as_deref()),
        (text, Some(text)),
        "typed into {key}"
    );
}

/// Every way a room comes on screen seats its draft before the frame that
/// draws its composer: a link the view was opened with, a press in the
/// sidebar, a direct room nobody posted in yet, and a search hit.
#[test]
fn the_first_key_is_kept_however_the_room_came_on_screen() {
    let mut cx = TestAppContext::new();
    configure(&mut cx);
    let routes = cx
        .host()
        .stream::<ducktape_view_guest::methods::HostRoute>();
    let view = cx.open::<Chat>();
    seat(&mut cx);
    let link = crate::links::channel_link("testnet#0a1b2c3d", "forge:web:3", None).unwrap();
    routes.send(ducklink::Link::parse(&link).unwrap().tail.join("/"));
    cx.run_until_parked();
    // five letters in one change, then a key
    types(&mut cx, &view, "draft-forge:web:3", "hello");
    types(&mut cx, &view, "draft-forge:web:3", "hello!");

    cx.simulate_click("chat-sidebar-channel-general");
    cx.run_until_parked();
    types(&mut cx, &view, "draft-general", "a");

    cx.update(&view, |chat, _, cx| {
        cx.notify();
        chat.choose("dm-7-9".into(), cx)
    });
    cx.run_until_parked();
    view.read(|chat| assert!(chat.info("dm-7-9").is_none(), "not opened yet"));
    types(&mut cx, &view, "draft-dm-7-9", "a");

    cx.simulate_input("chat-sidebar-search", "hello");
    cx.simulate_submit("chat-sidebar-search");
    cx.run_until_parked();
    // the window around the hit is asked and not answered
    cx.host().never::<Ask<::chat::Chat>>();
    cx.simulate_click("chat-search-hit-dm-7-8-1");
    view.read(|chat| assert_eq!(chat.room.as_ref().unwrap().id, "dm-7-8"));
    types(&mut cx, &view, "draft-dm-7-8", "a");
}

/// A view restored with a thread open and nothing typed yet.
#[test]
fn the_first_key_is_kept_in_a_thread_restored_open() {
    let (mut cx, view) = opened();
    thread(&mut cx, &view, 1);
    let (mut restored, view) = restored(&cx);
    types(&mut restored, &view, "draft-general-1", "a");
    types(&mut restored, &view, "draft-general", "a");
}

/// A composer is one document for as long as it is on screen: the host
/// takes a field's text from the frame whenever its generation moves, so a
/// render that moved it would take back what was typed since.
#[test]
fn a_render_leaves_the_composer_the_document_it_was() {
    let (mut cx, view) = opened();
    let generation = |cx: &TestAppContext| {
        let Some(wire::Node::Field { generation, .. }) = cx.find("draft-general/editor") else {
            panic!("no composer");
        };
        *generation
    };
    let drawn = generation(&cx);
    cx.update(&view, |_, _, cx| cx.notify());
    cx.run_until_parked();
    assert_eq!(generation(&cx), drawn);
}

/// The drafts are saved with the view, so a room or a thread left with
/// nothing written leaves none behind, and one left with text keeps it.
#[test]
fn a_draft_left_blank_goes_and_one_with_text_stays() {
    let (mut cx, view) = opened();
    let kept =
        |view: &Entity<Chat>| view.read(|chat| chat.drafts.keys().cloned().collect::<Vec<_>>());
    let choose = |cx: &mut TestAppContext, room: &str| {
        cx.update(&view, |chat, _, cx| {
            cx.notify();
            chat.choose(room.into(), cx)
        });
        cx.run_until_parked();
    };
    assert_eq!(kept(&view), ["draft-general"]);
    choose(&mut cx, "dm-7-8");
    assert_eq!(kept(&view), ["draft-dm-7-8"], "general was left blank");
    types(&mut cx, &view, "draft-dm-7-8", "half a thought");
    choose(&mut cx, "general");
    assert_eq!(kept(&view), ["draft-dm-7-8", "draft-general"]);
    thread(&mut cx, &view, 1);
    assert!(kept(&view).contains(&"draft-general-1".to_owned()));
    cx.simulate_click("chat-thread-close");
    assert_eq!(kept(&view), ["draft-dm-7-8", "draft-general"]);
    choose(&mut cx, "dm-7-8");
    assert_eq!(kept(&view), ["draft-dm-7-8"]);
    assert_eq!(held(&cx, "draft-dm-7-8"), "half a thought");
}

/// An edit emptied to write it again is still on screen: its draft stays
/// when the room around it is entered again.
#[test]
fn an_edit_emptied_keeps_its_draft_while_it_is_open() {
    let (mut cx, view) = opened();
    edit(&mut cx, &view, 1);
    types(&mut cx, &view, "edit-general-1", "");
    cx.update(&view, |chat, _, cx| {
        cx.notify();
        chat.open("general".into(), cx);
    });
    cx.run_until_parked();
    types(&mut cx, &view, "edit-general-1", "again");
}

/// An edit left without saving leaves no draft behind: opening it again
/// starts from the message, so there is nothing of it to keep.
#[test]
fn an_edit_cancelled_leaves_no_draft_and_opens_from_the_message_again() {
    let (mut cx, view) = opened();
    edit(&mut cx, &view, 1);
    types(&mut cx, &view, "edit-general-1", "hello there");
    cx.simulate_click("Cancel");
    view.read(|chat| assert!(!chat.drafts.contains_key("edit-general-1")));
    edit(&mut cx, &view, 1);
    assert_eq!(held(&cx, "edit-general-1"), "hello", "seeded again");
    // one that holds a send stays: the send is not in the message
    cx.update(&view, |chat, _, cx| {
        cx.notify();
        let failed_send = Some(crate::composer::Send { body: "x".into() });
        let parked = crate::composer::Draft {
            failed_send,
            ..Default::default()
        };
        chat.drafts.insert("edit-general-2".into(), parked);
    });
    cx.run_until_parked();
    view.read(|chat| assert!(chat.drafts.contains_key("edit-general-2")));
}
