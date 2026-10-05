//! Every node kind shares the frame's one node and text budget: a tooltip
//! response's content, anchored children, image state children and picture
//! labels cannot buy more than the tree they come with, and a tooltip's
//! content is bounded like the tree.
use gpui::{StyleRefinement, Styled, px};
use view_wire::*;

mod common;
use common::{PLAIN, sanitize, table};

fn container(interactivity: Interactivity, children: Vec<Node>) -> Node {
    Node::Container(view_wire::ContainerNode {
        id: None,
        style: PLAIN,
        interactivity: Some(Box::new(interactivity)),
        children,
    })
}

#[test]
fn focus_refinements_are_bounded_inside_tooltip_responses() {
    let hostile = Interactivity {
        focus: Some(StyleId(1)),
        in_focus: Some(StyleId(2)),
        focus_visible: Some(StyleId(3)),
        ..Default::default()
    };
    let root = container(
        Interactivity {
            tooltip: Some(Tooltip {
                request: 1,
                hoverable: true,
                delay_ms: u64::MAX,
            }),
            ..hostile.clone()
        },
        vec![],
    );
    let mut frame = Frame {
        root: Some(root),
        tooltip_responses: vec![TooltipResponse {
            request: 1,
            character_index: None,
            content: Some(Box::new(container(hostile, vec![]))),
        }],
        styles: table(&[
            StyleRefinement::default().w(px(f32::INFINITY)).opacity(10.),
            StyleRefinement::default().m(px(-1e9)),
            StyleRefinement::default().text_size(px(1e20)),
        ]),
        ..Default::default()
    };
    let mut styles = Styles::default();
    view_wire::sanitize(&mut frame, &mut styles).unwrap();
    let Node::Container(view_wire::ContainerNode {
        interactivity: Some(interactivity),
        ..
    }) = frame.root.unwrap()
    else {
        unreachable!()
    };
    assert_eq!(interactivity.tooltip.as_ref().unwrap().delay_ms, 60_000);
    let Node::Container(view_wire::ContainerNode {
        interactivity: Some(nested),
        ..
    }) = frame.tooltip_responses[0].content.as_deref().unwrap()
    else {
        unreachable!()
    };
    for interaction in [&interactivity, nested] {
        let style = |id: Option<StyleId>| &styles[id.unwrap()];
        assert_eq!(style(interaction.focus).size.width, Some(px(0.).into()));
        assert_eq!(style(interaction.focus).opacity, Some(1.));
        assert_eq!(
            style(interaction.in_focus).margin.left,
            Some(px(-8192.).into())
        );
        assert_eq!(
            style(interaction.focus_visible).text.font_size,
            Some(px(512.).into())
        );
    }
}

#[test]
fn a_tooltip_responses_content_and_the_tree_share_one_node_budget() {
    let root = container(
        Interactivity {
            tooltip: Some(Tooltip {
                request: 1,
                hoverable: false,
                delay_ms: 0,
            }),
            ..Default::default()
        },
        vec![Node::empty()],
    );
    let mut frame = Frame {
        root: Some(root),
        tooltip_responses: vec![TooltipResponse {
            request: 1,
            character_index: None,
            content: Some(Box::new(container(
                Interactivity::default(),
                (0..view_wire::MAX_NODES).map(|_| Node::empty()).collect(),
            ))),
        }],
        ..Default::default()
    };
    sanitize(&mut frame).unwrap();
    let tooltip_nodes = frame.tooltip_responses[0].content.as_ref().unwrap().count();
    assert!(frame.root.unwrap().count() + tooltip_nodes <= view_wire::MAX_NODES);
}

fn text() -> Node {
    Node::Text(view_wire::TextNode {
        id: None,
        style: PLAIN,
        content: "row".into(),
    })
}
fn anchored(children: Vec<Node>) -> Node {
    Node::Anchored {
        anchor: Anchor::TopLeft,
        fit: AnchoredFitMode::SnapToWindow,
        position: None,
        position_mode: AnchoredPositionMode::Local,
        offset: None,
        children,
    }
}
fn image(label: String, state_children: Vec<Node>) -> Node {
    Node::Image {
        id: None,
        hash: 0,
        data: None,
        label: Some(label),
        image_style: ImageStyle {
            grayscale: false,
            object_fit: ImageObjectFit::Contain,
        },
        loading: !state_children.is_empty(),
        fallback: state_children.len() > 1,
        state_children,
        style: PLAIN,
        interactivity: Default::default(),
    }
}

#[test]
fn anchored_children_obey_the_global_node_budget() {
    let mut frame = Frame {
        root: Some(anchored((0..MAX_NODES + 10).map(|_| text()).collect())),
        ..Default::default()
    };
    sanitize(&mut frame).unwrap();
    assert_eq!(frame.root.unwrap().count(), MAX_NODES);
}

#[test]
fn image_state_children_cannot_survive_past_the_global_node_budget() {
    let children = vec![anchored((0..MAX_NODES).map(|_| text()).collect()), text()];
    let mut frame = Frame {
        root: Some(image(String::new(), children)),
        ..Default::default()
    };
    sanitize(&mut frame).unwrap();
    let root = frame.root.unwrap();
    assert!(root.count() <= MAX_NODES);
    let Node::Image {
        loading,
        fallback,
        state_children,
        ..
    } = root
    else {
        unreachable!()
    };
    assert!(
        !loading && !fallback,
        "incomplete state recipes must not retain invalid indexes"
    );
    assert!(state_children.is_empty());
}

#[test]
fn picture_labels_share_the_frame_text_budget() {
    let mut frame = Frame {
        root: Some(anchored(vec![
            image("a".repeat(MAX_STRING_BYTES), Vec::new()),
            Node::Svg {
                id: None,
                source: SvgSource::None,
                transformation: SvgTransformation {
                    scale: [1., 1.],
                    translate: [0., 0.],
                    rotate: 0.,
                },
                label: Some("b".repeat(MAX_STRING_BYTES)),
                style: PLAIN,
                interactivity: Default::default(),
            },
        ])),
        ..Default::default()
    };
    sanitize(&mut frame).unwrap();
    let Node::Anchored { children, .. } = frame.root.unwrap() else {
        unreachable!()
    };
    let Node::Image { label, .. } = &children[0] else {
        unreachable!()
    };
    assert_eq!(label.as_ref().unwrap().len(), MAX_TEXT_BYTES_PER_FRAME);
    let Node::Svg { label, .. } = &children[1] else {
        unreachable!()
    };
    assert_eq!(label.as_deref(), Some(""));
}
