use super::*;
use futures::StreamExt;
use gpui::{Image, ImageFormat};
use serde::{Deserialize, Serialize};
use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::sync::Arc;

use crate::testing::TestAppContext;
use crate::wire::Event;

#[derive(Default, Serialize, Deserialize)]
struct Probe {
    received: Vec<String>,
    #[serde(skip)]
    streams: Vec<Task<()>>,
}
impl Probe {
    fn watch(&mut self, cx: &mut Context<Self>, kind: &'static str) {
        let mut stream = cx.host().raw_subscribe(kind, &[]);
        self.streams.push(cx.spawn(async move |this, cx| {
            while let Some(reply) = stream.next().await {
                reply.unwrap();
                if this
                    .update(cx, |view, cx| {
                        view.received.push(kind.into());
                        cx.notify();
                    })
                    .is_err()
                {
                    break;
                }
            }
        }));
    }
}
impl View for Probe {
    const NAME: &'static str = "Probe";
    const CAPABILITIES: &'static [wire::methods::Capability] = &[wire::methods::Capability::Host];
    fn attach(&mut self, _: &mut Window, cx: &mut Context<Self>) {
        self.watch(cx, "visible");
        self.watch(cx, "data");
    }
}
impl Render for Probe {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let press = cx.listener(|view, _: &ClickEvent, _, cx| {
            view.received.push("press".into());
            cx.notify();
        });
        div()
            .id("press")
            .role(crate::Role::Button)
            .focusable()
            .on_click(press)
            .child(self.received.len().to_string())
    }
}
fn response(id: u64, done: bool) -> wire::Event {
    wire::Event::Response {
        id,
        result: Ok(Vec::new()),
        done,
    }
}
fn opened<V: View>() -> (TestAppContext, Entity<V>) {
    let mut cx = TestAppContext::new();
    let view = cx.open::<V>();
    (cx, view)
}

#[test]
fn held_streams_remain_open_and_dropping_tasks_cancels_them() {
    let (mut cx, probe) = opened::<Probe>();
    let first = cx.last_frame().requests.clone();
    assert_eq!(first.len(), 2);
    let id = first[0].id;
    cx.tick(vec![]);
    assert!(cx.last_frame().requests.is_empty());
    cx.tick(vec![response(id, false)]);
    probe.read(|view| assert_eq!(view.received, ["visible"]));
    assert!(
        cx.snapshot().is_ok(),
        "held streams can be restored independently"
    );
    cx.update(&probe, |view, _, cx| {
        view.streams.clear();
        view.watch(cx, "replacement");
    });
    cx.tick(vec![]);
    let frame = cx.last_frame();
    assert_eq!(frame.cancels.len(), 2);
    assert!(frame.cancels.contains(&id));
    assert_eq!(frame.requests.len(), 1);
    assert_eq!(frame.requests[0].kind, "replacement");
}

#[test]
fn snapshot_refuses_unsettled_writes_and_failed_restore_keeps_old_view() {
    use wire::methods::{HostId, Method};
    let (mut cx, _) = opened::<Probe>();
    cx.host().never::<HostId>();
    cx.app_mut()
        .spawn(async move |cx| {
            cx.host().ask::<HostId>("write".into()).await.unwrap();
        })
        .detach();
    cx.tick(vec![]);
    assert!(cx.snapshot().is_err());
    let write = cx
        .last_frame()
        .requests
        .iter()
        .find(|request| request.kind == HostId::KIND)
        .unwrap()
        .id;
    cx.tick(vec![Event::Response {
        id: write,
        result: Ok(HostId::encode_reply(&"write-1".into())),
        done: true,
    }]);
    let snapshot = cx.snapshot().unwrap();
    assert!(cx.restore::<Probe>(&[]).is_err());
    assert_eq!(cx.snapshot().unwrap(), snapshot);
    cx.restore::<Probe>(&snapshot).unwrap();
    assert_eq!(cx.last_frame().requests.len(), 2);
}

#[test]
fn stream_updates_follow_response_order_instead_of_spawn_order() {
    let (mut cx, probe) = opened::<Probe>();
    let requests = cx.last_frame().requests.clone();
    for order in [["data", "visible"], ["visible", "data"]] {
        cx.update(&probe, |view, _, cx| {
            view.received.clear();
            cx.notify();
        });
        let events = order
            .iter()
            .map(|kind| {
                response(
                    requests
                        .iter()
                        .find(|request| request.kind == *kind)
                        .unwrap()
                        .id,
                    false,
                )
            })
            .collect();
        cx.tick(events);
        probe.read(|view| assert_eq!(view.received, order));
    }
}

#[test]
fn unchanged_frames_preserve_listener_tables_and_resync_renders() {
    let (mut cx, probe) = opened::<Probe>();
    assert!(cx.last_frame().root.is_some());
    cx.tick(vec![]);
    let renders = cx.renders();
    for _ in 0..3 {
        cx.tick(vec![]);
        assert!(cx.last_frame().unchanged);
    }
    assert_eq!(cx.renders(), renders);
    cx.simulate_click("press");
    assert!(cx.reports()[0].rendered && cx.reports()[0].patches > 0);
    probe.read(|view| assert_eq!(view.received, ["press"]));
    let renders = cx.renders();
    cx.tick(vec![Event::Resync]);
    assert!(cx.last_frame().root.is_some());
    assert_eq!(cx.renders(), renders + 1);
}

#[test]
fn two_views_on_one_thread_have_independent_hosts() {
    let (mut first, first_probe) = opened::<Probe>();
    let (mut second, second_probe) = opened::<Probe>();
    let first_requests = first.last_frame().requests.clone();
    let second_requests = second.last_frame().requests.clone();
    assert_eq!(first_requests.len(), 2);
    assert_eq!(second_requests.len(), 2);
    assert_eq!(
        first_requests[0].id, second_requests[0].id,
        "each host starts its own ID sequence"
    );
    first.tick(vec![response(first_requests[0].id, false)]);
    second.tick(vec![]);
    first_probe.read(|view| assert_eq!(view.received, ["visible"]));
    second_probe.read(|view| assert!(view.received.is_empty()));
    first.app_mut().host().log("only first");
    second.tick(vec![]);
    assert!(second.last_frame().requests.is_empty());
    first.tick(vec![]);
    let frame = first.last_frame();
    assert_eq!(frame.requests.len(), 1);
    assert_eq!(frame.requests[0].payload, methods::encode(&"only first"));
}

#[test]
fn tasks_are_awaitable_drop_cancels_and_detach_runs() {
    let (mut cx, _) = opened::<Probe>();
    let app = cx.app_mut();
    let order = Rc::new(RefCell::new(Vec::new()));
    let child_order = order.clone();
    let child = app.spawn(async move |_| {
        child_order.borrow_mut().push(1);
        3
    });
    let parent_order = order.clone();
    let parent = app.spawn(async move |_| {
        let value = child.await;
        parent_order.borrow_mut().push(value);
        value + 1
    });
    let canceled_order = order.clone();
    drop(app.spawn(async move |_| {
        canceled_order.borrow_mut().push(99);
    }));
    let detached_order = order.clone();
    app.spawn(async move |_| {
        detached_order.borrow_mut().push(5);
    })
    .detach();
    cx.tick(vec![]);
    assert_eq!(futures::executor::block_on(parent), 4);
    assert_eq!(*order.borrow(), vec![1, 3, 5]);
}

#[derive(Default, Serialize, Deserialize)]
struct UniformProbe;

impl View for UniformProbe {
    const NAME: &'static str = "UniformProbe";
    fn new(_: &mut Window, _: &mut Context<Self>) -> Self {
        Self
    }
}

impl Render for UniformProbe {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        uniform_list("rows", 2_000, |range, _, _| {
            range
                .map(|index| {
                    div()
                        .id(format!("row-{index}"))
                        .child(format!("row {index}"))
                })
                .collect::<Vec<_>>()
        })
    }
}

fn uniform_rows(cx: &TestAppContext) -> (usize, Vec<u32>, usize) {
    match cx.root() {
        wire::Node::UniformList {
            count,
            indices,
            children,
            ..
        } => (*count, indices.clone(), children.len()),
        node => panic!("expected uniform list, got {node:?}"),
    }
}

#[test]
fn uniform_list_lowers_only_initial_and_requested_ranges() {
    let (mut cx, _) = opened::<UniformProbe>();
    let (count, indices, _) = uniform_rows(&cx);
    assert_eq!(count, 2_000);
    assert_eq!(indices, [0]);

    cx.simulate_range("rows", 1_000..1_020);
    let (_, indices, children) = uniform_rows(&cx);
    assert_eq!(indices.len(), 21);
    assert_eq!(children, indices.len());
    assert_eq!(indices.first(), Some(&0));
    assert_eq!(
        &indices[1..],
        (1_000..1_020).map(|index| index as u32).collect::<Vec<_>>()
    );
    assert!(cx.reports()[0].patches <= wire::MAX_PATCHES);

    cx.simulate_range("rows", 1_000..1_020);
    assert!(
        cx.last_frame().unchanged,
        "duplicate range requests do not rerender"
    );

    cx.simulate_range("rows", 0..u32::MAX as usize);
    let (_, indices, _) = uniform_rows(&cx);
    assert_eq!(indices.len(), wire::MAX_UNIFORM_LIST_ROWS);
}

#[test]
fn spawning_from_an_entity_update_settles_without_borrowing_the_view() {
    let (mut cx, probe) = opened::<Probe>();
    cx.update(&probe, |_, _, cx| {
        cx.spawn(async move |this, cx| {
            this.update(cx, |view, cx| {
                view.received.push("first".into());
                cx.notify();
                cx.spawn(async move |this, cx| {
                    this.update(cx, |view, cx| {
                        view.received.push("second".into());
                        cx.notify();
                    })
                    .unwrap();
                })
                .detach();
            })
            .unwrap();
        })
        .detach();
    });
    assert!(!cx.tick(vec![]).busy);
    probe.read(|view| assert_eq!(view.received, ["first", "second"]));
}

#[test]
fn weak_entity_updates_fail_after_the_view_is_released() {
    let (mut cx, probe) = opened::<Probe>();
    let weak = probe.downgrade();
    let saved = Rc::new(RefCell::new(None));
    let captured = saved.clone();
    cx.app_mut()
        .spawn(async move |cx| {
            *captured.borrow_mut() = Some(cx.clone());
        })
        .detach();
    cx.tick(vec![]);
    let mut async_cx = saved.borrow_mut().take().unwrap();
    drop(cx);
    assert!(weak.upgrade().is_none());
    assert_eq!(weak.update(&mut async_cx, |_, _| ()), Err(Released));
}

#[test]
fn a_self_waking_future_is_budgeted_and_keeps_the_frame_busy() {
    let (mut cx, _) = opened::<Probe>();
    let polls = Rc::new(Cell::new(0));
    let counted = polls.clone();
    let task = cx.app_mut().spawn(async move |_| {
        std::future::poll_fn(move |cx| {
            counted.set(counted.get() + 1);
            cx.waker().wake_by_ref();
            std::task::Poll::<()>::Pending
        })
        .await;
    });
    assert!(cx.tick(vec![]).busy);
    assert!(
        polls.get() > 0 && polls.get() <= 4096,
        "poll budget must bound a frame: {}",
        polls.get()
    );
    assert!(cx.snapshot().is_err());
    drop(task);
    assert!(!cx.tick(vec![]).busy);
}

#[cfg(all(debug_assertions, not(target_arch = "wasm32")))]
#[test]
#[should_panic(expected = "state changed without cx.notify()")]
fn update_guard_detects_missing_notify() {
    let (mut cx, probe) = opened::<Probe>();
    cx.update(&probe, |view, _, _| view.received.push("forgot".into()));
}

#[cfg(all(debug_assertions, not(target_arch = "wasm32")))]
#[test]
#[should_panic(expected = "state changed without cx.notify()")]
fn listener_guard_detects_missing_notify() {
    #[derive(Default, Serialize, Deserialize)]
    struct Silent(bool);
    impl View for Silent {
        const NAME: &'static str = "Silent";
        fn new(_: &mut Window, _: &mut Context<Self>) -> Self {
            Self(false)
        }
    }
    impl Render for Silent {
        fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
            let press = cx.listener(|view, _: &ClickEvent, _, _| view.0 = true);
            div()
                .id("silent")
                .role(crate::Role::Button)
                .focusable()
                .on_click(press)
                .child("Silent")
        }
    }
    let (mut cx, _) = opened::<Silent>();
    cx.simulate_click("silent");
}

#[test]
fn messages_and_responses_settle_in_input_order() {
    let (mut cx, probe) = opened::<Probe>();
    let requests = cx.last_frame().requests.clone();
    let press = Event::Click {
        handler: cx.interactivity("press").on_click.unwrap(),
        event: (&ClickEvent::default()).into(),
    };
    cx.tick(vec![press.clone(), response(requests[1].id, false), press]);
    probe.read(|view| assert_eq!(view.received, ["press", "data", "press"]));
}

#[test]
fn repeated_spawns_exhaust_the_round_budget_and_resume_next_frame() {
    fn enqueue(cx: &mut Context<Probe>, remaining: usize) {
        cx.spawn(async move |this, cx| {
            this.update(cx, |view, cx| {
                view.received.push("round".into());
                cx.notify();
                if remaining > 1 {
                    enqueue(cx, remaining - 1);
                }
            })
            .unwrap();
        })
        .detach();
    }
    let (mut cx, probe) = opened::<Probe>();
    cx.update(&probe, |_, _, cx| enqueue(cx, 40));
    assert!(cx.tick(vec![]).busy);
    probe.read(|view| assert!(view.received.len() < 40));
    let mut busy = true;
    for _ in 0..10 {
        busy = cx.tick(vec![]).busy;
        if !busy {
            break;
        }
    }
    assert!(!busy);
    probe.read(|view| assert_eq!(view.received.len(), 40));
}

mod lifecycle;
mod picture_budget;
mod primitive_tests;
mod tick_alloc;

#[test]
fn notifying_during_render_requests_another_frame() {
    #[derive(Default, Serialize, Deserialize)]
    struct Again(bool);
    impl View for Again {
        const NAME: &'static str = "Again";
        fn new(_: &mut Window, _: &mut Context<Self>) -> Self {
            Self(false)
        }
    }
    impl Render for Again {
        fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
            if !self.0 {
                self.0 = true;
                cx.notify();
            }
            div()
        }
    }
    let (cx, _) = opened::<Again>();
    let busy: Vec<_> = cx.reports().iter().map(|report| report.busy).collect();
    assert_eq!(busy, [true, false]);
}

#[test]
fn the_manifest_bytes_are_what_the_view_trait_says() {
    use wire::methods::Capability;
    #[derive(Default, Serialize, Deserialize)]
    struct App;
    impl View for App {
        const NAME: &'static str = "App";
        const DESCRIPTION: &'static str = "Words";
        const CAPABILITIES: &'static [Capability] = &[Capability::Clock, Capability::Module];
        const TARGETS: &'static [&'static str] = &["chat", "identity"];
        const MIN_WINDOW_WIDTH: u32 = 560;
        fn new(_: &mut Window, _: &mut Context<Self>) -> Self {
            App
        }
    }
    impl Render for App {
        fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
            div()
        }
    }
    let bytes: [u8; manifest_len::<App>()] = manifest_bytes::<App, { manifest_len::<App>() }>();
    assert_eq!(
        std::str::from_utf8(&bytes).unwrap(),
        format!(
            "ducktape.view.manifest\nApp\nWords\nclock,module,\n560\n{}\nchat,identity,",
            wire::WIRE_ID
        )
    );
    let manifest = wire::manifest::Manifest::parse(std::str::from_utf8(&bytes).unwrap()).unwrap();
    assert_eq!(manifest.min_width, 560);
    assert_eq!(manifest.wire_id, wire::WIRE_ID);
    assert_eq!(manifest.capabilities, App::CAPABILITIES);
    assert_eq!(manifest.targets, App::TARGETS);
    assert_eq!((&*manifest.name, &*manifest.description), ("App", "Words"));
    // a view that declares nothing is laid out from 480 and reaches no method
    assert_eq!(<UniformProbe as View>::MIN_WINDOW_WIDTH, 480);
    assert_eq!(<UniformProbe as View>::DESCRIPTION, "");
    assert!(<UniformProbe as View>::CAPABILITIES.is_empty());
    let bytes: [u8; manifest_len::<UniformProbe>()] =
        manifest_bytes::<UniformProbe, { manifest_len::<UniformProbe>() }>();
    let manifest = wire::manifest::Manifest::parse(std::str::from_utf8(&bytes).unwrap()).unwrap();
    assert_eq!(manifest.min_width, 480);
    assert_eq!(manifest.name, "UniformProbe");
}
