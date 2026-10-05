//! `entity.cached(style)`: a cached child entity that was not notified is
//! not rendered, not lowered and not diffed; its kept subtree stands in.
//! Everything else about a child is as `.child(entity)`: these drive a root
//! with two children, each a text, a button and maybe a child of its own,
//! and read what each tick lowered (`TickReport::lowered`,
//! `TestAppContext::lowered`) and what crossed.
use super::*;
use crate::testing::TickReport;
use crate::{Subscription, deferred, uniform_list};
use gpui::{ScrollStrategy, StyleRefinement};

/// The box a cached child is kept in.
fn boxed(width: f32) -> StyleRefinement {
    StyleRefinement::default().w(px(width)).h_full()
}

/// A child: a line of text, a button that counts presses, and maybe a
/// child of its own, a uniform list, a variable list or a picture.
pub(super) struct Leaf {
    name: &'static str,
    text: String,
    presses: usize,
    /// A cached child of its own.
    pub(super) inner: Option<Entity<Leaf>>,
    rows: usize,
    scroll: UniformListScrollHandle,
    list: Option<ListState>,
    pub(super) picture: Option<Vec<u8>>,
    /// A field it shows, when it shows one.
    field: Option<TextField>,
    /// Shows a second button whose handler moves the text and forgets
    /// `cx.notify()`.
    silent: bool,
    /// Heard the root render once ([`Rendered`]).
    heard: bool,
    subscription: Option<Subscription>,
}

impl Leaf {
    pub(super) fn new(name: &'static str) -> Self {
        Self {
            name,
            text: format!("{name} text"),
            presses: 0,
            inner: None,
            rows: 0,
            scroll: UniformListScrollHandle::new(),
            list: None,
            picture: None,
            field: None,
            silent: false,
            heard: false,
            subscription: None,
        }
    }
}

impl Render for Leaf {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let press = cx.listener(|leaf, _: &ClickEvent, _, cx| {
            leaf.presses += 1;
            cx.notify();
        });
        let name = self.name;
        div()
            .id(name)
            .size_full()
            .child(self.text.clone())
            .child(
                div()
                    .id(format!("{name}-press"))
                    .role(Role::Button)
                    .focusable()
                    .on_click(press)
                    .child("press"),
            )
            .when(self.silent, |el| {
                el.child(
                    div()
                        .id(format!("{name}-silent"))
                        .role(Role::Button)
                        .focusable()
                        .on_click(cx.listener(|leaf, _: &ClickEvent, _, _| {
                            leaf.text = "moved in silence".into();
                        }))
                        .child("silent"),
                )
            })
            .when_some(self.inner.clone(), |el, inner| {
                el.child(inner.cached(boxed(100.)))
            })
            .when(self.rows > 0, |el| {
                el.child(
                    uniform_list(format!("{name}-rows"), self.rows, |range, _, _| {
                        range.map(|row| format!("row {row}")).collect::<Vec<_>>()
                    })
                    .track_scroll(&self.scroll)
                    .h(px(200.)),
                )
            })
            .when_some(self.list.clone(), |el, state| {
                el.child(
                    list(format!("{name}-list"), state, |row, _, _| {
                        format!("item {row}").into_any_element()
                    })
                    .h(px(200.)),
                )
            })
            .when_some(self.picture.clone(), |el, bytes| {
                el.child(img(Arc::new(Image::from_bytes(ImageFormat::Png, bytes))))
            })
            .when_some(self.field.clone(), |el, field| {
                el.child(
                    Input::new(format!("{name}-input"), "Input")
                        .value(&field)
                        .on_change(cx.listener(|_, _: &wire::TextChange, _, _| {})),
                )
            })
    }
}

/// The root rendered: what a child may subscribe to.
struct Rendered;
impl EventEmitter<Rendered> for Shell {}

/// Where the root places its two children.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, Default)]
enum Layout {
    /// `a` and `b` cached, in order.
    #[default]
    Flat,
    /// `b` then `a`.
    Swapped,
    /// `a`'s box inside a deferred draw.
    DeferredA,
    /// `a` under another identified element.
    MovedA,
    /// `a` plain, `b` cached.
    PlainA,
    /// `a` cached twice.
    TwiceCached,
    /// `a` cached, then plain.
    CachedThenPlain,
    /// `a` plain twice, the second copy under another identified element
    /// (two siblings of one id the host refuses).
    TwicePlain,
    /// `a` cached as a uniform list's row root.
    RowRoot,
}

#[derive(Serialize, Deserialize)]
pub(super) struct Shell {
    text: String,
    /// The width of `a`'s box.
    width: f32,
    layout: Layout,
    /// A width the root draws a div at, to bring a new style a turn.
    styles: usize,
    /// Emits [`Rendered`] from its render.
    emits: bool,
    /// A fact pushed into `a` from the render, before `a` is placed.
    push: Option<String>,
    #[serde(skip)]
    pub(super) a: Option<Entity<Leaf>>,
    #[serde(skip)]
    pub(super) b: Option<Entity<Leaf>>,
    #[serde(skip)]
    pub(super) picture: Option<Vec<u8>>,
}

impl Default for Shell {
    fn default() -> Self {
        Self {
            text: "shell text".into(),
            width: 200.,
            layout: Layout::Flat,
            styles: 0,
            emits: false,
            push: None,
            a: None,
            b: None,
            picture: None,
        }
    }
}

impl View for Shell {
    const NAME: &'static str = "Shell";
    const CAPABILITIES: &'static [wire::methods::Capability] = &[wire::methods::Capability::Host];
    fn attach(&mut self, _: &mut Window, cx: &mut Context<Self>) {
        self.a = Some(cx.new(|_| Leaf::new("a")));
        self.b = Some(cx.new(|_| Leaf::new("b")));
    }
}

impl Render for Shell {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        if self.emits {
            cx.emit(Rendered);
        }
        let a = self.a.clone().expect("attach built it");
        let b = self.b.clone().expect("attach built it");
        if let Some(text) = &self.push {
            a.update(cx, |a, cx| {
                if a.text != *text {
                    a.text = text.clone();
                    cx.notify();
                }
            });
        }
        let root = div()
            .id("shell")
            .size_full()
            .child(self.text.clone())
            .when(self.styles > 0, |el| {
                el.child(div().w(px(self.styles as f32)))
            })
            .when_some(self.picture.clone(), |el, bytes| {
                el.child(img(Arc::new(Image::from_bytes(ImageFormat::Png, bytes))))
            });
        let box_a = || boxed(self.width);
        match self.layout {
            Layout::Flat => root.child(a.cached(box_a())).child(b.cached(boxed(200.))),
            Layout::Swapped => root.child(b.cached(boxed(200.))).child(a.cached(box_a())),
            Layout::DeferredA => root
                .child(deferred(a.cached(box_a())))
                .child(b.cached(boxed(200.))),
            Layout::MovedA => root
                .child(div().id("elsewhere").child(a.cached(box_a())))
                .child(b.cached(boxed(200.))),
            Layout::PlainA => root.child(a).child(b.cached(boxed(200.))),
            Layout::TwiceCached => root
                .child(a.clone().cached(box_a()))
                .child(a.cached(box_a())),
            Layout::CachedThenPlain => root.child(a.clone().cached(box_a())).child(a),
            Layout::TwicePlain => root.child(a.clone()).child(div().id("other").child(a)),
            Layout::RowRoot => root.child(uniform_list("rows", 1, move |_, _, _| {
                vec![a.clone().cached(boxed(200.))]
            })),
        }
    }
}

/// A root with its two cached children on screen.
fn opened_shell() -> (TestAppContext, Entity<Shell>, Entity<Leaf>, Entity<Leaf>) {
    let (cx, root) = opened::<Shell>();
    let (a, b) = root.read(|shell| (shell.a.clone().unwrap(), shell.b.clone().unwrap()));
    (cx, root, a, b)
}

/// `cx.lowered` of the root and both children.
fn counts(
    cx: &TestAppContext,
    root: &Entity<Shell>,
    a: &Entity<Leaf>,
    b: &Entity<Leaf>,
) -> [u64; 3] {
    [cx.lowered(root), cx.lowered(a), cx.lowered(b)]
}

/// The nodes a `Leaf` with nothing but its text and button lowers to.
const LEAF_NODES: usize = 4;
/// The root's own nodes with two cached children: its div, its text and
/// the two boxes.
const SHELL_OWN: usize = 4;

fn paths(cx: &TestAppContext) -> Vec<Vec<u32>> {
    cx.last_frame()
        .patches
        .iter()
        .map(|patch| match patch {
            wire::Patch::Replace { path, .. }
            | wire::Patch::Props { path, .. }
            | wire::Patch::Insert { path, .. }
            | wire::Patch::Remove { path, .. }
            | wire::Patch::Move { path, .. } => path.clone(),
        })
        .collect()
}

/// A clean cached child is not rendered, not lowered and not diffed; a
/// notified one is, with the root's own nodes around it.
#[test]
fn a_cached_child_is_not_lowered() {
    let (mut cx, root, a, b) = opened_shell();
    let before = counts(&cx, &root, &a, &b);
    cx.update(&a, |a, _, cx| {
        a.text = "a changed".into();
        cx.notify();
    });
    let report = cx.tick(vec![]);
    assert!(report.rendered);
    assert_eq!(report.lowered, SHELL_OWN + LEAF_NODES, "{report:?}");
    let after = counts(&cx, &root, &a, &b);
    assert_eq!(after, [before[0] + 1, before[1] + 1, before[2]]);
    assert!(
        paths(&cx).iter().all(|path| path.starts_with(&[1])),
        "{:?}",
        paths(&cx)
    );
    // the root alone: both children stand in, and still show
    cx.update(&root, |shell, _, cx| {
        shell.text = "shell changed".into();
        cx.notify();
    });
    let report = cx.tick(vec![]);
    assert_eq!(report.lowered, SHELL_OWN, "{report:?}");
    assert_eq!(
        counts(&cx, &root, &a, &b),
        [after[0] + 1, after[1], after[2]]
    );
    assert!(cx.has_text("a changed") && cx.has_text("b text") && cx.has_text("shell changed"));
    assert_eq!(paths(&cx), [[0u32]]);
}

/// `.child(entity)` is what it was: the child renders and lowers whenever
/// its parent does. A pin of the shape, not a regression test.
#[test]
fn an_uncached_child_lowers_with_its_parent() {
    let mut cx = TestAppContext::new();
    let root = cx.open::<Shell>();
    cx.update(&root, |shell, _, cx| {
        shell.layout = Layout::PlainA;
        cx.notify();
    });
    cx.run_until_parked();
    let (a, b) = root.read(|shell| (shell.a.clone().unwrap(), shell.b.clone().unwrap()));
    let before = counts(&cx, &root, &a, &b);
    cx.update(&root, |shell, _, cx| {
        shell.text = "again".into();
        cx.notify();
    });
    let report = cx.tick(vec![]);
    assert_eq!(
        counts(&cx, &root, &a, &b),
        [before[0] + 1, before[1] + 1, before[2]]
    );
    assert_eq!(report.lowered, SHELL_OWN - 1 + LEAF_NODES, "{report:?}");
}

/// A root whose child `a` holds a cached grandchild `g`, after one frame
/// that lowered only the root.
fn nested() -> (
    TestAppContext,
    Entity<Shell>,
    Entity<Leaf>,
    Entity<Leaf>,
    Entity<Leaf>,
) {
    let (mut cx, root, a, b) = opened_shell();
    let g = cx.update(&a, |a, _, cx| {
        let g = cx.new(|_| Leaf::new("g"));
        a.inner = Some(g.clone());
        cx.notify();
        g
    });
    cx.run_until_parked();
    cx.update(&root, |shell, _, cx| {
        shell.text = "root only".into();
        cx.notify();
    });
    let report = cx.tick(vec![]);
    assert_eq!(report.lowered, SHELL_OWN, "a root-only frame: {report:?}");
    (cx, root, a, b, g)
}

/// A notified grandchild under a clean cached parent is lowered: its
/// notify marks the parent and the root on the way up.
#[test]
fn a_dirty_grandchild_dirties_its_ancestors() {
    let (mut cx, root, a, b, g) = nested();
    let before = counts(&cx, &root, &a, &b);
    let g_before = cx.lowered(&g);
    cx.update(&g, |g, _, cx| {
        g.text = "g changed".into();
        cx.notify();
    });
    let report = cx.tick(vec![]);
    assert!(report.rendered);
    assert_eq!(cx.lowered(&g), g_before + 1);
    assert_eq!(
        counts(&cx, &root, &a, &b),
        [before[0] + 1, before[1] + 1, before[2]]
    );
    assert!(cx.has_text("g changed"), "{:?}", cx.texts());
}

/// A kept grandchild's routes live while it stands under a stand-in.
#[test]
fn a_kept_child_under_a_stand_in_keeps_its_routes() {
    let (mut cx, root, _, _, g) = nested();
    for n in 0..3 {
        cx.update(&root, |shell, _, cx| {
            shell.text = format!("root {n}");
            cx.notify();
        });
        cx.tick(vec![]);
    }
    cx.simulate_click("g-press");
    assert_eq!(g.read(|g| g.presses), 1);
}

/// A root whose cached child's root is an id-less element with a mouse-move
/// listener, beside an id-less sibling of the root's with one too (a move
/// is a listener the audit lets an id-less element carry).
#[derive(Default, Serialize, Deserialize)]
struct Bare {
    text: String,
    hovers: usize,
    #[serde(skip)]
    child: Option<Entity<BareChild>>,
}
struct BareChild {
    hovers: usize,
}
impl Render for BareChild {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .size_full()
            .on_mouse_move(cx.listener(|child, _: &gpui::MouseMoveEvent, _, cx| {
                child.hovers += 1;
                cx.notify();
            }))
            .child("child")
    }
}
impl View for Bare {
    const NAME: &'static str = "Bare";
    fn attach(&mut self, _: &mut Window, cx: &mut Context<Self>) {
        self.child = Some(cx.new(|_| BareChild { hovers: 0 }));
    }
}
impl Render for Bare {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let child = self.child.clone().expect("attach built it");
        div()
            .id("x")
            .child(self.text.clone())
            .child(child.cached(boxed(100.)))
            .child(
                div()
                    .on_mouse_move(cx.listener(|bare, _: &gpui::MouseMoveEvent, _, cx| {
                        bare.hovers += 1;
                        cx.notify();
                    }))
                    .child("sibling"),
            )
    }
}

/// The mouse-move route of the id-less node at `path` (child indices).
fn route_at(cx: &TestAppContext, path: &[usize]) -> u32 {
    let mut node = cx.root();
    for index in path {
        node = &node.children()[*index];
    }
    node.interactivity()
        .and_then(|i| i.on_mouse_move)
        .unwrap_or_else(|| panic!("no mouse-move route at {path:?}: {node:?}"))
}

/// A kept id-less child's listener and its parent's next id-less sibling's
/// listener of the same kind keep routes of their own: the boundary is a
/// route scope, filed under its owner.
#[test]
fn an_id_less_child_and_an_id_less_sibling_keep_their_own_routes() {
    let mut cx = TestAppContext::new();
    let root = cx.open::<Bare>();
    let child = root.read(|bare| bare.child.clone().unwrap());
    let (child_route, sibling_route) = (route_at(&cx, &[1, 0]), route_at(&cx, &[2]));
    assert_ne!(child_route, sibling_route);
    cx.update(&root, |bare, _, cx| {
        bare.text = "root only".into();
        cx.notify();
    });
    cx.tick(vec![]);
    assert_eq!(
        (route_at(&cx, &[1, 0]), route_at(&cx, &[2])),
        (child_route, sibling_route),
        "the routes did not change hands"
    );
    let moved = || wire::interactivity::MouseMove::from(&gpui::MouseMoveEvent::default());
    cx.simulate_event(wire::Event::MouseMove {
        handler: child_route,
        phase: wire::DispatchPhase::Bubble,
        event: moved(),
    });
    assert_eq!(
        child.read(|child| child.hovers),
        1,
        "the child's handler ran"
    );
    assert_eq!(root.read(|bare| bare.hovers), 0);
    cx.simulate_event(wire::Event::MouseMove {
        handler: sibling_route,
        phase: wire::DispatchPhase::Bubble,
        event: moved(),
    });
    assert_eq!(
        root.read(|bare| bare.hovers),
        1,
        "the sibling's handler ran"
    );
}

/// A kept child placed under another identified element is lowered fresh
/// there: a subtree is reused only at the path it was kept at.
#[test]
fn a_kept_child_moved_to_another_path_is_lowered_fresh() {
    let (mut cx, root, a, b) = opened_shell();
    let before = counts(&cx, &root, &a, &b);
    cx.update(&root, |shell, _, cx| {
        shell.layout = Layout::MovedA;
        cx.notify();
    });
    let report = cx.tick(vec![]);
    assert_eq!(
        counts(&cx, &root, &a, &b),
        [before[0] + 1, before[1] + 1, before[2]]
    );
    assert_eq!(report.lowered, SHELL_OWN + 1 + LEAF_NODES, "{report:?}");
    assert!(cx.find("elsewhere").is_some() && cx.find("a").is_some());
    cx.simulate_click("a-press");
    assert_eq!(a.read(|a| a.presses), 1);
}

/// One entity placed cached, then plain, then cached: the slot's new
/// subtree crosses each way (a `Remove` and an `Insert` at the slot, as the
/// differ sends a keyed child whose key changed among its siblings), the
/// kept entry freed and made again, the button live throughout.
#[test]
fn cached_then_plain_then_cached() {
    let (mut cx, root, a, _) = opened_shell();
    for (presses, layout) in [(1, Layout::PlainA), (2, Layout::Flat), (3, Layout::PlainA)] {
        cx.update(&root, |shell, _, cx| {
            shell.layout = layout;
            cx.notify();
        });
        cx.tick(vec![]);
        let patches = &cx.last_frame().patches;
        assert!(
            matches!(
                &patches[..],
                [wire::Patch::Remove { path: at, index: 1 }, wire::Patch::Insert { path, index: 1, .. }]
                    if at.is_empty() && path.is_empty()
            ),
            "{layout:?}: {patches:#?}"
        );
        cx.simulate_click("a-press");
        assert_eq!(a.read(|a| a.presses), presses);
    }
}

/// Two cached children reordered are one `Move`, both subtrees whole.
#[test]
fn a_moved_child_is_a_move() {
    let (mut cx, root, _, _) = opened_shell();
    cx.update(&root, |shell, _, cx| {
        shell.layout = Layout::Swapped;
        cx.notify();
    });
    let report = cx.tick(vec![]);
    assert_eq!(report.lowered, SHELL_OWN, "{report:?}");
    let patches = &cx.last_frame().patches;
    assert!(
        matches!(&patches[..], [wire::Patch::Move { path, from: 2, to: 1 }] if path.is_empty()),
        "{patches:#?}"
    );
    assert!(cx.find("a").is_some() && cx.find("b").is_some());
    assert_eq!(
        cx.root().children()[1].children()[0].identity(),
        cx.find("b").unwrap().identity()
    );
}

/// A kind change above a kept child (its box wrapped in a deferred draw)
/// sends the new node, carrying the kept content whole: an identified
/// node becoming an id-less one is, as it was, a `Remove` and an `Insert`
/// at the slot (the differ matches a kind change among keyed siblings by
/// key), so the carrier is the `Insert`.
#[test]
fn a_kind_change_above_a_kept_child_carries_it_whole() {
    let (mut cx, root, a, b) = opened_shell();
    let before = counts(&cx, &root, &a, &b);
    cx.update(&root, |shell, _, cx| {
        shell.layout = Layout::DeferredA;
        cx.notify();
    });
    cx.tick(vec![]);
    assert_eq!(
        counts(&cx, &root, &a, &b),
        [before[0] + 1, before[1], before[2]]
    );
    let patches = &cx.last_frame().patches;
    let carried: Vec<&wire::Node> = patches
        .iter()
        .filter_map(|patch| match patch {
            wire::Patch::Replace { path, node } | wire::Patch::Insert { path, node, .. }
                if path.is_empty() || path == &[1] =>
            {
                Some(node)
            }
            _ => None,
        })
        .collect();
    assert!(
        matches!(&carried[..], [node] if node.count() == 2 + LEAF_NODES),
        "{patches:#?}"
    );
    assert!(cx.has_text("a text"));
}

/// A new box style is one `Props` on the `View`, nothing below it.
#[test]
fn a_box_style_change_is_one_props() {
    let (mut cx, root, a, b) = opened_shell();
    let before = counts(&cx, &root, &a, &b);
    cx.update(&root, |shell, _, cx| {
        shell.width = 240.;
        cx.notify();
    });
    cx.tick(vec![]);
    assert_eq!(
        counts(&cx, &root, &a, &b),
        [before[0] + 1, before[1], before[2]]
    );
    let patches = &cx.last_frame().patches;
    assert!(
        matches!(&patches[..], [wire::Patch::Props { path, node: wire::Node::View { .. } }] if path == &[1]),
        "{patches:#?}"
    );
    let wire::Node::View { style, .. } = &cx.root().children()[1] else {
        panic!("{:?}", cx.root());
    };
    assert_eq!(cx.styles()[*style].size.width, Some(px(240.).into()));
}

/// `window.focus(..)` on a field inside a kept child goes out with the
/// whole path: the command resolves against the filled tree.
#[test]
fn focus_into_a_kept_child_resolves() {
    use crate::methods::HostWidget;
    let (mut cx, root, a, _) = opened_shell();
    cx.update(&a, |a, _, cx| {
        a.field = Some(TextField::new(""));
        cx.notify();
    });
    cx.run_until_parked();
    // asked in the tick that renders the root alone: the field is in the
    // kept subtree the frame fills in, not in what the root lowered
    cx.update(&root, |shell, window, cx| {
        shell.text = "root only".into();
        cx.notify();
        window.focus("a-input");
    });
    let report = cx.tick(vec![]);
    assert_eq!(report.lowered, SHELL_OWN, "{report:?}");
    let name = |name: &str| wire::ElementIdWire::Name(name.into());
    assert_eq!(
        cx.host().requests::<HostWidget>().last(),
        Some(&wire::WidgetCommand::Focus {
            target: vec![name("shell"), name("a"), name("a-input")]
        })
    );
}

/// A root whose cached child `a` holds a cached grandchild `g` with a
/// uniform list of `rows` rows, after a root-only frame.
fn nested_rows(
    rows: usize,
) -> (
    TestAppContext,
    Entity<Shell>,
    Entity<Leaf>,
    Entity<Leaf>,
    Entity<Leaf>,
) {
    let (mut cx, root, a, b, g) = nested();
    cx.update(&g, |g, _, cx| {
        g.rows = rows;
        cx.notify();
    });
    cx.run_until_parked();
    cx.simulate_viewport(8);
    cx.update(&root, |shell, _, cx| {
        shell.text = "root only again".into();
        cx.notify();
    });
    let report = cx.tick(vec![]);
    assert_eq!(report.lowered, SHELL_OWN, "{report:?}");
    (cx, root, a, b, g)
}

/// The host's word that moves a list window reaches the list's owner, a
/// cached grandchild under a clean cached parent: the owner and the root's
/// own nodes lower, the sibling does not, and the rows arrive.
#[test]
fn a_host_word_reaches_a_nested_lists_owner() {
    let (mut cx, root, a, b, g) = nested_rows(1_000);
    let before = counts(&cx, &root, &a, &b);
    let g_before = cx.lowered(&g);
    cx.simulate_range("g-rows", 500..508);
    assert_eq!(cx.lowered(&g), g_before + 1);
    assert_eq!(
        counts(&cx, &root, &a, &b),
        [before[0] + 1, before[1] + 1, before[2]]
    );
    assert!(
        cx.has_text("row 500") && cx.has_text("row 507"),
        "{:?}",
        cx.texts()
    );
}

/// A host word inside the window, and a new scroll state, lower nothing:
/// S8's rule, kept. A pin that passes at dev.
#[test]
fn a_notch_inside_the_window_lowers_nothing() {
    let (mut cx, _, _, _, _) = nested_rows(1_000);
    cx.simulate_range("g-rows", 2..10);
    assert!(
        cx.reports()
            .iter()
            .all(|report| !report.rendered && report.lowered == 0),
        "{:?}",
        cx.reports()
    );
}

/// A `ListRequest` on a variable list inside a kept grandchild lowers that
/// entity, not its sibling.
#[test]
fn a_list_request_re_lowers_its_owner() {
    let (mut cx, root, a, b, g) = nested();
    cx.update(&g, |g, _, cx| {
        g.list = Some(ListState::new(500, ListAlignment::Top, px(20.)));
        cx.notify();
    });
    cx.run_until_parked();
    cx.update(&root, |shell, _, cx| {
        shell.text = "root only again".into();
        cx.notify();
    });
    cx.tick(vec![]);
    let before = counts(&cx, &root, &a, &b);
    let g_before = cx.lowered(&g);
    cx.simulate_range("g-list", 300..312);
    assert_eq!(cx.lowered(&g), g_before + 1);
    assert_eq!(
        counts(&cx, &root, &a, &b),
        [before[0] + 1, before[1] + 1, before[2]]
    );
    assert!(cx.has_text("item 300"), "{:?}", cx.texts());
}

/// A fact the root pushes into a cached child from its own render, before
/// the child's box is placed: the child renders in that frame (as gpui
/// paints a view notified before its cached element is reached), the frame
/// is not busy, and the same fact pushed again moves nothing.
#[test]
fn a_fact_pushed_in_the_parents_render_lowers_the_child_in_that_frame() {
    let (mut cx, root, a, b) = opened_shell();
    let before = counts(&cx, &root, &a, &b);
    cx.update(&root, |shell, _, cx| {
        shell.push = Some("pushed".into());
        cx.notify();
    });
    let report = cx.tick(vec![]);
    assert!(report.rendered && !report.busy, "{report:?}");
    assert_eq!(report.lowered, SHELL_OWN + LEAF_NODES, "{report:?}");
    assert_eq!(
        counts(&cx, &root, &a, &b),
        [before[0] + 1, before[1] + 1, before[2]]
    );
    assert!(cx.has_text("pushed"), "{:?}", cx.texts());
    let report = cx.tick(vec![]);
    assert!(!report.rendered, "answered in its frame: {report:?}");
    cx.update(&root, |shell, _, cx| {
        shell.text = "root again".into();
        cx.notify();
    });
    let report = cx.tick(vec![]);
    assert_eq!(report.lowered, SHELL_OWN, "the same fact: {report:?}");
    assert_eq!(cx.lowered(&a), before[1] + 1);
}

/// A hearer that notifies a kept child while the root's render ends: the
/// tick's check does not fault it (its notify is pending), the frame says
/// busy, and the next tick lowers the child.
#[test]
fn a_notify_raised_during_a_render_passes_the_check_and_lowers_next_tick() {
    let (mut cx, root, a, _) = opened_shell();
    cx.update(&a, |leaf, _, cx| {
        leaf.subscription = Some(cx.subscribe(&root, |leaf, _, Rendered: &Rendered, cx| {
            if !leaf.heard {
                leaf.heard = true;
                leaf.text = "heard".into();
                cx.notify();
            }
        }));
    });
    cx.update(&root, |shell, _, cx| {
        shell.emits = true;
        cx.notify();
    });
    let report = cx.tick(vec![]);
    assert!(report.busy && report.lowered == SHELL_OWN, "{report:?}");
    let a_before = cx.lowered(&a);
    let report = cx.tick(vec![]);
    assert!(report.rendered, "{report:?}");
    assert_eq!(cx.lowered(&a), a_before + 1);
    assert!(cx.has_text("heard"));
}

/// A kept child survives a style table that outgrew the host's: the root
/// draws a new style every tick until the table starts over, and the whole
/// frame that restarts it holds both children's content with renumbered
/// ids, no empty node in their place, the children themselves not lowered.
#[test]
fn a_kept_child_survives_an_outgrown_table() {
    let (mut cx, root, a, b) = opened_shell();
    let before = counts(&cx, &root, &a, &b);
    let mut wholes = Vec::new();
    for turn in 1..=wire::MAX_STYLES {
        cx.update(&root, |shell, _, cx| {
            shell.styles = turn;
            cx.notify();
        });
        cx.tick(vec![]);
        if cx.last_frame().root.is_some() {
            wholes.push(turn);
        }
    }
    assert_eq!(wholes.len(), 1, "the table started over once: {wholes:?}");
    assert_eq!(counts(&cx, &root, &a, &b)[1..], before[1..]);
    assert!(cx.has_text("a text") && cx.has_text("b text"));
    let mut spaces = 0;
    cx.root().clone().for_each_mut(&mut |node| {
        spaces += usize::from(matches!(node, wire::Node::Space));
    });
    assert_eq!(spaces, 0);
    let table = cx.styles().len();
    cx.root().clone().for_each_mut(&mut |node| {
        node.styles_mut(&mut |id| assert!((id.0 as usize) < table, "{id:?} past {table}"));
    });
    cx.simulate_click("a-press");
    assert_eq!(a.read(|a| a.presses), 1);
}

/// A render that diffs to nothing leaves a whole base: the next notify of a
/// kept child diffs against its content, not a hollow box.
#[test]
fn an_unchanged_render_keeps_a_whole_base() {
    let (mut cx, root, a, _) = opened_shell();
    cx.update(&root, |_, _, cx| cx.notify());
    let report = cx.tick(vec![]);
    assert!(report.rendered && report.patches == 0 && cx.last_frame().unchanged);
    cx.update(&a, |a, _, cx| {
        a.text = "a changed".into();
        cx.notify();
    });
    cx.tick(vec![]);
    assert!(
        matches!(&cx.last_frame().patches[..], [wire::Patch::Props { path, .. }] if path == &[1, 0, 0]),
        "{:#?}",
        cx.last_frame().patches
    );
    assert!(cx.has_text("a changed"));
}

/// A kept child whose state moved without `cx.notify()` is named by the
/// debug check on the next tick, rendered or not.
#[test]
#[should_panic(expected = "Leaf changed without cx.notify()")]
fn a_child_changed_without_notify_is_caught() {
    let (mut cx, _, a, _) = opened_shell();
    cx.update(&a, |a, _, _| a.text = "sneaky".into());
    cx.tick(vec![]);
}

/// A scroll asked of a kept list without a notify is caught, and the ask
/// is still there for the frame after the fix.
#[test]
fn a_scroll_asked_without_notify_is_caught() {
    let (mut cx, _, _, _, g) = nested_rows(1_000);
    let handle = g.read(|g| g.scroll.clone());
    handle.scroll_to_item(900, ScrollStrategy::Top);
    let caught = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| cx.tick(vec![])));
    let message = caught
        .err()
        .and_then(|payload| payload.downcast_ref::<String>().cloned())
        .unwrap_or_default();
    assert!(
        message.contains("Leaf changed without cx.notify()"),
        "{message}"
    );
    // the fix: a notify. The ask is still there for the frame that carries it
    cx.update(&g, |_, _, cx| cx.notify());
    cx.tick(vec![]);
    let wire::Node::UniformList { scroll_request, .. } = cx.node("g-rows") else {
        panic!("{:?}", cx.node("g-rows"));
    };
    assert_eq!(scroll_request.map(|request| request.index), Some(900));
    drop(handle);
}

/// A host word inside a kept list's window, and a new scroll state, pass
/// the check: the registry's window is what the base shows.
#[test]
fn an_in_window_scroll_of_a_kept_list_passes_the_check() {
    let (mut cx, _, _, _, _) = nested_rows(1_000);
    cx.simulate_range("g-rows", 2..10);
    let report = cx.tick(vec![]);
    assert!(!report.rendered && report.lowered == 0, "{report:?}");
}

/// A handler that moves a kept child's state and forgets `cx.notify()`:
/// the tick renders nothing, and the check names the child there.
#[test]
#[should_panic(expected = "Leaf changed without cx.notify()")]
fn the_check_runs_on_a_tick_without_a_render() {
    let (mut cx, root, a, _) = opened_shell();
    cx.update(&a, |a, _, cx| {
        a.silent = true;
        cx.notify();
    });
    cx.run_until_parked();
    cx.update(&root, |shell, _, cx| {
        shell.text = "root only".into();
        cx.notify();
    });
    cx.tick(vec![]);
    let handler = cx.interactivity("a-silent").on_click.unwrap();
    let report = cx.tick(vec![wire::Event::Click {
        handler,
        event: (&ClickEvent::default()).into(),
    }]);
    unreachable!("the check panics in the tick that rendered nothing: {report:?}");
}

/// The check writes nothing: a tick that only checks leaves the next tick
/// nothing to lower, the routes and the style table as they were.
#[test]
fn the_check_writes_nothing() {
    let (mut cx, root, a, _, _) = nested_rows(1_000);
    let (styles, tree) = (cx.styles().len(), cx.root().clone());
    let report = cx.tick(vec![]);
    assert!(!report.rendered && report.lowered == 0, "{report:?}");
    let report = cx.tick(vec![]);
    assert!(!report.rendered && report.lowered == 0, "{report:?}");
    assert_eq!(cx.styles().len(), styles);
    assert_eq!(*cx.root(), tree);
    cx.update(&root, |shell, _, cx| {
        shell.text = "after the checks".into();
        cx.notify();
    });
    cx.tick(vec![]);
    assert_eq!(
        paths(&cx),
        [[0u32]],
        "nothing under the kept children moved"
    );
    cx.simulate_click("a-press");
    assert_eq!(
        a.read(|a| a.presses),
        1,
        "the routes are the ones the host holds"
    );
}

/// An entity with a cached placement is placed once per frame: twice
/// cached, or cached and plain, panics naming both paths; a cached entity
/// as a list row's root panics; plain twice lowers two copies, as before.
#[test]
fn an_entity_is_one_element() {
    for layout in [Layout::TwiceCached, Layout::CachedThenPlain] {
        let caught = std::panic::catch_unwind(|| {
            let mut cx = TestAppContext::new();
            let root = cx.open::<Shell>();
            cx.update(&root, |shell, _, cx| {
                shell.layout = layout;
                cx.notify();
            });
            cx.run_until_parked();
        });
        let message = caught
            .err()
            .and_then(|payload| payload.downcast_ref::<String>().cloned())
            .unwrap_or_default();
        assert!(
            message.contains("Leaf is one element: it is a child twice"),
            "{message}"
        );
    }
    let caught = std::panic::catch_unwind(|| {
        let mut cx = TestAppContext::new();
        let root = cx.open::<Shell>();
        cx.update(&root, |shell, _, cx| {
            shell.layout = Layout::RowRoot;
            cx.notify();
        });
        cx.run_until_parked();
    });
    let message = caught
        .err()
        .and_then(|payload| payload.downcast_ref::<String>().cloned())
        .unwrap_or_default();
    assert!(
        message.contains("Leaf is cached as a list row's root"),
        "{message}"
    );
    let mut cx = TestAppContext::new();
    let root = cx.open::<Shell>();
    cx.update(&root, |shell, _, cx| {
        shell.layout = Layout::TwicePlain;
        cx.notify();
    });
    cx.run_until_parked();
    assert_eq!(
        cx.texts().iter().filter(|text| *text == "a text").count(),
        2,
        "{:?}",
        cx.texts()
    );
}

/// A resync after kept children: the next frame is whole and complete.
#[test]
fn a_resync_after_kept_children_sends_a_whole_frame() {
    let (mut cx, root, a, b) = opened_shell();
    cx.update(&root, |shell, _, cx| {
        shell.text = "root only".into();
        cx.notify();
    });
    cx.tick(vec![]);
    let before = counts(&cx, &root, &a, &b);
    cx.simulate_resync();
    let frame = cx.last_frame();
    assert!(frame.root.is_some() && frame.patches.is_empty());
    assert!(cx.has_text("a text") && cx.has_text("b text") && cx.has_text("root only"));
    assert_eq!(counts(&cx, &root, &a, &b), before.map(|n| n + 1));
    let _: TickReport = cx.tick(vec![]);
}
