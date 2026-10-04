//! The style table: a style crosses once however many nodes name it, a
//! node that names a style its table does not hold is a refused frame, and
//! a struct that mostly says nothing is the fields it sets, with no name.
use gpui::{StyleRefinement, Styled, px};
use view_wire::{
    Aria, ContainerNode, Frame, GroupRefinement, Interactivity, Interner, MAX_NODES, MAX_STYLES,
    Node, Patch, Refused, Style, StyleId, Styles, TextNode, Tooltip, TooltipResponse, decode,
    encode,
};

mod common;
use common::{PLAIN, plain, table};

fn text(style: StyleId) -> Node {
    Node::Text(TextNode {
        id: None,
        style,
        content: "row".into(),
    })
}

fn container(style: StyleId, interactivity: Interactivity, children: Vec<Node>) -> Node {
    Node::Container(ContainerNode {
        id: None,
        style,
        interactivity: Box::new(interactivity),
        children,
    })
}

const REFUSED: Refused = Refused::Invalid("a node names a style its table does not hold");

#[test]
fn a_style_two_nodes_share_is_one_entry_and_a_new_one_crosses_alone() {
    let row = StyleRefinement::default().flex().gap_2();
    let bold = row.clone().font_weight(gpui::FontWeight::BOLD);
    let mut styles = Interner::default();

    // a whole frame: two nodes, one style, one entry
    let (first, second) = (styles.intern(&row), styles.intern(&row.clone()));
    assert_eq!(first, second);
    let root = container(
        first,
        Interactivity::default(),
        vec![text(first), text(second)],
    );
    let whole = Frame {
        root: Some(root.clone()),
        styles: styles.unsent(),
        ..Default::default()
    };
    assert_eq!(whole.styles, [Style::new(&row)]);

    // a patch frame: the node it adds names a new style, and that entry
    // alone crosses with it
    assert_eq!(styles.intern(&row), first);
    let added = styles.intern(&bold);
    assert_ne!(added, first);
    let patch = Frame {
        patches: vec![Patch::Insert {
            path: Vec::new(),
            index: 2,
            node: text(added),
        }],
        styles: styles.unsent(),
        ..Default::default()
    };
    assert_eq!(patch.styles, [Style::new(&bold)]);
    assert!(styles.unsent().is_empty(), "an entry crosses once");

    // the host: the whole frame's table, then the patch frame's entry after it
    let mut held = Styles::default();
    let mut whole: Frame = decode(&encode(&whole)).unwrap();
    view_wire::sanitize(&mut whole, &mut held).unwrap();
    let mut tree = whole.root.unwrap();
    let mut patch: Frame = decode(&encode(&patch)).unwrap();
    view_wire::sanitize(&mut patch, &mut held).unwrap();
    view_wire::apply(&mut tree, patch.patches, &held).unwrap();
    assert_eq!(held.len(), 2);
    assert_eq!(held[first], row);
    assert_eq!(held[added], bold);
    assert_eq!(tree.children()[2], text(added));
}

/// A whole frame starts the table over with the styles its tree names, in
/// the order it names them, and carries them all.
#[test]
fn a_whole_frame_carries_the_styles_its_tree_names_and_no_others() {
    let style = |width: f32| StyleRefinement::default().w(px(width));
    let mut styles = Interner::default();
    let ids: Vec<StyleId> = (0..4)
        .map(|width| styles.intern(&style(width as f32)))
        .collect();
    assert_eq!(styles.unsent().len(), 4);

    // the next tree names the fourth style twice and the second once
    let mut tree = container(
        ids[3],
        Interactivity {
            hover: Some(ids[1]),
            ..Default::default()
        },
        vec![text(ids[3])],
    );
    styles.retain(|visit| tree.for_each_mut(&mut |node| node.styles_mut(visit)));
    assert_eq!(styles.len(), 2);
    assert_eq!(
        tree,
        container(
            StyleId(0),
            Interactivity {
                hover: Some(StyleId(1)),
                ..Default::default()
            },
            vec![text(StyleId(0))],
        )
    );
    assert_eq!(
        styles.unsent(),
        [Style::new(&style(3.)), Style::new(&style(1.))]
    );
    // a style keeps the number the whole frame gave it
    assert_eq!(styles.intern(&style(1.)), StyleId(1));
    assert_eq!(styles.intern(&style(0.)), StyleId(2));
}

#[test]
fn a_node_that_names_a_style_its_table_does_not_hold_is_refused() {
    let absent = StyleId(1);
    let conditional = |set: fn(&mut Interactivity, StyleId)| {
        let mut interactivity = Interactivity::default();
        set(&mut interactivity, absent);
        container(PLAIN, interactivity, Vec::new())
    };
    fn group(style: StyleId) -> Option<GroupRefinement> {
        Some(GroupRefinement {
            group: "row".into(),
            style,
        })
    }
    let roots = [
        text(absent),
        container(absent, Interactivity::default(), Vec::new()),
        container(PLAIN, Interactivity::default(), vec![text(absent)]),
        conditional(|interactivity, style| interactivity.hover = Some(style)),
        conditional(|interactivity, style| interactivity.active = Some(style)),
        conditional(|interactivity, style| interactivity.focus = Some(style)),
        conditional(|interactivity, style| interactivity.in_focus = Some(style)),
        conditional(|interactivity, style| interactivity.focus_visible = Some(style)),
        conditional(|interactivity, style| interactivity.group_hover = group(style)),
        conditional(|interactivity, style| interactivity.group_active = group(style)),
    ];
    for root in roots {
        // the tree a whole frame brings
        let mut frame = Frame {
            root: Some(root.clone()),
            styles: plain(),
            ..Default::default()
        };
        let refused = view_wire::sanitize(&mut frame, &mut Styles::default());
        assert_eq!(refused, Err(REFUSED), "{root:?}");

        // the subtree a patch brings
        let mut held = Styles::default();
        held.extend(plain()).unwrap();
        let mut tree = container(PLAIN, Interactivity::default(), Vec::new());
        let patch = Patch::Insert {
            path: Vec::new(),
            index: 0,
            node: root.clone(),
        };
        assert_eq!(
            view_wire::apply(&mut tree, vec![patch], &held),
            Err(REFUSED),
            "{root:?}"
        );

        // a tooltip's content
        let mut frame = Frame {
            root: Some(container(
                PLAIN,
                Interactivity {
                    tooltip: Some(Tooltip {
                        request: 1,
                        hoverable: false,
                        delay_ms: 0,
                    }),
                    ..Default::default()
                },
                Vec::new(),
            )),
            tooltip_responses: vec![TooltipResponse {
                request: 1,
                character_index: None,
                content: Some(Box::new(root)),
            }],
            styles: plain(),
            ..Default::default()
        };
        let refused = view_wire::sanitize(&mut frame, &mut Styles::default());
        assert_eq!(refused, Err(REFUSED));
    }

    // the entry a later frame brings is the one the id names from then on
    let mut held = Styles::default();
    let mut frame = Frame {
        root: Some(text(PLAIN)),
        styles: plain(),
        ..Default::default()
    };
    view_wire::sanitize(&mut frame, &mut held).unwrap();
    let mut tree = frame.root.unwrap();
    let props = || {
        vec![Patch::Props {
            path: Vec::new(),
            node: text(absent),
        }]
    };
    assert_eq!(
        view_wire::apply(&mut tree.clone(), props(), &held),
        Err(REFUSED)
    );
    held.extend(vec![Style::new(&StyleRefinement::default().w(px(4.)))])
        .unwrap();
    view_wire::apply(&mut tree, props(), &held).unwrap();
}

/// A whole frame replaces the table: an id the last tree's table held and
/// this one's does not is refused, not read from the old table.
#[test]
fn a_whole_frame_replaces_the_table_the_host_holds() {
    let mut held = Styles::default();
    let wide = StyleRefinement::default().w(px(4.));
    let mut frame = Frame {
        root: Some(text(StyleId(1))),
        styles: table(&[wide]),
        ..Default::default()
    };
    view_wire::sanitize(&mut frame, &mut held).unwrap();
    assert_eq!(held.len(), 2);

    let mut frame = Frame {
        root: Some(text(PLAIN)),
        styles: plain(),
        ..Default::default()
    };
    view_wire::sanitize(&mut frame, &mut held).unwrap();
    assert_eq!(held.len(), 1);
    let mut tree = frame.root.unwrap();
    let old = Patch::Props {
        path: Vec::new(),
        node: text(StyleId(1)),
    };
    assert_eq!(view_wire::apply(&mut tree, vec![old], &held), Err(REFUSED));
}

/// A refused frame leaves the table as it was: the tree the host still
/// holds names its styles there.
#[test]
fn a_refused_frame_leaves_the_table_the_host_holds() {
    let wide = StyleRefinement::default().w(px(4.));
    let mut held = Styles::default();
    let mut frame = Frame {
        root: Some(text(StyleId(1))),
        styles: table(std::slice::from_ref(&wide)),
        ..Default::default()
    };
    view_wire::sanitize(&mut frame, &mut held).unwrap();
    let before = held.clone();
    let pattern = StyleRefinement::default().bg(gpui::pattern_slash(gpui::black(), 1., 4.));

    // a whole frame the sanitizer refuses, by its tree and by an entry
    for (root, styles) in [
        (text(StyleId(7)), plain()),
        (text(PLAIN), table(std::slice::from_ref(&pattern))),
    ] {
        let mut frame = Frame {
            root: Some(root),
            styles,
            ..Default::default()
        };
        view_wire::sanitize(&mut frame, &mut held).unwrap_err();
        assert_eq!(held, before);
    }
    // a frame with no tree: its entries do not stay behind a refusal
    for styles in [
        vec![Style::new(&wide), Style::new(&pattern)],
        vec![Style::new(&wide); MAX_STYLES],
    ] {
        let mut frame = Frame {
            styles,
            ..Default::default()
        };
        view_wire::sanitize(&mut frame, &mut held).unwrap_err();
        assert_eq!(held, before);
    }
    assert_eq!(held[StyleId(1)], wide);
}

/// A host's own style is named from the far end of the ids, where no
/// frame's node reaches: the tree's table neither counts it nor loses it,
/// and a table a frame added to still extends the one before.
#[test]
fn a_hosts_own_style_is_no_entry_of_the_trees_table() {
    let mut held = Styles::default();
    held.extend(plain()).unwrap();
    let before = held.clone();
    let frame = std::sync::Arc::new(StyleRefinement::default().size_full());
    let mut drawn = held.clone();
    let id = drawn.host(frame.clone());
    assert_eq!(id, StyleId(u32::MAX));
    assert_eq!(drawn[id], *frame);
    assert_eq!(drawn.len(), 1);
    // a frame's node cannot name it
    let mut tree = text(PLAIN);
    let named = Patch::Props {
        path: Vec::new(),
        node: text(id),
    };
    assert_eq!(
        view_wire::apply(&mut tree, vec![named], &drawn),
        Err(REFUSED)
    );

    // entries added: every id the old table held names the same style
    held.extend(vec![Style::new(&StyleRefinement::default().w(px(4.)))])
        .unwrap();
    assert!(held.extends(&before) && !before.extends(&held));
    // another table, though its one entry is an equal style
    let mut other = Styles::default();
    other.extend(plain()).unwrap();
    assert_eq!(other, before);
    assert!(!other.extends(&before));
}

/// A table is no way around the node bounds: an entry decodes inside the
/// frame's node budget, and a table holds no more entries than a tree holds
/// nodes.
#[test]
fn a_table_is_held_to_the_bounds_a_tree_is() {
    assert_eq!(MAX_STYLES, MAX_NODES);
    let frame = |entries: usize| Frame {
        styles: vec![Style::new(&StyleRefinement::default()); entries],
        ..Default::default()
    };
    assert!(decode::<Frame>(&encode(&frame(MAX_STYLES))).is_ok());
    let refused = decode::<Frame>(&encode(&frame(MAX_STYLES + 1))).unwrap_err();
    assert!(
        refused.contains("more styles than the host holds"),
        "{refused}"
    );

    // the entries of patch frames add up: the table the host holds is bounded
    let mut held = Styles::default();
    held.extend(frame(MAX_STYLES).styles).unwrap();
    assert_eq!(
        held.extend(frame(1).styles),
        Err("more styles than the host holds")
    );
    assert_eq!(held.len(), MAX_STYLES);
}

/// An entry is bytes the guest wrote: one that does not read as a style
/// refuses its frame.
#[test]
fn an_entry_that_is_no_style_is_refused() {
    let entry = |bytes: &[u8]| -> Style {
        // an entry crosses as a MessagePack `bin`
        let mut encoded = vec![0xc4, bytes.len() as u8];
        encoded.extend_from_slice(bytes);
        decode(&encoded).unwrap()
    };
    let plain = encode(&Style::new(&StyleRefinement::default()));
    assert_eq!(plain, [0xc4, 5, 0, 0, 0, 0, 0]);
    for (bytes, refusal) in [
        (&[][..], "a style entry ends before its fields do"),
        (
            &[1, 0, 0, 0, 0][..],
            "a style entry ends before its fields do",
        ),
        (&[1, 0, 0, 0, 0, 9][..], "a style entry names no Display"),
        (
            &[0, 0, 0, 0, 0x10][..],
            "a style entry sets a field this build does not have",
        ),
        (
            &[0, 0, 0, 0, 0, 0][..],
            "a style entry runs past its fields",
        ),
    ] {
        assert_eq!(entry(bytes).read(), Err(refusal), "{bytes:?}");
        let mut frame = Frame {
            styles: vec![entry(bytes)],
            ..Default::default()
        };
        assert_eq!(
            view_wire::sanitize(&mut frame, &mut Styles::default()),
            Err(Refused::Invalid(refusal))
        );
    }
}

/// A style that sets one field is its bitmap and that field: five bytes
/// of bitmap for `StyleRefinement`'s 36 fields, bit 32 (`opacity`) set,
/// then the `f32`. No name is in it.
#[test]
fn a_style_with_one_field_is_its_bitmap_and_that_field() {
    let style = StyleRefinement::default().opacity(0.5);
    let fields = view_wire::entry_fields()[0].1;
    assert_eq!((fields.len(), fields[32]), (36, "opacity"));
    let mut expected = vec![0xc4, 9, 0, 0, 0, 0, 1];
    expected.extend_from_slice(&0.5f32.to_le_bytes());
    assert_eq!(encode(&Style::new(&style)), expected);
    assert_eq!(Style::new(&style).read(), Ok(style));

    // a nested refinement is a field with a bitmap of its own: `size`
    // (bit 8), and in it `width` (bit 0) as a length in pixels
    let style = StyleRefinement::default().w(px(2.));
    let mut expected = vec![0xc4, 11, 0, 1, 0, 0, 0, 1, 0];
    expected.extend_from_slice(&2f32.to_le_bytes());
    assert_eq!(encode(&Style::new(&style)), expected);
    assert_eq!(Style::new(&style).read(), Ok(style));
}

/// `Interactivity` and `Aria` are the fields they set, each under its
/// declaration index: no name, and nothing for a field that says nothing.
#[test]
fn a_sparse_struct_with_one_field_is_that_field_under_its_index() {
    // `on_click` is `Interactivity`'s field 18 (`schema.txt`)
    let interactivity = Interactivity {
        on_click: Some(7),
        ..Default::default()
    };
    let bytes = encode(&interactivity);
    assert_eq!(bytes, [0x81, 18, 7]);
    assert_eq!(decode::<Interactivity>(&bytes).unwrap(), interactivity);

    // `label` is `Aria`'s field 1
    let aria = Aria {
        label: Some("Send".into()),
        ..Default::default()
    };
    let bytes = encode(&aria);
    assert_eq!(bytes, [0x81, 1, 0xa4, b'S', b'e', b'n', b'd']);
    assert_eq!(decode::<Aria>(&bytes).unwrap(), aria);

    // one that says nothing is an empty map
    assert_eq!(encode(&Interactivity::default()), [0x80]);
    assert_eq!(encode(&Aria::default()), [0x80]);
}

/// A sparse struct's reader takes the fields this build has and refuses any
/// other index, as a style entry's bitmap refuses a bit past its last field.
/// An index it skipped would be a value of any shape read and dropped.
#[test]
fn a_sparse_struct_refuses_a_field_this_build_does_not_have() {
    // `Interactivity` has 43 fields and `Aria` 32 (`schema.txt`)
    let error = decode::<Interactivity>(&[0x81, 99, 0xc3]).unwrap_err();
    assert!(
        error.contains("expected field index 0 <= i < 43"),
        "{error}"
    );
    let error = decode::<Aria>(&[0x81, 127, 0xc0]).unwrap_err();
    assert!(
        error.contains("expected field index 0 <= i < 32"),
        "{error}"
    );
    // the last field each has is still read
    assert!(decode::<Interactivity>(&[0x81, 42, 0xc0]).is_ok());
    assert!(decode::<Aria>(&[0x81, 31, 0x90]).is_ok());
}

/// A node on the wire: the variant's index (`Text` is `Node`'s variant 8),
/// then its fields in order, the style as its id. No field or variant name
/// is in a frame's bytes.
#[test]
fn a_node_is_its_fields_in_order_with_no_name() {
    assert_eq!(
        encode(&text(StyleId(3))),
        [0x81, 0x08, 0x93, 0xc0, 0x03, 0xa3, b'r', b'o', b'w']
    );
    let frame = Frame {
        root: Some(container(
            PLAIN,
            Interactivity {
                hover: Some(PLAIN),
                on_click: Some(1),
                ..Default::default()
            },
            vec![text(PLAIN)],
        )),
        styles: plain(),
        ..Default::default()
    };
    let bytes = encode(&frame);
    for name in [
        "root",
        "styles",
        "Container",
        "Text",
        "style",
        "children",
        "content",
        "hover",
        "on_click",
    ] {
        assert!(
            !bytes
                .windows(name.len())
                .any(|window| window == name.as_bytes()),
            "`{name}` is in the frame's bytes"
        );
    }
    assert_eq!(decode::<Frame>(&bytes).unwrap(), frame);
}
