use super::*;
use crate::{Context, Driver, ParentElement, Render, Role, View, div};
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize)]
struct ListView {
    #[serde(skip)]
    state: ListState,
    #[serde(skip)]
    rendered: Vec<usize>,
    scrolls: usize,
}

impl View for ListView {
    fn new(_: &mut Window, _: &mut Context<Self>) -> Self {
        Self {
            state: ListState::new(2_000, ListAlignment::Bottom, px(160.)),
            rendered: Vec::new(),
            scrolls: 0,
        }
    }
}

impl Render for ListView {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        self.state.set_scroll_handler(cx.listener(|view, _, _, cx| {
            view.scrolls += 1;
            cx.notify();
        }));
        list(
            self.state.clone(),
            cx.processor(|view, index, _, _| {
                view.rendered.push(index);
                div()
                    .id(format!("row-{index}"))
                    .on_click(|_, _, _| {})
                    .child(index.to_string())
                    .into_any_element()
            }),
        )
    }
}

fn list_node(frame: &wire::Frame) -> &wire::Node {
    frame.root.as_ref().expect("list root")
}

#[test]
fn a_row_scrolled_in_leaves_the_other_rows_routes_alone() {
    let mut driver = Driver::<ListView>::new();
    let first = driver.tick(vec![]);
    let wire::Node::List {
        request_handler,
        range_start,
        ..
    } = list_node(&first)
    else {
        panic!("expected list")
    };
    let (handler, start) = (*request_handler, *range_start);
    let frame = driver.tick_wire(vec![wire::Event::ListRequest {
        handler,
        request: wire::ListRequest {
            start: start - 1,
            end: start,
        },
    }]);
    let props = frame
        .patches
        .iter()
        .filter(|patch| matches!(patch, wire::Patch::Props { path, .. } if !path.is_empty()))
        .count();
    let inserts = frame
        .patches
        .iter()
        .filter(|patch| matches!(patch, wire::Patch::Insert { .. }))
        .count();
    assert_eq!((props, inserts), (0, 1), "{:?}", frame.patches);
}

#[test]
fn two_thousand_items_only_evaluate_a_bounded_requested_window() {
    let mut driver = Driver::<ListView>::new();
    let first = driver.tick(vec![]);
    let (handler, scroll_handler) = match list_node(&first) {
        wire::Node::List {
            item_count,
            range_start,
            children,
            request_handler,
            scroll_handler,
            ..
        } => {
            assert_eq!(*item_count, 2_000);
            assert_eq!(*range_start, 2_000 - INITIAL_ROWS);
            assert_eq!(children.len(), INITIAL_ROWS);
            (
                *request_handler,
                scroll_handler.expect("settled scroll route"),
            )
        }
        other => panic!("expected list, got {other:?}"),
    };
    driver
        .entity()
        .read(|view| assert_eq!(view.rendered.len(), INITIAL_ROWS));

    let second = driver.tick(vec![wire::Event::ListRequest {
        handler,
        request: wire::ListRequest {
            start: 0,
            end: usize::MAX,
        },
    }]);
    match list_node(&second) {
        wire::Node::List {
            range_start,
            children,
            ..
        } => {
            assert_eq!(*range_start, 0);
            assert_eq!(children.len(), wire::MAX_LIST_ROWS);
        }
        other => panic!("expected list, got {other:?}"),
    }
    driver
        .entity()
        .read(|view| assert_eq!(view.rendered.len(), INITIAL_ROWS + wire::MAX_LIST_ROWS));

    driver.tick(vec![wire::Event::ListScroll {
        handler: scroll_handler,
        event: wire::ListScroll {
            visible_start: 1_990,
            visible_end: 2_000,
            count: 2_000,
            is_scrolled: true,
            is_following_tail: false,
            offset: wire::ListOffset {
                item_ix: 1_990,
                offset_in_item: 7.,
            },
        },
    }]);
    driver.entity().read(|view| {
        assert_eq!(view.scrolls, 1);
        assert_eq!(view.state.logical_scroll_top().item_ix, 1_990);
        assert!(!view.state.is_following_tail());
    });
}

#[test]
fn state_operations_cross_once_as_native_commands_and_patch_props() {
    let mut driver = Driver::<ListView>::new();
    driver.tick(vec![]);
    let state = driver.entity().read(|view| view.state.clone());
    state.splice(0..0, 20);
    state.remeasure_items(100..102);
    state.scroll_to_reveal_item(1_999);
    driver.app.notify();
    let frame = driver.tick(vec![]);
    match list_node(&frame) {
        wire::Node::List {
            item_count,
            commands,
            children,
            ..
        } => {
            assert_eq!(*item_count, 2_020);
            assert_eq!(commands.len(), 3);
            assert!(children.len() <= wire::MAX_LIST_ROWS);
        }
        other => panic!("expected list, got {other:?}"),
    }
    state.scroll_to(ListOffset {
        item_ix: 1_999,
        offset_in_item: px(3.),
    });
    driver.app.notify();
    let patched = driver.tick(vec![]);
    assert!(
        patched
            .patches
            .iter()
            .any(|patch| matches!(patch, wire::Patch::Props { .. })),
        "same-shape state operations update by props patches"
    );
    driver.app.notify();
    let next = driver.tick(vec![]);
    assert!(matches!(list_node(&next), wire::Node::List { commands, .. } if commands.is_empty()));
}

#[test]
fn list_handles_and_requested_windows_are_driver_isolated() {
    let mut first = Driver::<ListView>::new();
    let mut second = Driver::<ListView>::new();
    let first_frame = first.tick(vec![]);
    let second_frame = second.tick(vec![]);
    let (first_state, first_handler) = match list_node(&first_frame) {
        wire::Node::List {
            state,
            request_handler,
            ..
        } => (*state, *request_handler),
        _ => unreachable!(),
    };
    let second_state = match list_node(&second_frame) {
        wire::Node::List { state, .. } => *state,
        _ => unreachable!(),
    };
    assert_ne!(first_state, second_state);
    first.tick(vec![wire::Event::ListRequest {
        handler: first_handler,
        request: wire::ListRequest { start: 7, end: 9 },
    }]);
    first
        .entity()
        .read(|view| assert_eq!(view.rendered.last().copied(), Some(8)));
    second
        .entity()
        .read(|view| assert_eq!(view.rendered.last().copied(), Some(1_999)));
}

#[test]
fn a_list_carries_its_role_and_name_to_the_wire() {
    let mut app = App::for_driver();
    let mut window = app.window();
    let rows = list(
        ListState::new(3, ListAlignment::Top, px(40.)),
        |index, _, _| div().child(index.to_string()).into_any_element(),
    )
    .role(Role::ListBox)
    .aria_label("Members")
    .focusable();
    let node = Lowering::new(&mut window, &mut app).lower(rows);
    let wire::Node::List { interactivity, .. } = node else {
        panic!("a list")
    };
    assert_eq!(interactivity.role, Some(Role::ListBox));
    assert_eq!(interactivity.aria.label.as_deref(), Some("Members"));
    assert!(interactivity.focusable);
}
