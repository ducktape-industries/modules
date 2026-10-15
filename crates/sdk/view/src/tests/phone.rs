//! A view on a phone: its page stack, Back, a sheet, and the sweep that
//! holds it to the touch ladder (SPEC §8).
use serde::{Deserialize, Serialize};

use crate::design::{self, CAPSULE_CLEARANCE, Pages};
use crate::prelude::*;
use crate::testing::{self, TestAppContext};
use crate::{BackEvent, View, wire};

#[derive(Serialize, Deserialize)]
struct Phone {
    pages: Pages<String>,
    picking: bool,
}
impl Default for Phone {
    fn default() -> Self {
        Self {
            pages: Pages::root("rooms".into()),
            picking: false,
        }
    }
}
impl View for Phone {
    const NAME: &'static str = "Phone";
    const CAPABILITIES: &'static [wire::methods::Capability] = &[wire::methods::Capability::Host];
}
impl Render for Phone {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = *cx.global::<Theme>();
        let ladder = design::ladder(window.touch());
        let depth = self.pages.depth();
        let pop = |view: &mut Self, cx: &mut Context<Self>| {
            view.pages.pop();
            cx.notify();
        };
        let back =
            (depth > 0).then(|| cx.listener(move |view, _: &ClickEvent, _, cx| pop(view, cx)));
        let page = design::page("rooms", self.pages.top().clone(), back, &ladder, &theme)
            .caption(format!("{} deep", depth))
            .leading(div().id("avatar").size(ladder.avatar).rounded_full())
            .trailing(design::button(
                "new",
                "New",
                &ladder,
                &theme,
                cx.listener(|view, _, _, cx| {
                    view.pages.push("new".into());
                    cx.notify();
                }),
            ))
            .child(design::button(
                "open",
                "Open",
                &ladder,
                &theme,
                cx.listener(|view, _, _, cx| {
                    view.pages.push("room".into());
                    cx.notify();
                }),
            ))
            .child(design::button(
                "pick",
                "Pick",
                &ladder,
                &theme,
                cx.listener(|view, _, _, cx| {
                    view.picking = true;
                    cx.notify();
                }),
            ))
            .children(self.picking.then(|| {
                design::sheet("picker", "Pick one", &theme)
                    .on_dismiss(cx.listener(|view, _, _, cx| {
                        view.picking = false;
                        cx.notify();
                    }))
                    .child("options")
                    .into_any_element()
            }));
        div()
            .id("phone")
            .size_full()
            .when(depth > 0, |root| {
                root.on_back(cx.listener(move |view, _: &BackEvent, _, cx| pop(view, cx)))
            })
            .child(page)
    }
}

#[test]
fn a_phone_view_survives_the_sweep() {
    let mut cx = TestAppContext::new();
    cx.open::<Phone>();
    testing::sweep(&mut cx, |cx| {
        assert_eq!(cx.back_handler(), None);
        cx.simulate_click("Open");
        assert!(cx.has_text("room"));
        assert!(cx.back_handler().is_some());
        cx.simulate_click("Pick");
        assert!(
            matches!(cx.node("picker"), wire::Node::Sheet { detents, label, .. }
                if detents == &[0.6, 1.0] && label.as_deref() == Some("Pick one"))
        );
        cx.simulate_dismiss("picker");
        assert!(cx.find("picker").is_none());
        cx.simulate_back();
        assert!(cx.has_text("rooms"));
        assert_eq!(cx.back_handler(), None);
    });
}

/// A Back the host sends to a route the view no longer holds (it landed a
/// frame late) does nothing; the live route pops.
#[test]
fn a_back_to_no_route_is_ignored() {
    let mut cx = TestAppContext::new();
    cx.open::<Phone>();
    cx.simulate_click("Open");
    let handler = cx.back_handler().unwrap();
    cx.tick(vec![wire::Event::Back {
        handler: handler + 1_000,
    }]);
    assert!(cx.has_text("room"));
    cx.tick(vec![wire::Event::Back { handler }]);
    assert!(cx.has_text("rooms"));
}

/// The title row follows the ladder, draws Back at every ladder, and clears
/// the host's capsule under touch only. The device is read in the first
/// frame: a replacement opened under touch draws no pointer frame first.
#[test]
fn page_draws_back_by_the_ladder_and_clears_the_capsule_under_touch() {
    let mut cx = TestAppContext::new();
    cx.open::<Phone>();
    assert!(cx.find("rooms-back").is_none(), "no Back at the root");
    cx.simulate_click("Open");
    assert_eq!(
        cx.interactivity("rooms-back").aria.label.as_deref(),
        Some("Back")
    );
    // Back, leading, the words (heading over caption, truncating, taking
    // the slack), then the page's own control
    let row = cx.node("rooms-title").children();
    let keys: Vec<_> = row.iter().map(|node| node.key()).collect();
    assert_eq!(keys[..2], [Some("rooms-back"), Some("avatar")]);
    assert_eq!(keys.len(), 4);
    assert!(keys[3].is_some_and(|key| key.ends_with("new")), "{keys:?}");
    let words: Vec<_> = row[2].children().iter().map(|node| node.key()).collect();
    assert_eq!(words, [Some("rooms-heading"), Some("rooms-caption")]);
    assert!(cx.has_text("1 deep"));
    for key in ["rooms-heading", "rooms-caption"] {
        assert_eq!(
            cx.style(key).text.text_overflow,
            Some(gpui::TextOverflow::Truncate("…".into())),
            "{key}"
        );
    }
    let height = |cx: &TestAppContext| cx.style("rooms-title").min_size.height;
    assert_eq!(height(&cx), Some(design::POINTER.row.into()));
    assert_ne!(
        cx.style("rooms-title").padding.right,
        Some(CAPSULE_CLEARANCE.into())
    );
    let renders = cx.renders();
    cx.simulate_touch(true);
    assert_eq!(cx.renders(), renders + 1);
    assert_eq!(height(&cx), Some(design::TOUCH.row.into()));
    assert!(cx.find("rooms-back").is_some());
    assert_eq!(
        cx.style("rooms-title").padding.right,
        Some(CAPSULE_CLEARANCE.into())
    );
    cx.simulate_touch(true);
    assert_eq!(cx.renders(), renders + 1, "the same device is no render");
    cx.open::<Phone>();
    cx.simulate_click("Open");
    assert_eq!(height(&cx), Some(design::TOUCH.row.into()));
}

#[derive(Default, Serialize, Deserialize)]
struct Cramped;
impl View for Cramped {
    const NAME: &'static str = "Cramped";
}
impl Render for Cramped {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div()
            .id("short")
            .h(px(20.))
            .role(crate::Role::Button)
            .focusable()
            .on_click(|_, _, _| {})
            .child("Tap")
    }
}

#[test]
#[should_panic(expected = "declares no height of at least 44")]
fn a_short_press_target_does_not_fit_a_phone() {
    let mut cx = TestAppContext::new();
    cx.open::<Cramped>();
    testing::assert_fits(cx.root(), cx.styles(), 390., &design::TOUCH);
}

#[derive(Default, Serialize, Deserialize)]
struct NoWayBack;
impl View for NoWayBack {
    const NAME: &'static str = "NoWayBack";
}
impl Render for NoWayBack {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div()
            .id("root")
            .child(crate::sheet("picker", "Pick one").child("options"))
    }
}

#[test]
#[should_panic(expected = "a sheet is open and the root has no on_back")]
fn a_sheet_without_a_root_back_does_not_fit_a_phone() {
    let mut cx = TestAppContext::new();
    cx.open::<NoWayBack>();
    testing::assert_fits(cx.root(), cx.styles(), 390., &design::TOUCH);
}

#[derive(Default, Serialize, Deserialize)]
struct BackToNothing;
impl View for BackToNothing {
    const NAME: &'static str = "BackToNothing";
}
impl Render for BackToNothing {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div()
            .id("root")
            .on_back(|_: &BackEvent, _, _| {})
            .child("home")
    }
}

/// The other direction: a root `on_back` with no sheet open and no Back
/// drawn takes the phone's Back for a pop that has nowhere to go.
#[test]
#[should_panic(expected = "the root has on_back with no sheet open and no Back control drawn")]
fn a_root_back_with_nowhere_to_go_does_not_fit_a_phone() {
    let mut cx = TestAppContext::new();
    cx.open::<BackToNothing>();
    testing::assert_fits(cx.root(), cx.styles(), 390., &design::TOUCH);
}
