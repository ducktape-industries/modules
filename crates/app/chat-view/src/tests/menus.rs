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
            ..
        }) if label == "Message menu"
    ));

    cx.simulate_dismiss("chat-menu-overlay");
    view.read(|chat| assert!(chat.menu.is_none()));
    cx.simulate_click("chat-sidebar-new-channel");
    assert!(matches!(
        cx.find("chat-create-overlay"),
        Some(wire::Node::Overlay {
            label: Some(label),
            on_dismiss: Some(_),
            ..
        }) if label == "Create channel"
    ));
    cx.simulate_dismiss("chat-create-overlay");
    view.read(|chat| assert!(chat.create.is_none()));
}

/// A chosen message (its strip stays open) is no dialog: no overlay opens
/// for it, so the screen itself lets it go on Escape from anywhere in the
/// view, on a press anywhere in it (a press on a message chooses that one
/// after), and when the arrows move on. With shift, a press grows the
/// copy range from the chosen message instead, which lets go the same way.
#[test]
fn a_chosen_message_lets_go_on_escape_a_press_elsewhere_or_an_arrow() {
    let (mut cx, view) = opened();
    let chosen = |view: &Entity<Chat>| {
        view.read(|chat| chat.menu.as_ref().map(|menu| (menu.seq, menu.mode)))
    };
    cx.simulate_click("chat-message-m1");
    assert_eq!(chosen(&view), Some((1, Mode::Toolbar)));
    assert!(cx.find("chat-message-m1-actions").is_some());
    assert!(
        !matches!(
            cx.find("chat-menu-overlay"),
            Some(wire::Node::Overlay { .. })
        ),
        "a chosen message opens no dialog"
    );
    // Escape, with the keys in the sidebar's search field
    let root = cx.interactivity("chat-root");
    assert!(root.on_key_down.is_some() && root.consumes_keys.contains(&"escape".into()));
    cx.simulate_focus("chat-sidebar-search");
    cx.simulate_key_down("chat-sidebar-search", "escape");
    assert_eq!(chosen(&view), None);
    assert!(cx.find("chat-message-m1-actions").is_none());
    assert!(
        cx.interactivity("chat-root").consumes_keys.is_empty(),
        "with nothing to let go, Escape is the host's"
    );
    // a press anywhere else
    cx.simulate_click("chat-message-m1");
    assert_eq!(chosen(&view), Some((1, Mode::Toolbar)));
    let down = |shift| ducktape_view_guest::MouseDownEvent {
        button: ducktape_view_guest::MouseButton::Left,
        position: Default::default(),
        modifiers: ducktape_view_guest::Modifiers {
            shift,
            ..Default::default()
        },
        click_count: 1,
        first_mouse: false,
    };
    cx.simulate_mouse_down("chat-room-header", down(false));
    assert_eq!(chosen(&view), None);
    // a press on another message chooses it (the press comes down first)
    cx.simulate_click("chat-message-m1");
    cx.simulate_mouse_down("chat-message-m2", down(false));
    cx.simulate_click("chat-message-m2");
    assert_eq!(chosen(&view), Some((2, Mode::Toolbar)));
    // the arrows leave it behind
    cx.simulate_focus("chat-message-list");
    cx.simulate_key_down("chat-message-list", "up");
    assert_eq!(chosen(&view), None);
    // shift: the range grows from the chosen message to the pressed one
    cx.simulate_click("chat-message-m1");
    cx.simulate_mouse_down("chat-message-m2", down(true));
    assert_eq!(
        chosen(&view),
        Some((1, Mode::Toolbar)),
        "a shift press keeps it for the range"
    );
    let handler = cx.interactivity("chat-message-m2").on_click.unwrap();
    let button = |shift| wire::click::ButtonEvent {
        button: wire::click::MouseButton::Left,
        position: Default::default(),
        modifiers: ducktape_view_guest::Modifiers {
            shift,
            ..Default::default()
        },
        click_count: 1,
    };
    cx.simulate_event(wire::Event::Click {
        handler,
        event: wire::click::Click::Mouse {
            down: button(true),
            up: button(true),
            first_mouse: false,
        },
    });
    cx.run_until_parked();
    assert_eq!(chosen(&view), None);
    view.read(|chat| {
        let range = chat.copy.expect("the copy range");
        assert_eq!((range.anchor, range.head), (1, 2));
    });
    assert!(cx.has_text("2 messages selected"));
    cx.simulate_key_down("chat-message-list", "escape");
    view.read(|chat| assert!(chat.copy.is_none()));
    assert!(!cx.has_text("2 messages selected"));
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
    cx.update(&view, |chat, _, cx| {
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
    cx.update(&view, |chat, _, cx| {
        chat.menu = Some(menu_on(2));
        chat.session.account = Some(7);
        cx.notify();
    });
    cx.run_until_parked();
    assert!(cx.find("chat-menu-edit").is_none(), "only the author edits");
    assert!(cx.find("chat-menu-delete").is_some(), "the owner deletes");

    cx.update(&view, |chat, _, cx| {
        chat.menu = Some(menu_on(1));
        cx.notify();
    });
    cx.run_until_parked();
    assert!(cx.has_text("😀") && cx.has_text("✎") && cx.has_text("🗑"));
    cx.update(&view, |chat, _, cx| {
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
        interactivity: Some(interactivity),
        ..
    })) = cx.find("chat-room-message-delete-frame")
    else {
        panic!("delete confirmation frame")
    };
    assert!(
        interactivity.occlude,
        "delete confirmation popup must occlude so its clicks don't also fire the row beneath"
    );
    cx.simulate_click("chat-menu-confirm-delete");
    cx.run_until_parked();
    assert!(
        cx.host()
            .requests::<Submit<::chat::Chat>>()
            .iter()
            .any(|op| {
                matches!(op, Op::DeleteMessage { channel_id, seq: 1 } if channel_id == "general")
            })
    );
}

#[test]
fn delete_confirmation_opens_with_the_keys_on_its_first_button() {
    let (mut cx, view) = opened();
    cx.update(&view, |chat, _, cx| {
        chat.menu = Some(Menu {
            pane: Pane::Timeline,
            seq: 1,
            rev: 0,
            mode: Mode::More,
            at: (611., 455.),
        });
        // the channel's owner deletes
        chat.session.account = Some(7);
        cx.notify();
    });
    cx.run_until_parked();
    cx.simulate_click("chat-menu-delete");
    // Cancel, the first button, takes the keys on open: a stray Enter
    // keeps the message
    assert!(
        cx.host()
            .requests::<ducktape_view_guest::methods::HostWidget>()
            .iter()
            .any(|command| matches!(
                command,
                wire::WidgetCommand::Focus { target }
                    if target.last() == Some(&wire::ElementIdWire::Name("chat-menu-cancel-delete".into()))
            )),
        "Cancel takes the keys on open"
    );
    let cancel = cx.interactivity("chat-menu-cancel-delete");
    assert!(
        cancel.focusable && cancel.tab_stop != Some(false),
        "Cancel is a stop"
    );
    // the frame offers no focus: one it offered and no Tab reached was
    // the door's AX-021 on it
    let dialog = cx.interactivity("chat-room-message-delete-frame");
    assert_eq!(dialog.role, Some(ducktape_view_guest::Role::AlertDialog));
    assert_eq!(dialog.aria.label.as_deref(), Some("Delete this message?"));
    assert!(!dialog.focusable, "the frame offers no focus");
}

#[test]
fn reaction_picker_keeps_labels_and_its_stable_action_id() {
    let (mut cx, view) = opened();
    cx.update(&view, |chat, _, cx| {
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
        interactivity: Some(interactivity),
        ..
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
            .requests::<Submit<::chat::Chat>>()
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
    cx.update(&view, |chat, _, cx| {
        chat.menu = Some(Menu {
            pane: Pane::Timeline,
            seq: 1,
            rev: 0,
            mode: Mode::Reactions,
            at: (333., 222.),
        });
        chat.picker.query.reset("c");
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
    edit(&mut cx, &view, 1);
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
                    if target.last() == Some(&wire::ElementIdWire::Name("chat-room-message-action-focus".into()))
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
                    if target.last() == Some(&wire::ElementIdWire::Name("chat-room-message-reaction-focus".into()))
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
