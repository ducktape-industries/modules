//! The timeline as a reader scrolls it: the list's rows through a re-read,
//! the paging at both ends of a landed window, and what a hover costs.
use super::*;
use crate::ui::timeline::{OVERDRAW, splices};
use std::cell::Cell;
use std::rc::Rc;

fn list_of(node: &wire::Node) -> Option<&wire::Node> {
    if matches!(node, wire::Node::List { .. }) {
        return Some(node);
    }
    node.children().iter().find_map(list_of)
}

fn timeline(cx: &TestAppContext) -> wire::Node {
    list_of(cx.find("chat-message-list").expect("the timeline"))
        .cloned()
        .expect("the timeline is a list")
}

fn window(cx: &TestAppContext) -> (String, usize, Vec<String>) {
    let wire::Node::List {
        id,
        range_start,
        children,
        ..
    } = timeline(cx)
    else {
        unreachable!()
    };
    let keys = children
        .iter()
        .map(|row| row.key().unwrap_or_default().to_owned())
        .collect();
    let wire::ElementIdWire::Name(id) = id else {
        panic!("the list is named")
    };
    (id.to_string(), range_start, keys)
}

fn commands(cx: &TestAppContext) -> Vec<wire::ListCommand> {
    let wire::Node::List { commands, .. } = timeline(cx) else {
        unreachable!()
    };
    commands
}

fn full_room(newest: Rc<Cell<u64>>) -> (TestAppContext, Entity<Chat>) {
    let mut cx = TestAppContext::new();
    configure(&mut cx);
    cx.host().handle::<Ask<::chat::Chat>>(move |query| {
        Ok(match query {
            Query::Accounts { .. } => Reply::Accounts(page(Vec::new())),
            Query::Channels { .. } => {
                Reply::Channels(page(vec![channel("general", "General", newest.get())]))
            }
            Query::Roots { .. } => {
                let top = newest.get();
                Reply::Roots(::chat::PageResponse {
                    height: 1,
                    items: (top + 1 - WINDOW as u64..=top).map(marked_row).collect(),
                    next: Some(vec![1]),
                })
            }
            Query::Members { .. } => Reply::Members(page(Vec::new())),
            query => panic!("unexpected chat query: {query:?}"),
        })
    });
    let view = cx.open::<Chat>();
    seat(&mut cx);
    cx.simulate_click("chat-sidebar-channel-general");
    cx.run_until_parked();
    (cx, view)
}

#[test]
fn a_post_above_a_reader_scrolled_up_slides_the_list_by_two_edits() {
    let newest = Rc::new(Cell::new(WINDOW as u64));
    let (mut cx, _) = full_room(newest.clone());
    let changes = cx.host().stream::<Changes<::chat::Chat>>();
    cx.simulate_range("chat-message-list", 100..130);
    let (id, start, shown) = window(&cx);
    assert_eq!(id, "chat-message-rows");
    newest.set(WINDOW as u64 + 1);
    changes.send(block(4, posted("general")));
    cx.run_until_parked();
    assert_eq!(
        commands(&cx),
        [
            wire::ListCommand::Splice {
                start: WINDOW,
                end: WINDOW,
                count: 1,
            },
            wire::ListCommand::Splice {
                start: 0,
                end: 1,
                count: 0,
            },
        ]
    );
    let (_, after, kept) = window(&cx);
    assert_eq!(after, start - 1, "the window moved with its rows");
    assert_eq!(kept, shown, "the rows on screen are the rows it holds");
}

#[test]
fn an_older_page_lands_only_in_the_room_it_was_read_for() {
    let (mut cx, view) = full_room(Rc::new(Cell::new(WINDOW as u64)));
    let older = Rc::new(Cell::new(0));
    let paged = older.clone();
    cx.host().handle::<Ask<::chat::Chat>>(move |query| {
        Ok(match query {
            Query::Roots {
                page: ::chat::PageRequest { after: None, .. },
                ..
            } => Reply::Roots(::chat::PageResponse {
                height: 1,
                items: (1..=WINDOW as u64).map(marked_row).collect(),
                next: Some(vec![1]),
            }),
            Query::Roots { .. } => {
                paged.set(paged.get() + 1);
                Reply::Roots(::chat::PageResponse {
                    height: 1,
                    items: vec![marked_row(1_000 + paged.get())],
                    next: (paged.get() < 4).then(|| vec![1]),
                })
            }
            Query::Members { .. } => Reply::Members(page(Vec::new())),
            Query::Channels { .. } => Reply::Channels(page(Vec::new())),
            query => panic!("unexpected chat query: {query:?}"),
        })
    });
    cx.update(&view, |chat, _, cx| {
        chat.load_older(cx);
        chat.open("dm-7-8".into(), cx);
        cx.notify();
    });
    cx.run_until_parked();
    view.read(|chat| {
        let room = chat.room.as_ref().unwrap();
        assert_eq!(room.id, "dm-7-8");
        assert_eq!(room.messages.ready().map(Vec::len), Some(WINDOW));
    });
    assert_eq!(older.get(), 4, "the older page was read through");
}

fn keys(keys: &[&str]) -> Vec<String> {
    keys.iter().map(|key| (*key).to_owned()).collect()
}

#[test]
fn a_list_is_edited_by_its_keys() {
    let old = keys(&["a", "b", "c", "d", "e"]);
    let edits = |new: &[&str]| splices(&old, &keys(new));
    assert_eq!(edits(&["b", "c", "d", "e", "f"]), [(0..1, 0), (5..5, 1)]);
    assert_eq!(edits(&["c", "d", "e", "f", "g"]), [(0..2, 0), (5..5, 2)]);
    assert_eq!(edits(&["x", "y", "a", "b", "c", "d", "e"]), [(0..0, 2)]);
    assert_eq!(edits(&["a", "b", "c", "d", "e", "f"]), [(5..5, 1)]);
    assert_eq!(edits(&["a", "b", "d", "e"]), [(2..3, 0)]);
    assert_eq!(edits(&["a", "b", "c", "d", "e"]), []);
    assert_eq!(edits(&["v", "w", "x", "y", "z"]), [(0..5, 5)]);
    assert_eq!(
        edits(&["b", "x", "d", "e", "f"]),
        [(0..1, 0), (2..3, 1), (5..5, 1)]
    );
}

#[test]
fn a_list_changed_all_through_is_one_edit() {
    let old: Vec<String> = (0..40).map(|at| at.to_string()).collect();
    let new: Vec<String> = old
        .iter()
        .enumerate()
        .map(|(at, key)| match at % 2 {
            0 => format!("x{key}"),
            _ => key.clone(),
        })
        .collect();
    assert_eq!(splices(&old, &new), [(0..39, 39)]);
    let reversed: Vec<String> = old.iter().rev().cloned().collect();
    let edits = splices(&old, &reversed);
    let rows = edits
        .iter()
        .rev()
        .fold(old.len(), |rows, (gone, count)| rows - gone.len() + count);
    assert_eq!(rows, reversed.len(), "{edits:?}");
}

#[test]
fn the_timeline_is_laid_out_past_the_screen() {
    let (cx, _) = opened();
    let wire::Node::List { overdraw, .. } = timeline(&cx) else {
        unreachable!()
    };
    assert_eq!(overdraw, OVERDRAW);
    assert!(overdraw >= 600., "several notches of the wheel: {overdraw}");
}

#[test]
fn a_hover_folds_no_message_again() {
    let (mut cx, view) = opened();
    let folded = |view: &Entity<Chat>| {
        view.read(|chat| {
            chat.drawn(Pane::Timeline)
                .borrow()
                .folded
                .as_ref()
                .map(|folded| folded.messages.clone())
                .expect("the timeline folded its rows")
        })
    };
    let before = folded(&view);
    cx.simulate_hover("chat-message-m1-row", true);
    assert!(cx.find("chat-message-m1-actions").is_some());
    cx.simulate_hover("chat-message-m1-row", false);
    cx.simulate_hover("chat-message-m2-row", true);
    assert!(cx.find("chat-message-m2-actions").is_some());
    assert!(Rc::ptr_eq(&before, &folded(&view)), "no row moved");
    cx.update(&view, |chat, _, cx| {
        let rows = chat.room.as_mut().unwrap().messages.ready_mut().unwrap();
        rows[1].reactions.push(::chat::Reaction {
            emoji: "🎉".into(),
            count: 1,
            reacted_by_me: false,
        });
        cx.notify();
    });
    cx.run_until_parked();
    assert!(!Rc::ptr_eq(&before, &folded(&view)), "a reaction moved m2");
    assert!(cx.has_text("🎉"), "{:?}", cx.texts());
}

#[test]
fn jump_to_latest_shows_as_the_reader_leaves_the_tail() {
    let (mut cx, _) = opened();
    assert!(cx.find("chat-jump-latest").is_none());
    let wire::Node::List {
        scroll_handler: Some(handler),
        item_count,
        ..
    } = timeline(&cx)
    else {
        panic!("the timeline hears its scroll")
    };
    let scrolled = |end: usize, tail: bool| wire::Event::ListScroll {
        handler,
        event: wire::ListScroll {
            visible_start: 0,
            visible_end: end,
            count: item_count,
            is_scrolled: !tail,
            is_following_tail: tail,
            offset: wire::ListOffset::default(),
        },
    };
    cx.simulate_event(scrolled(item_count - 1, false));
    assert!(cx.find("chat-jump-latest").is_some());
    cx.simulate_event(scrolled(item_count, true));
    assert!(cx.find("chat-jump-latest").is_none());
}

fn long_room(head: Rc<Cell<u64>>) -> (TestAppContext, Entity<Chat>) {
    let mut cx = TestAppContext::new();
    configure(&mut cx);
    cx.host().handle::<Ask<::chat::Chat>>(move |query| {
        let head = head.get();
        Ok(match query {
            Query::Accounts { .. } => Reply::Accounts(page(Vec::new())),
            Query::Channels { .. } => {
                Reply::Channels(page(vec![channel("general", "General", head)]))
            }
            Query::MessagesAround { seq, page, .. } => {
                let half = page.limit() / 2;
                let lo = seq.saturating_sub(half).max(1);
                let hi = seq.saturating_add(half).min(head);
                Reply::Messages((lo..=hi).map(marked_row).collect())
            }
            Query::Roots { .. } => Reply::Roots(::chat::PageResponse {
                height: 1,
                items: (head + 1 - WINDOW as u64..=head).map(marked_row).collect(),
                next: Some(vec![1]),
            }),
            Query::Members { .. } => Reply::Members(page(Vec::new())),
            query => panic!("unexpected chat query: {query:?}"),
        })
    });
    let view = cx.open::<Chat>();
    seat(&mut cx);
    (cx, view)
}

fn seqs(view: &Entity<Chat>) -> (u64, u64, usize) {
    view.read(|chat| {
        let rows = chat.room.as_ref().unwrap().messages.ready().unwrap();
        (rows[0].seq, rows[rows.len() - 1].seq, rows.len())
    })
}

#[test]
fn a_landed_room_opens_on_its_message_and_pages_both_ways() {
    let head = Rc::new(Cell::new(1_000));
    let (mut cx, view) = long_room(head.clone());
    let routes = cx
        .host()
        .stream::<ducktape_view_guest::methods::HostRoute>();
    routes.send("general/500".into());
    cx.run_until_parked();
    assert_eq!(seqs(&view), (436, 564, 129), "64 either side of 500");
    let (id, _, _) = window(&cx);
    assert_eq!(id, "chat-landed-rows");
    assert!(
        commands(&cx).contains(&wire::ListCommand::ScrollTo(wire::ListOffset {
            item_ix: 500 - 436,
            offset_in_item: 0.,
        })),
        "{:?}",
        commands(&cx)
    );
    assert!(cx.has_text("Load older messages"));

    let wire::Node::List { item_count, .. } = timeline(&cx) else {
        unreachable!()
    };
    cx.simulate_range("chat-message-list", item_count - 10..item_count);
    assert_eq!(seqs(&view), (436, 629, 194), "a page below the last row");
    cx.simulate_range("chat-message-list", 0..10);
    assert_eq!(
        seqs(&view),
        (371, 626, 256),
        "a page above, the far end let go"
    );
    view.read(|chat| assert!(chat.room.as_ref().unwrap().landed));

    for _ in 0..8 {
        let wire::Node::List { item_count, .. } = timeline(&cx) else {
            unreachable!()
        };
        cx.simulate_range("chat-message-list", item_count - 10..item_count);
    }
    view.read(|chat| {
        let room = chat.room.as_ref().unwrap();
        assert!(!room.landed, "read down to the head, the room is live");
        assert!(room.at_tail);
    });
    let (id, _, _) = window(&cx);
    assert_eq!(id, "chat-message-rows", "kept on the newest");
    let (_, last, _) = seqs(&view);
    assert_eq!(last, 1_000);
    assert!(cx.find("chat-jump-latest").is_none());
}

#[test]
#[ignore = "measurement: prints a hover's render time over a full room, asserts nothing of it"]
fn a_hover_over_a_full_room_measured() {
    for shown in [12, 40, 64] {
        let (mut cx, _) = full_room(Rc::new(Cell::new(WINDOW as u64)));
        cx.simulate_range("chat-message-list", WINDOW - shown..WINDOW);
        let hovers = 200;
        let clock = std::time::Instant::now();
        for at in 0..hovers {
            let seq = WINDOW as u64 - (at % 10) as u64;
            cx.simulate_hover(&format!("chat-message-m{seq}-row"), true);
        }
        let each = clock.elapsed() / hovers as u32;
        let (_, start, rows) = window(&cx);
        eprintln!(
            "a hover over a {WINDOW}-row room, {} rows lowered from {start}: {each:?} per hover",
            rows.len()
        );
    }
}
