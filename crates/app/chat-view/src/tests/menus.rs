//! Message menus, the reaction picker and the edit field.
use super::*;

#[test]
fn menus_and_dialogs_are_modal_overlays_with_dismiss_routes() {
    let (mut cx, view) = opened();
    message::hover(&mut cx, &view, 1);
    cx.simulate_click("chat-message-m1-more");
    assert!(matches!(
        cx.find("chat-menu-overlay"),
        Some(wire::Node::Overlay {
            label: Some(label),
            on_dismiss: Some(_),
            children,
            ..
        }) if label == "Message menu" && children.len() == 2
    ));
    // a Menu of MenuItems, named
    let role = |cx: &TestAppContext, key: &str| match cx.find(key) {
        Some(wire::Node::Container(node)) => node.interactivity.role,
        _ => panic!("{key} is a container"),
    };
    assert_eq!(
        role(&cx, "chat-room-message-action-focus"),
        Some(ducktape_view_guest::Role::Menu)
    );
    assert_eq!(
        role(&cx, "chat-menu-reply"),
        Some(ducktape_view_guest::Role::MenuItem)
    );

    cx.simulate_dismiss("chat-menu-overlay");
    view.read(|chat| assert!(chat.menu.is_none()));
    cx.simulate_click("chat-sidebar-new-channel");
    assert!(matches!(
        cx.find("chat-create-overlay"),
        Some(wire::Node::Overlay {
            label: Some(label),
            on_dismiss: Some(_),
            children,
            ..
        }) if label == "Create channel" && children.len() == 2
    ));
    cx.simulate_dismiss("chat-create-overlay");
    view.read(|chat| assert!(chat.create.is_none()));
}

#[test]
fn message_menu_offers_only_what_the_reader_may_do_and_executes_it() {
    let (mut cx, view) = opened();
    let menu_on = |seq| Menu {
        pane: Pane::Timeline,
        seq,
        rev: 0,
        mode: Mode::More,
        at: (611., 455.),
    };
    view.update(&mut cx, |chat, _, cx| {
        chat.menu = Some(menu_on(1));
        chat.session.account = None;
        cx.notify();
    });
    cx.run_until_parked();
    // a key with no account reads: nothing it would be refused is offered
    for id in [
        "chat-menu-add-reaction",
        "chat-menu-edit",
        "chat-menu-delete",
    ] {
        assert!(cx.find(id).is_none(), "{id} offered to a reader");
    }
    assert!(cx.find("chat-menu-copy-link").is_some());

    // someone else's message in a channel the reader owns: delete, no edit
    view.update(&mut cx, |chat, _, cx| {
        chat.menu = Some(menu_on(2));
        chat.session.account = Some(7);
        cx.notify();
    });
    cx.run_until_parked();
    assert!(cx.find("chat-menu-edit").is_none(), "only the author edits");
    assert!(cx.find("chat-menu-delete").is_some(), "the owner deletes");

    view.update(&mut cx, |chat, _, cx| {
        chat.menu = Some(menu_on(1));
        cx.notify();
    });
    cx.run_until_parked();
    assert!(cx.has_text("😀") && cx.has_text("✎") && cx.has_text("🗑"));
    view.update(&mut cx, |chat, _, cx| {
        chat.session.account = Some(7);
        cx.notify();
    });
    cx.run_until_parked();
    cx.simulate_click("chat-menu-delete");
    assert!(cx.has_text("Delete this message?"));
    // Anchored near the row it opened from, this popup can overlap the
    // message card beneath it; without occlude, a click on "Delete" here
    // also fires the card's row-select handler, which resets `chat.menu`
    // to `Mode::Toolbar` before `delete_armed` reads it, so the delete is
    // silently dropped (no submit, no error).
    let Some(wire::Node::Container(ducktape_view_guest::wire::ContainerNode {
        interactivity, ..
    })) = cx.find(&ui::menu::focus_key(Pane::Timeline, Mode::Delete))
    else {
        panic!("delete confirmation frame")
    };
    assert!(
        interactivity.occlude,
        "delete confirmation popup must occlude so its clicks don't also fire the row beneath"
    );
    cx.simulate_click("chat-menu-confirm-delete");
    cx.run_until_parked();
    assert!(cx.host().requests::<Submit<ChatApi>>().iter().any(|op| {
        matches!(op, Op::DeleteMessage { channel_id, seq: 1 } if channel_id == "general")
    }));
}

#[test]
fn reaction_picker_keeps_labels_and_its_stable_action_id() {
    let (mut cx, view) = opened();
    view.update(&mut cx, |chat, _, cx| {
        chat.menu = Some(Menu {
            pane: Pane::Timeline,
            seq: 1,
            rev: 0,
            mode: Mode::Reactions,
            at: (333., 222.),
        });
        cx.notify();
    });
    cx.run_until_parked();
    let Some(wire::Node::Container(ducktape_view_guest::wire::ContainerNode {
        interactivity, ..
    })) = cx.find("chat-reaction-🔥")
    else {
        panic!("reaction is a native cell");
    };
    // each cell is named by its emoji: 48 "Add reaction"s told nothing apart
    assert_eq!(interactivity.aria.label.as_deref(), Some("React with 🔥"));
    assert_eq!(interactivity.aria.description, None);
    assert_eq!(
        cx.interactivity("chat-reaction-Smileys-😀")
            .aria
            .label
            .as_deref(),
        Some("React with 😀")
    );
    cx.simulate_click("chat-reaction-🔥");
    cx.run_until_parked();
    assert!(
        cx.host()
            .requests::<Submit<ChatApi>>()
            .iter()
            .any(|op| { matches!(op, Op::AddReaction { emoji, .. } if emoji == "🔥") })
    );
}

/// A search shows every match, past one tab's worth, in a scrolled grid.
#[test]
fn an_emoji_search_shows_every_match() {
    let (mut cx, view) = opened();
    let found = crate::emoji::search("c");
    assert!(
        found.len() > crate::emoji::PER_TAB,
        "a query that overflows a tab"
    );
    view.update(&mut cx, |chat, _, cx| {
        chat.menu = Some(Menu {
            pane: Pane::Timeline,
            seq: 1,
            rev: 0,
            mode: Mode::Reactions,
            at: (333., 222.),
        });
        chat.picker.query = "c".into();
        cx.notify();
    });
    cx.run_until_parked();
    assert!(cx.has_text(&format!("{} MATCHES", found.len())));
    assert!(cx.find("chat-reaction-results-scroll").is_some());
    for emoji in found {
        assert!(
            cx.find(&format!("chat-reaction-{emoji}")).is_some(),
            "{emoji}"
        );
    }
}

/// The field that edits a message commits with "Save"; a new one sends.
#[test]
fn the_edit_field_saves() {
    let (mut cx, view) = opened();
    assert!(cx.has_text("Send") && !cx.has_text("Save"));
    view.update(&mut cx, |chat, window, cx| {
        cx.notify();
        chat.open_menu(Pane::Timeline, 1, 0, Mode::Editing, window, cx);
    });
    cx.run_until_parked();
    assert!(cx.find("chat-message-editing").is_some());
    assert!(cx.has_text("Save"));
    // Cancel sits on Save's row, and leaves the edit
    let key = crate::composer::Target::Edit {
        channel: view.read(|chat| chat.room_id()),
        seq: 1,
        base_rev: 0,
    }
    .key();
    cx.simulate_click(&format!("{key}/cancel"));
    assert!(cx.find("chat-message-editing").is_none());
}

/// The message menu is one stop: it takes the keys on open with its first
/// item active, ↓ moves, Enter runs the item; its dialogs are no stops.
#[test]
fn the_message_menu_walks_its_items_and_enter_runs_one() {
    let (mut cx, view) = opened();
    message::hover(&mut cx, &view, 1);
    cx.simulate_click("chat-message-m1-more");
    let menu = cx.interactivity("chat-room-message-action-focus");
    assert_eq!(menu.role, Some(ducktape_view_guest::Role::Menu));
    assert!(menu.focusable && menu.tab_stop == Some(true));
    assert!(
        cx.host()
            .requests::<ducktape_view_guest::methods::HostWidget>()
            .iter()
            .any(|command| matches!(
                command,
                wire::WidgetCommand::Focus { target }
                    if target == &[wire::ElementIdWire::Name("chat-room-message-action-focus".into())]
            )),
        "the menu takes the keys on open"
    );
    let reply = cx.interactivity("chat-menu-reply");
    assert_eq!(reply.role, Some(ducktape_view_guest::Role::MenuItem));
    assert!(!reply.focusable && reply.aria.active_descendant);
    cx.simulate_key_down("chat-room-message-action-focus", "down");
    assert!(
        cx.interactivity("chat-menu-add-reaction")
            .aria
            .active_descendant
    );
    assert!(!cx.interactivity("chat-menu-reply").aria.active_descendant);
    cx.simulate_key_down("chat-room-message-action-focus", "enter");
    view.read(|chat| {
        assert_eq!(
            chat.menu.as_ref().map(|menu| menu.mode),
            Some(Mode::Reactions)
        );
    });
    // the picker's search field takes the keys on open; its dialog frame
    // offers no focus (a focus offered that no Tab reaches is AX-021)
    assert!(
        cx.host()
            .requests::<ducktape_view_guest::methods::HostWidget>()
            .iter()
            .any(|command| matches!(
                command,
                wire::WidgetCommand::Focus { target }
                    if target == &[wire::ElementIdWire::Name("chat-room-message-reaction-focus".into())]
            )),
        "the search field takes the keys on open"
    );
    let frame = cx.interactivity("chat-room-message-reaction-frame");
    assert!(!frame.focusable, "the frame offers no focus");
    // the host's modal layer is the picker's dialog: the frame is none
    // (an inner Dialog the wire cannot mark modal is AX-103)
    assert_eq!(frame.role, None, "the frame is no dialog of its own");
    assert!(matches!(
        cx.find("chat-menu-overlay"),
        Some(wire::Node::Overlay { label: Some(label), .. }) if label == "Add reaction"
    ));
}
