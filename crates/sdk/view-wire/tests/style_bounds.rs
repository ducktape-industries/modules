//! `StyleRefinement` on the wire: every style a tree names is an entry of
//! its table, bounded once as the host takes it; a style change is a props
//! patch that names a new entry, which receives the same bounds.
use gpui::{StyleRefinement, Styled, px};
use view_wire::{Frame, GroupRefinement, Node, Patch, Style, StyleId, Styles, decode, encode};

mod common;
use common::{PLAIN, table};

fn container(style: StyleId) -> Node {
    Node::Container(view_wire::ContainerNode {
        id: None,
        style,
        interactivity: Default::default(),
        children: vec![Node::Text(view_wire::TextNode {
            id: None,
            style: PLAIN,
            content: "stable child".into(),
        })],
    })
}

#[test]
fn whole_frames_bound_base_and_every_conditional_style() {
    // five entries, each hostile and each its own
    let hostile: Vec<_> = (0..5)
        .map(|height| {
            StyleRefinement::default()
                .w(px(f32::INFINITY))
                .h(px(height as f32))
                .m(px(-1e9))
                .opacity(7.)
        })
        .collect();
    let mut root = container(StyleId(1));
    let Node::Container(view_wire::ContainerNode { interactivity, .. }) = &mut root else {
        unreachable!()
    };
    let interactivity = interactivity.insert(Default::default());
    interactivity.hover = Some(StyleId(2));
    interactivity.active = Some(StyleId(3));
    interactivity.group_hover = Some(GroupRefinement {
        group: "row".into(),
        style: StyleId(4),
    });
    interactivity.group_active = Some(GroupRefinement {
        group: "row".into(),
        style: StyleId(5),
    });
    let mut frame = Frame {
        root: Some(root),
        styles: table(&hostile),
        ..Default::default()
    };
    let mut styles = Styles::default();
    view_wire::sanitize(&mut frame, &mut styles).unwrap();
    let Node::Container(view_wire::ContainerNode {
        style,
        interactivity: Some(interactivity),
        ..
    }) = frame.root.unwrap()
    else {
        unreachable!()
    };
    for (height, style) in [
        style,
        interactivity.hover.unwrap(),
        interactivity.active.unwrap(),
        interactivity.group_hover.unwrap().style,
        interactivity.group_active.unwrap().style,
    ]
    .into_iter()
    .enumerate()
    {
        let style = &styles[style];
        assert_eq!(style.size.width, Some(px(0.).into()));
        assert_eq!(style.size.height, Some(px(height as f32).into()));
        assert_eq!(style.margin.left, Some(px(-8192.).into()));
        assert_eq!(style.opacity, Some(1.));
    }
}

/// gpui's own styles cross unchanged; a style change is one props patch
/// whose frame carries the new entry, bounded as a whole frame's entries
/// are, and the children under it stay.
#[test]
fn actual_gpui_styled_payloads_roundtrip_and_patch_without_replacing_children() {
    let text = StyleRefinement::default()
        .text_size(px(13.))
        .text_color(gpui::rgb(0xabcdef));
    let row = StyleRefinement::default()
        .flex()
        .gap_2()
        .p_4()
        .rounded_md()
        .bg(gpui::rgb(0x112233));
    // the patch frame's entry is bounded as the whole frame's were
    let wider = row.clone().p_6().w(px(1e20));
    let node = |style| {
        Node::Container(view_wire::ContainerNode {
            id: None,
            style,
            interactivity: Default::default(),
            children: vec![Node::Text(view_wire::TextNode {
                id: None,
                style: StyleId(1),
                content: "retained child".into(),
            })],
        })
    };
    let mut frame: Frame = decode(&encode(&Frame {
        root: Some(node(StyleId(2))),
        styles: table(&[text.clone(), row.clone()]),
        ..Default::default()
    }))
    .unwrap();
    let mut styles = Styles::default();
    view_wire::sanitize(&mut frame, &mut styles).unwrap();
    let mut old = frame.root.unwrap();
    assert_eq!(styles[StyleId(1)], text);
    assert_eq!(styles[StyleId(2)], row);

    let mut changed = node(StyleId(3));
    let patches = view_wire::diff(&mut old, &mut changed);
    assert!(matches!(patches.as_slice(), [Patch::Props { path, .. }] if path.is_empty()));
    let mut frame: Frame = decode(&encode(&Frame {
        patches,
        styles: vec![Style::new(&wider)],
        ..Default::default()
    }))
    .unwrap();
    view_wire::sanitize(&mut frame, &mut styles).unwrap();
    view_wire::apply(&mut old, frame.patches, &styles).unwrap();
    assert_eq!(old, changed);
    assert_eq!(styles[StyleId(3)], row.clone().p_6().w(px(8192.)));
    let Node::Container(container) = old else {
        unreachable!()
    };
    let [Node::Text(text)] = container.children.as_slice() else {
        unreachable!()
    };
    assert_eq!(text.content, "retained child");
    assert_eq!(styles[text.style].text.font_size, Some(px(13.).into()));
}
