//! A list row is filed under its own id, else under its index, and the ids
//! inside it under the row (`wire::identity`): two rows that author the
//! same id inside them both draw, each pressing its own listener, and a
//! list is a scope of its own. A duplicate an author wrote by hand among
//! siblings fails the view's test, naming the id and its scope: the test
//! host runs the host's sanitizer on every frame, and the words are its.
use super::*;
use crate::testing::TestAppContext;
use serde::{Deserialize, Serialize};
use std::cell::RefCell;
use std::rc::Rc;

type Log = Rc<RefCell<Vec<usize>>>;

/// Row `index` as a list draws it: no id of its own, an `open` button in it.
fn row(index: usize, log: &Log) -> AnyElement {
    let log = log.clone();
    div()
        .flex()
        .child(format!("Row {index}"))
        .child(
            div()
                .id("open")
                .role(Role::Button)
                .focusable()
                .aria_label(format!("Open {index}"))
                .on_click(move |_, _, _| log.borrow_mut().push(index)),
        )
        .into_any_element()
}

/// The `open` buttons' click routes, in tree order.
fn opens(node: &wire::Node, out: &mut Vec<u32>) {
    if let wire::Node::Container(wire::ContainerNode {
        interactivity, id, ..
    }) = node
        && id.as_ref().and_then(wire::ElementIdWire::name) == Some("open")
    {
        out.push(interactivity.on_click.expect("a click route"));
    }
    for child in node.children() {
        opens(child, out);
    }
}

fn press_each(cx: &mut TestAppContext) {
    let mut routes = Vec::new();
    opens(cx.root(), &mut routes);
    assert_eq!(routes.len(), 3, "every row drew its button");
    for handler in routes {
        cx.tick(vec![wire::Event::Click {
            handler,
            event: (&gpui::ClickEvent::default()).into(),
        }]);
    }
}

#[derive(Default, Serialize, Deserialize)]
struct Rows {
    #[serde(skip)]
    state: Option<ListState>,
    #[serde(skip)]
    log: Log,
}
impl View for Rows {
    const NAME: &'static str = "Rows";
    fn new(_: &mut Window, _: &mut Context<Self>) -> Self {
        Self {
            state: Some(ListState::new(3, ListAlignment::Top, px(40.))),
            log: Log::default(),
        }
    }
}
impl Render for Rows {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        let log = self.log.clone();
        div()
            .id("rows")
            .size_full()
            .child(list(self.state.clone().unwrap(), move |index, _, _| {
                row(index, &log)
            }))
    }
}

/// P28's shape: rows of one data-driven list author one id inside them.
/// The view lives, and each press runs its own row's listener.
#[test]
fn rows_of_a_list_may_author_the_same_id_inside_them() {
    let mut cx = TestAppContext::new();
    let view = cx.open::<Rows>();
    press_each(&mut cx);
    let log = view.read(|view| view.log.clone());
    assert_eq!(*log.borrow(), [0, 1, 2]);
}

/// Two lists under one identified parent, their rows with no id at all,
/// beside a sibling named by the integer a first row is filed under: the
/// second list's rows log ten past their index.
#[derive(Default, Serialize, Deserialize)]
struct TwoLists {
    #[serde(skip)]
    states: Vec<ListState>,
    #[serde(skip)]
    log: Log,
}
impl View for TwoLists {
    const NAME: &'static str = "TwoLists";
    fn new(_: &mut Window, _: &mut Context<Self>) -> Self {
        Self {
            states: (0..2)
                .map(|_| ListState::new(3, ListAlignment::Top, px(40.)))
                .collect(),
            log: Log::default(),
        }
    }
}
impl Render for TwoLists {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        let lists = self.states.iter().cloned().enumerate().map(|(at, state)| {
            let log = self.log.clone();
            list(state, move |index, _, _| row(at * 10 + index, &log))
        });
        div()
            .id("page")
            .size_full()
            .children(lists)
            .child(div().id(0usize))
    }
}

/// A list is a scope of its own: the rows of two lists under one parent,
/// each filed under its index, meet neither each other nor a sibling of
/// the lists named by that integer. Every row keeps its own route from
/// frame to frame, and a list that grows leaves the other's alone.
#[test]
fn two_lists_under_one_parent_each_file_their_own_rows() {
    let mut cx = TestAppContext::new();
    let view = cx.open::<TwoLists>();
    let mut routes = Vec::new();
    opens(cx.root(), &mut routes);
    assert_eq!(routes.len(), 6, "both lists drew their rows");
    cx.update(&view, |view, _, cx| {
        view.states[0].splice(3..3, 1);
        cx.notify();
    });
    cx.run_until_parked();
    let mut after = Vec::new();
    opens(cx.root(), &mut after);
    assert_eq!(after.len(), 7, "the first list grew a row");
    assert_eq!(after[..3], routes[..3], "the first list's rows moved");
    assert_eq!(after[4..], routes[3..], "the second list's rows moved");
    for handler in routes {
        cx.tick(vec![wire::Event::Click {
            handler,
            event: (&gpui::ClickEvent::default()).into(),
        }]);
    }
    let log = view.read(|view| view.log.clone());
    assert_eq!(*log.borrow(), [0, 1, 2, 10, 11, 12]);
}

#[derive(Default, Serialize, Deserialize)]
struct UniformRows {
    #[serde(skip)]
    log: Log,
}
impl View for UniformRows {
    const NAME: &'static str = "UniformRows";
    fn new(_: &mut Window, _: &mut Context<Self>) -> Self {
        Self::default()
    }
}
impl Render for UniformRows {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        let log = self.log.clone();
        uniform_list("rows", 3, move |range, _, _| {
            range.map(|index| row(index, &log)).collect::<Vec<_>>()
        })
    }
}

#[test]
fn rows_of_a_uniform_list_may_author_the_same_id_inside_them() {
    let mut cx = TestAppContext::new();
    let view = cx.open::<UniformRows>();
    let wire::Node::UniformList { path, route, .. } = cx.root().clone() else {
        panic!("a uniform list")
    };
    cx.tick(vec![wire::Event::UniformListRange {
        path,
        route,
        start: 0,
        end: 3,
    }]);
    press_each(&mut cx);
    let log = view.read(|view| view.log.clone());
    assert_eq!(*log.borrow(), [0, 1, 2]);
}

/// Two children of `rows`: `x`, and `second` under an id-less wrapper,
/// which is transparent to the scope.
#[derive(Default, Serialize, Deserialize)]
struct Twice {
    second: String,
}
impl View for Twice {
    const NAME: &'static str = "Twice";
    fn new(_: &mut Window, _: &mut Context<Self>) -> Self {
        Self { second: "y".into() }
    }
}
impl Render for Twice {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div().id("page").child(
            div().id("rows").child(div().id("x").child("one")).child(
                div().child(
                    div()
                        .id(SharedString::from(self.second.clone()))
                        .child("two"),
                ),
            ),
        )
    }
}

/// The author writes `x` twice among the children of `rows`.
fn write_twice(cx: &mut TestAppContext, view: &Entity<Twice>) {
    cx.update(view, |view, _, cx| {
        view.second = "x".into();
        cx.notify();
    });
    cx.run_until_parked();
}

#[test]
#[should_panic(
    expected = "the host refuses this frame: duplicate typed element identity \
                           among siblings: x twice under page > rows"
)]
fn a_duplicate_written_among_siblings_fails_naming_its_site() {
    let mut cx = TestAppContext::new();
    let view = cx.open::<Twice>();
    write_twice(&mut cx, &view);
}

/// The panic `f` ends in, as text.
fn panic_message(f: impl FnOnce()) -> String {
    let payload = std::panic::catch_unwind(std::panic::AssertUnwindSafe(f))
        .expect_err("the test host refuses a duplicate");
    payload
        .downcast_ref::<String>()
        .cloned()
        .or_else(|| payload.downcast_ref::<&str>().map(|text| text.to_string()))
        .unwrap_or_default()
}

/// What a view's test fails with when the host's sanitizer refuses `root`,
/// sent whole.
fn refusal(root: wire::Node) -> String {
    let mut frame = wire::Frame {
        root: Some(root),
        ..Default::default()
    };
    let refused = wire::sanitize(&mut frame).expect_err("the host refuses it");
    format!("the host refuses this frame: {refused}")
}

/// `name` wherever an id names it is `to`.
fn rename(node: &mut wire::Node, name: &str, to: &str) {
    node.for_each_mut(&mut |node| {
        if let wire::Node::Container(wire::ContainerNode { id: Some(id), .. }) = node
            && id.name() == Some(name)
        {
            *id = wire::ElementIdWire::Name(to.to_owned().into());
        }
    });
}

/// A view's test and the host refuse a duplicate in one place, the
/// sanitizer: the tree the guest lowers with `y`, renamed `x` on the wire,
/// is refused by it in the words the test of the view that writes `x`
/// fails with.
#[test]
fn a_views_test_fails_a_duplicate_with_the_hosts_refusal() {
    let mut cx = TestAppContext::new();
    let view = cx.open::<Twice>();
    let mut sent = cx.root().clone();
    rename(&mut sent, "y", "x");
    let guest = panic_message(|| write_twice(&mut cx, &view));
    assert_eq!(guest, refusal(sent));
}

/// The same for rows: two rows of a list each filed under its own id `m`
/// (a key the data repeated) are one id twice in one scope, the list's,
/// refused in the view's test as the host refuses them.
#[derive(Default, Serialize, Deserialize)]
struct Keyed {
    keys: Vec<String>,
    #[serde(skip)]
    state: Option<ListState>,
}
impl View for Keyed {
    const NAME: &'static str = "Keyed";
    fn new(_: &mut Window, _: &mut Context<Self>) -> Self {
        Self {
            keys: vec!["m".into(), "n".into()],
            state: Some(ListState::new(2, ListAlignment::Top, px(40.))),
        }
    }
}
impl Render for Keyed {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        let keys = self.keys.clone();
        div()
            .id("rows")
            .size_full()
            .child(list(self.state.clone().unwrap(), move |index, _, _| {
                div()
                    .id(SharedString::from(keys[index].clone()))
                    .child(keys[index].clone())
                    .into_any_element()
            }))
    }
}

#[test]
fn rows_named_by_one_key_fail_a_views_test_with_the_hosts_refusal() {
    let mut cx = TestAppContext::new();
    let view = cx.open::<Keyed>();
    let mut sent = cx.root().clone();
    rename(&mut sent, "n", "m");
    let guest = panic_message(|| {
        cx.update(&view, |view, _, cx| {
            view.keys[1] = "m".into();
            cx.notify();
        });
        cx.run_until_parked();
    });
    assert_eq!(guest, refusal(sent));
}
