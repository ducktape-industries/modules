//! A view's styles cross once. The driver numbers each distinct style as it
//! lowers, a frame carries the entries the host does not hold yet, and a
//! tree sent whole starts the table over with the styles it names.
use super::*;
use gpui::{StyleRefinement, px};

fn row_style() -> StyleRefinement {
    StyleRefinement::default().flex().gap_2().p_4()
}

/// Rows of one style, and one more row of its own style once `marked`.
#[derive(Default, Serialize, Deserialize)]
struct Rows {
    rows: usize,
    marked: bool,
    rogue: bool,
}
impl View for Rows {
    const NAME: &'static str = "Rows";
    fn new(_: &mut Window, _: &mut Context<Self>) -> Self {
        Self {
            rows: 40,
            ..Self::default()
        }
    }
}
impl Render for Rows {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        let row = |id: ElementId| {
            let mut row = div().id(id);
            *row.style() = row_style();
            row
        };
        div()
            .id("rows")
            .children((0..self.rows).map(|n| row(n.into()).child(format!("row {n}"))))
            .when(self.marked, |rows| {
                rows.child(row("marked".into()).font_weight(gpui::FontWeight::BOLD))
            })
            .when(self.rogue, |rows| rows.child(Rogue))
    }
}

/// An element that names a style no table holds.
struct Rogue;
impl Element for Rogue {
    fn lower(self: Box<Self>, _: &mut Lowering<'_>) -> wire::Node {
        wire::Node::Text(wire::TextNode {
            id: None,
            style: wire::StyleId(9_999),
            content: "rogue".into(),
        })
    }
}
impl IntoElement for Rogue {
    type Element = Self;
    fn into_element(self) -> Self {
        self
    }
}

#[test]
fn forty_rows_of_one_style_send_it_once_and_a_new_row_brings_only_its_own() {
    let mut driver = Driver::<Rows>::new();
    let first = driver.tick_with(vec![], wire::Frame::clone);
    // 81 nodes name two styles: the one that sets nothing (the root's and
    // every text's) and the rows'
    assert_eq!(first.root.as_ref().unwrap().count(), 81);
    assert_eq!(
        first.styles,
        [
            wire::Style::new(&StyleRefinement::default()),
            wire::Style::new(&row_style())
        ]
    );
    let rows = first.root.as_ref().unwrap().children();
    assert!(rows.iter().all(|row| row.style() == Some(wire::StyleId(1))));

    driver.entity().update(driver.app_mut(), |view, cx| {
        view.marked = true;
        cx.notify();
    });
    let patch = driver.tick_with(vec![], wire::Frame::clone);
    assert!(matches!(
        patch.patches.as_slice(),
        [wire::Patch::Insert { node, .. }] if node.style() == Some(wire::StyleId(2))
    ));
    assert_eq!(
        patch.styles,
        [wire::Style::new(
            &row_style().font_weight(gpui::FontWeight::BOLD)
        )]
    );

    // a frame that changes no style brings none
    driver.entity().update(driver.app_mut(), |view, cx| {
        view.rows = 41;
        cx.notify();
    });
    let patch = driver.tick_with(vec![], wire::Frame::clone);
    assert!(!patch.patches.is_empty() && patch.styles.is_empty());
}

/// The test host holds a frame to the host's rule: a node that names a
/// style the table does not hold is a refused frame.
#[test]
#[should_panic(
    expected = "the host refuses this frame: a node names a style its table does not hold"
)]
fn the_test_host_refuses_a_style_its_table_does_not_hold() {
    let mut cx = TestAppContext::new();
    let view = cx.open::<Rows>();
    cx.update(&view, |view, _, cx| {
        view.rogue = true;
        cx.notify();
    });
    cx.run_until_parked();
}

/// One node whose style is new every turn.
#[derive(Default, Serialize, Deserialize)]
struct Churn(usize);
impl View for Churn {
    const NAME: &'static str = "Churn";
}
impl Render for Churn {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div().id("churn").w(px(self.0 as f32)).child("churn")
    }
}

/// A table that outgrows what the host holds starts over: the tree goes
/// whole, with the styles it names and no others, and the host takes every
/// frame on the way.
#[test]
fn a_table_past_what_the_host_holds_starts_over_with_a_whole_tree() {
    let mut driver = Driver::<Churn>::new();
    let mut held = wire::Styles::default();
    let mut wholes = Vec::new();
    for turn in 0..wire::MAX_STYLES + 4 {
        driver.entity().update(driver.app_mut(), |view, cx| {
            view.0 = turn;
            cx.notify();
        });
        let mut frame = driver.tick_with(vec![], wire::Frame::clone);
        if frame.root.is_some() {
            wholes.push((turn, frame.styles.len()));
        }
        wire::sanitize(&mut frame, &mut held).expect("the host takes every frame");
        assert!(held.len() <= wire::MAX_STYLES);
    }
    // the first frame, and the one that started the table over: each
    // brought the root's style and the text's
    assert_eq!(wholes, [(0, 2), (wire::MAX_STYLES - 1, 2)]);
    assert_eq!(held.len(), 2 + 4);
}

/// A tooltip built in the tick a tree goes whole is renumbered with it.
#[derive(Default, Serialize, Deserialize)]
struct Tipped;
impl View for Tipped {
    const NAME: &'static str = "Tipped";
}
impl Render for Tipped {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div()
            .id("tipped")
            .w(px(7.))
            .child("tipped")
            .tooltip(|_, cx| cx.new(|_| Tip).into())
    }
}
struct Tip;
impl Render for Tip {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div().w(px(9.)).child("tip")
    }
}

#[test]
fn a_tooltip_names_its_styles_in_the_table_of_the_frame_it_crosses_in() {
    let mut cx = TestAppContext::new();
    cx.open::<Tipped>();
    let width =
        |cx: &TestAppContext, node: &wire::Node| cx.styles()[node.style().unwrap()].size.width;
    let tip = |cx: &TestAppContext| {
        let [response] = cx.last_frame().tooltip_responses.as_slice() else {
            panic!("one tooltip answered")
        };
        *response.content.clone().unwrap()
    };

    // in a frame that changes nothing else: its entry joins the table
    cx.simulate_hover("tipped", true);
    assert!(cx.last_frame().root.is_none());
    assert_eq!(cx.last_frame().styles.len(), 1);
    assert_eq!(width(&cx, &tip(&cx)), Some(px(9.).into()));

    // in a frame that brings the tree whole: numbered with the tree's
    let request = cx.interactivity("tipped").tooltip.as_ref().unwrap().request;
    cx.tick(vec![
        Event::TooltipRequest {
            request,
            character_index: None,
        },
        Event::Resync,
    ]);
    assert!(cx.last_frame().root.is_some());
    assert_eq!(cx.last_frame().styles.len(), 3);
    assert_eq!(width(&cx, &tip(&cx)), Some(px(9.).into()));
    assert_eq!(width(&cx, cx.root()), Some(px(7.).into()));
}
