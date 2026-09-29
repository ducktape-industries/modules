use super::*;
use ducktape_view_guest::{StyleRefinement, Styled};

#[test]
fn action_strip_keeps_the_rows_hover_and_is_not_inside_selection_target() {
    let (mut cx, view) = opened();
    assert!(
        cx.find("chat-message-m1-actions").is_none(),
        "no strip on a row the pointer is not over"
    );
    hover(&mut cx, &view, 1);
    let Some(wire::Node::Container(ducktape_view_guest::wire::ContainerNode {
        style,
        interactivity,
        ..
    })) = cx.find("chat-message-m1-actions")
    else {
        panic!("action strip")
    };
    assert_eq!(
        style.visibility,
        StyleRefinement::default().invisible().visibility
    );
    assert_eq!(
        interactivity.group_hover.as_ref().unwrap().style.visibility,
        StyleRefinement::default().visible().visibility
    );
    // An occluding strip took the row's group hover away as the pointer
    // reached it: it hid itself and could not be clicked. It stays under
    // the group; its buttons claim their click from the card instead.
    assert!(
        !interactivity.occlude,
        "the strip must not take the row's hover from under the pointer"
    );
    let card = cx.find("chat-message-m1").unwrap();
    fn has_actions(node: &wire::Node) -> bool {
        node.key() == Some("chat-message-m1-actions") || node.children().iter().any(has_actions)
    }
    assert!(
        !has_actions(card),
        "native action clicks must not bubble through selection"
    );
    assert!(cx.find("chat-message-m1-thumbs-up").is_some());
}

/// The pointer over message `seq`'s row, as the host reports it.
pub(super) fn hover(cx: &mut TestAppContext, view: &Entity<Chat>, seq: u64) {
    view.update(cx, |chat, _, cx| {
        chat.hovered = Some((Pane::Timeline, seq));
        cx.notify();
    });
    cx.run_until_parked();
}

#[test]
fn copy_range_keeps_its_distinct_message_plate() {
    let (mut cx, view) = opened();
    view.update(&mut cx, |chat, _, cx| {
        chat.copy = Some(CopyRange {
            pane: Pane::Timeline,
            anchor: 1,
            head: 2,
        });
        cx.notify();
    });
    cx.run_until_parked();
    let Some(wire::Node::Container(ducktape_view_guest::wire::ContainerNode { style, .. })) =
        cx.find("chat-message-m1")
    else {
        panic!("message card")
    };
    assert_eq!(
        style
            .background
            .as_ref()
            .and_then(|fill| fill.color())
            .and_then(|color| color.as_solid()),
        Some(ducktape_view_guest::Theme::light().surface_raised)
    );
}

#[test]
fn reaction_rows_keep_add_action_and_selected_accessibility() {
    let (mut cx, view) = opened();
    view.update(&mut cx, |chat, _, cx| {
        chat.room.as_mut().unwrap().messages.ready_mut().unwrap()[0]
            .reactions
            .push(chat::Reaction {
                emoji: "🔥".into(),
                count: 2,
                reacted_by_me: true,
            });
        cx.notify();
    });
    cx.run_until_parked();
    let Some(wire::Node::Container(ducktape_view_guest::wire::ContainerNode {
        interactivity, ..
    })) = cx.find("chat-message-m1-reaction-🔥")
    else {
        panic!("reaction pill")
    };
    assert_eq!(interactivity.aria.description.as_deref(), Some("2"));
    assert!(interactivity.aria.toggled.is_some());
    assert!(cx.find("chat-message-m1-reaction-add").is_some());
    cx.simulate_click("chat-message-m1-reaction-add");
    view.read(|chat| {
        assert!(
            chat.menu
                .as_ref()
                .is_some_and(|menu| menu.mode == Mode::Reactions && menu.seq == 1)
        )
    });
}

/// A chip is named by its emoji, yours or not: the toggle says which, so
/// the name the door reads does not flip with it, and it is neither the `+`
/// picker's "Add reaction" nor the strip's "React with 👍", which always adds.
#[test]
fn a_reaction_chip_is_named_by_its_emoji_whether_toggled_or_not() {
    let (mut cx, view) = opened();
    view.update(&mut cx, |chat, _, cx| {
        let rows = chat.room.as_mut().unwrap().messages.ready_mut().unwrap();
        for (row, mine) in rows.iter_mut().zip([true, false]) {
            row.reactions.push(chat::Reaction {
                emoji: "👍".into(),
                count: 1,
                reacted_by_me: mine,
            });
        }
        cx.notify();
    });
    cx.run_until_parked();
    hover(&mut cx, &view, 1);
    let aria = |key: &str| match cx.find(key) {
        Some(wire::Node::Container(ducktape_view_guest::wire::ContainerNode {
            interactivity,
            ..
        })) => interactivity.aria.clone(),
        _ => panic!("{key} is a container"),
    };
    let (on, off) = (
        aria("chat-message-m1-reaction-👍"),
        aria("chat-message-m2-reaction-👍"),
    );
    assert_eq!(off.label, on.label, "one name, toggled or not");
    assert_eq!(on.label.as_deref(), Some("👍 reaction"));
    assert_eq!(on.toggled, Some(true.into()));
    assert_eq!(off.toggled, Some(false.into()));
    assert_eq!(
        aria("chat-message-m1-reaction-add").label.as_deref(),
        Some("Add reaction")
    );
    assert_eq!(
        aria("chat-message-m1-thumbs-up").label.as_deref(),
        Some("React with 👍")
    );
}

/// What a pointer presses on a message (its block link, a reaction chip,
/// the `+`) and the sidebar's "+ New channel" are at least 24 px each way:
/// the box's own floor, so the bounds the door's AX-017 reads cannot come
/// in under it, whatever the text inside.
#[test]
fn the_small_press_targets_are_at_least_24_px_each_way() {
    use ducktape_view_guest::px;
    let (mut cx, view) = opened();
    view.update(&mut cx, |chat, _, cx| {
        chat.room.as_mut().unwrap().messages.ready_mut().unwrap()[0]
            .reactions
            .push(chat::Reaction {
                emoji: "🔥".into(),
                count: 2,
                reacted_by_me: true,
            });
        cx.notify();
    });
    cx.run_until_parked();
    let style = |key: &str| match cx.find(key) {
        Some(wire::Node::Container(node)) => node.style.clone(),
        other => panic!("{key} is no container: {other:?}"),
    };
    for key in ["chat-message-m1-height", "chat-sidebar-new-channel"] {
        let style = style(key);
        assert_eq!(
            (style.min_size.width, style.min_size.height),
            (Some(px(24.).into()), Some(px(24.).into())),
            "{key}"
        );
    }
    // a chip is as tall as the thread button, and no narrower than tall
    for key in [
        "chat-message-m1-reaction-🔥",
        "chat-message-m1-reaction-add",
    ] {
        let style = style(key);
        assert_eq!(
            (style.min_size.width, style.size.height),
            (Some(px(24.).into()), Some(px(24.).into())),
            "{key}"
        );
    }
}

/// The hover strip's buttons, "Clear search" and the confirmation's
/// "Dismiss" ✕ are at least 24 px each way, like the message's own
/// controls above: the box's declared floor.
#[test]
fn the_strip_and_the_glyph_buttons_are_at_least_24_px_each_way() {
    use ducktape_view_guest::{design, px};
    let (mut cx, view) = opened();
    view.update(&mut cx, |chat, _, cx| {
        chat.search.draft = "hello".into();
        chat.confirmation = "Saved".into();
        chat.hovered = Some((Pane::Timeline, 1));
        cx.notify();
    });
    cx.run_until_parked();
    let style = |key: &str| match cx.find(key) {
        Some(wire::Node::Container(node)) => node.style.clone(),
        other => panic!("{key} is no container: {other:?}"),
    };
    // a strip button is 24 tall and as wide as a kit row, over 24
    assert!(design::size::ROW >= px(24.));
    for key in ["thread", "thumbs-up", "react", "more"] {
        let style = style(&format!("chat-message-m1-{key}"));
        assert_eq!(
            (style.size.width, style.size.height),
            (Some(design::size::ROW.into()), Some(px(24.).into())),
            "{key}"
        );
    }
    for key in [
        "chat-sidebar-clear-search",
        "chat-room-confirmation-dismiss",
    ] {
        let style = style(key);
        assert_eq!(
            (style.min_size.width, style.min_size.height),
            (Some(px(24.).into()), Some(px(24.).into())),
            "{key}"
        );
    }
}

#[test]
fn thread_root_uses_reply_count_as_a_separator() {
    let (mut cx, view) = opened();
    view.update(&mut cx, |chat, _, cx| {
        let room = chat.room.as_mut().unwrap();
        room.messages.ready_mut().unwrap()[0].reply_count = 2;
        room.thread = Some(Thread {
            root: 1,
            replies: Loadable::Ready(Vec::new()),
            ..Thread::default()
        });
        cx.notify();
    });
    cx.run_until_parked();
    assert!(cx.find("chat-message-m1-reply-separator").is_some());
    assert!(cx.has_text("2 replies"));
}

#[test]
fn a_control_on_a_card_keeps_its_click_from_the_card_beneath() {
    use ducktape_view_guest::{ClickEvent, Point, px};
    let pointer = |x: f32| {
        let button = wire::click::ButtonEvent {
            button: wire::click::MouseButton::Left,
            position: Point {
                x: px(x),
                y: px(9.),
            },
            modifiers: Default::default(),
            click_count: 1,
        };
        ClickEvent::from(wire::click::Click::Mouse {
            down: button.clone(),
            up: button,
            first_mouse: false,
        })
    };
    let mut chat = Chat::default();
    // GPUI hands one click to the control and then to the card under it
    chat.claim(&pointer(40.));
    assert!(chat.was_claimed((40., 9.)), "the card stands down");
    assert!(
        !chat.was_claimed((40., 9.)),
        "once: the next click is the card's"
    );
    chat.claim(&pointer(40.));
    assert!(
        !chat.was_claimed((41., 9.)),
        "a click elsewhere is the card's"
    );
    // a key press reaches only the focused control, never the card
    chat.claim(&ClickEvent::default());
    assert!(!chat.was_claimed((0., 0.)));
}

#[test]
fn replies_read_as_a_button() {
    let (mut cx, view) = opened();
    view.update(&mut cx, |chat, _, cx| {
        if let Some(Loadable::Ready(rows)) = chat.room.as_mut().map(|room| &mut room.messages) {
            rows[0].reply_count = 3;
        }
        cx.notify();
    });
    cx.run_until_parked();
    let Some(wire::Node::Container(ducktape_view_guest::wire::ContainerNode {
        style,
        interactivity,
        ..
    })) = cx.find("chat-message-m1-replies")
    else {
        panic!("replies button")
    };
    assert_eq!(interactivity.role, Some(ducktape_view_guest::Role::Button));
    // a cell of the timeline grid, not a stop of its own
    assert!(!interactivity.focusable && interactivity.hover.is_some());
    assert_eq!(
        style.mouse_cursor,
        Some(ducktape_view_guest::CursorStyle::PointingHand)
    );
    assert!(cx.has_text("Open thread →"));
    cx.simulate_click("chat-message-m1-replies");
    view.read(|chat| {
        assert_eq!(
            chat.room.as_ref().unwrap().thread.as_ref().map(|t| t.root),
            Some(1)
        );
    });
}

/// The emoji categories are one tab list: → opens the next category.
#[test]
fn an_arrow_on_the_emoji_tabs_opens_the_next_category() {
    let (mut cx, view) = opened();
    hover(&mut cx, &view, 1);
    cx.simulate_click("chat-message-m1-react");
    let tabs = cx.interactivity("chat-reaction-tabs");
    assert_eq!(tabs.role, Some(ducktape_view_guest::Role::TabList));
    assert!(tabs.focusable && tabs.tab_stop == Some(true));
    assert!(!cx.interactivity("chat-reaction-tab-Smileys").focusable);
    assert!(
        cx.interactivity("chat-reaction-tab-Smileys")
            .aria
            .active_descendant
    );
    cx.simulate_key_down("chat-reaction-tabs", "right");
    view.read(|chat| assert_eq!(chat.picker.tab, 1));
    assert!(
        cx.find("chat-reaction-People-👋").is_some()
            || cx.find("chat-reaction-Smileys-😀").is_none()
    );
    let people = cx.interactivity("chat-reaction-tab-People");
    assert_eq!(people.aria.selected, Some(true));
    assert!(people.aria.active_descendant);
}

#[test]
fn the_picker_searches_and_enter_picks_the_first_match() {
    let (mut cx, view) = opened();
    hover(&mut cx, &view, 1);
    cx.simulate_click("chat-message-m1-react");
    assert!(cx.find("chat-reaction-🔥").is_some(), "the frequent row");
    assert!(
        cx.find("chat-reaction-Smileys-😀").is_some(),
        "the first tab"
    );
    cx.simulate_click("chat-reaction-tab-Food");
    assert!(cx.find("chat-reaction-Food-🍕").is_some());
    let focus = ui::menu::focus_key(Pane::Timeline, Mode::Reactions);
    cx.simulate_input(&focus, "duck");
    assert!(cx.has_text("1 MATCH"));
    cx.simulate_submit(&focus);
    cx.run_until_parked();
    assert!(
        cx.host()
            .requests::<Submit<ChatApi>>()
            .iter()
            .any(|op| matches!(op, Op::AddReaction { emoji, .. } if emoji == "🦆"))
    );
    view.read(|chat| {
        assert!(chat.menu.is_none());
        assert_eq!(chat.recent_emoji.first().map(String::as_str), Some("🦆"));
    });
}

/// The timeline is one grid: Tab lands on the newest message, ↑ reveals and
/// claims the one before it, → and Enter press its first control, Enter on
/// the content selects the row as a click would (the strip appears).
#[test]
fn the_timeline_is_a_grid_whose_arrows_walk_messages_and_their_controls() {
    let (mut cx, view) = opened();
    view.update(&mut cx, |chat, _, cx| {
        chat.room.as_mut().unwrap().messages.ready_mut().unwrap()[0]
            .reactions
            .push(chat::Reaction {
                emoji: "🔥".into(),
                count: 2,
                reacted_by_me: false,
            });
        cx.notify();
    });
    cx.run_until_parked();
    let grid = cx.interactivity("chat-message-list");
    assert_eq!(grid.role, Some(ducktape_view_guest::Role::Grid));
    assert!(grid.focusable && grid.tab_stop == Some(true));
    // Grid > Row > GridCell: the card is cell 0 and keeps the pointer's
    // click; the row holds the name and never claims
    let newest = cx.interactivity("chat-message-m2");
    assert_eq!(newest.role, Some(ducktape_view_guest::Role::GridCell));
    assert!(newest.aria.active_descendant, "the newest message on entry");
    assert!(!newest.focusable && newest.on_click.is_some());
    let row = cx.interactivity("chat-message-m2-row");
    assert_eq!(row.role, Some(ducktape_view_guest::Role::Row));
    assert!(!row.focusable && !row.aria.active_descendant && row.on_click.is_none());
    assert!(
        row.aria
            .label
            .as_deref()
            .is_some_and(|label| label.starts_with("Select message, shows its actions:"))
    );

    cx.simulate_key_down("chat-message-list", "up");
    assert!(cx.interactivity("chat-message-m1").aria.active_descendant);
    // the list draws the revealed row until the host asks for more
    assert!(!super::room::claims(&cx, "chat-message-m2"));
    let Some(wire::Node::List { commands, .. }) = cx
        .find("chat-message-list")
        .and_then(|grid| grid.children().first())
    else {
        panic!("the grid holds the list");
    };
    assert!(
        commands
            .iter()
            .any(|command| matches!(command, wire::ListCommand::ScrollToRevealItem(1))),
        "the list reveals the message (after the intro row) before it claims: {commands:?}"
    );

    // → the controls in paint order: the header's block link, then the
    // reaction chip, a button that stays one
    cx.simulate_key_down("chat-message-list", "right");
    let link = cx.interactivity("chat-message-m1-height");
    assert_eq!(link.role, Some(ducktape_view_guest::Role::Link));
    assert!(link.aria.active_descendant && !link.focusable);
    cx.simulate_key_down("chat-message-list", "right");
    let chip = cx.interactivity("chat-message-m1-reaction-🔥");
    assert_eq!(chip.role, Some(ducktape_view_guest::Role::Button));
    assert!(chip.aria.active_descendant && !chip.focusable);
    assert!(!cx.interactivity("chat-message-m1").aria.active_descendant);
    cx.simulate_key_down("chat-message-list", "enter");
    cx.run_until_parked();
    assert!(
        cx.host()
            .requests::<Submit<ChatApi>>()
            .iter()
            .any(|op| matches!(op, Op::AddReaction { emoji, seq: 1, .. } if emoji == "🔥"))
    );
    // Home is the content; Enter is the row's click: chosen, its strip shows
    cx.simulate_key_down("chat-message-list", "home");
    assert!(cx.interactivity("chat-message-m1").aria.active_descendant);
    assert!(cx.find("chat-message-m1-actions").is_none());
    cx.simulate_key_down("chat-message-list", "enter");
    view.read(|chat| {
        let menu = chat.menu.as_ref().expect("the message is chosen");
        assert_eq!((menu.seq, menu.mode), (1, Mode::Toolbar));
    });
    assert!(cx.find("chat-message-m1-actions").is_some());
    // the strip's buttons are the cells after the chip and the `+`, each
    // in a cell of the row beside the card
    cx.simulate_key_down("chat-message-list", "end");
    assert!(
        cx.interactivity("chat-message-m1-more")
            .aria
            .active_descendant
    );
    let strip = cx.find("chat-message-m1-actions").expect("the strip");
    assert!(strip.children().iter().all(|cell| {
        cell.interactivity()
            .is_some_and(|cell| cell.role == Some(ducktape_view_guest::Role::GridCell))
            && cell.children().len() == 1
    }));
    assert!(
        cx.find("chat-message-m1-row")
            .expect("the row")
            .children()
            .iter()
            .any(|child| child.key() == Some("chat-message-m1-actions")),
        "the strip's cells are the row's, not the card's"
    );
    cx.simulate_key_down("chat-message-list", "enter");
    view.read(|chat| {
        let menu = chat.menu.as_ref().expect("the menu opened");
        assert_eq!(menu.mode, Mode::More);
        assert_eq!(
            menu.at,
            chat.key_spot(Pane::Timeline),
            "a key opens it at the list's spot"
        );
    });
}

/// An emoji grid is one stop of rows of eight: → → Enter adds the third
/// emoji of the open category.
#[test]
fn two_arrows_and_enter_on_the_emoji_grid_add_the_third_emoji() {
    let (mut cx, view) = opened();
    hover(&mut cx, &view, 1);
    cx.simulate_click("chat-message-m1-react");
    let grid = cx.interactivity("chat-reaction-grid");
    assert_eq!(grid.role, Some(ducktape_view_guest::Role::Grid));
    assert!(grid.focusable && grid.tab_stop == Some(true));
    let rows = cx.find("chat-reaction-grid").unwrap().children();
    assert_eq!(rows.len(), emoji::PER_TAB.div_ceil(8));
    assert_eq!(rows[0].children().len(), 8, "rows of eight");
    let category = &emoji::CATEGORIES[0];
    let third = category.emoji[2].0;
    let key = |emoji: &str| format!("chat-reaction-{}-{emoji}", category.name);
    assert!(
        cx.interactivity(&key(category.emoji[0].0))
            .aria
            .active_descendant
    );
    assert!(!cx.interactivity(&key(third)).focusable);
    cx.simulate_key_down("chat-reaction-grid", "right");
    cx.simulate_key_down("chat-reaction-grid", "right");
    assert!(cx.interactivity(&key(third)).aria.active_descendant);
    // ↓ a row: eight on
    cx.simulate_key_down("chat-reaction-grid", "down");
    assert!(
        cx.interactivity(&key(category.emoji[10].0))
            .aria
            .active_descendant
    );
    cx.simulate_key_down("chat-reaction-grid", "up");
    cx.simulate_key_down("chat-reaction-grid", "enter");
    cx.run_until_parked();
    assert!(
        cx.host()
            .requests::<Submit<ChatApi>>()
            .iter()
            .any(|op| matches!(op, Op::AddReaction { emoji, seq: 1, .. } if emoji == third))
    );
    view.read(|chat| assert!(chat.menu.is_none(), "a reaction closes the picker"));
}
