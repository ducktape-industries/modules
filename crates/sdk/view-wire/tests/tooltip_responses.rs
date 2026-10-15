//! A tooltip response's content is sanitized like the tree it comes with.
use gpui::{StyleRefinement, Styled, px};
use view_wire::*;

mod common;
use common::{PLAIN, table};

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
    assert_eq!(interactivity.tooltip.as_ref().unwrap().delay_ms, u64::MAX);
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
            Some(px(-1e9).into())
        );
        assert_eq!(
            style(interaction.focus_visible).text.font_size,
            Some(px(1e20).into())
        );
    }
}
