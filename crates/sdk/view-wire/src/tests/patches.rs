use super::*;

/// `diff` then `apply` is the identity on the new tree, and the patches
/// are the ones a reader expects: a changed field is `Props`, a moved
/// key is `Move`, a new key `Insert`, a gone key `Remove`, and a node
/// of another kind `Replace`.
#[test]
fn a_diff_applied_to_the_old_tree_is_the_new_tree() {
    let old = column(vec![
        keyed("a", "one"),
        keyed("b", "two"),
        keyed("c", "three"),
        Node::Container(crate::ContainerNode {
            id: Some(ElementIdWire::Name("box".into())),
            style: gpui::StyleRefinement::default(),
            interactivity: Default::default(),
            children: vec![keyed("inner", "deep")],
        }),
    ]);
    let mut new = column(vec![
        keyed("c", "three"),
        keyed("a", "one!"),
        keyed("d", "four"),
        Node::Container(crate::ContainerNode {
            id: Some(ElementIdWire::Name("box".into())),
            style: gpui::StyleRefinement::default(),
            interactivity: Default::default(),
            children: vec![Node::empty()],
        }),
    ]);
    let mut applied = old.clone();
    let patches = diff(&mut applied.clone(), &mut new.clone());
    let kinds: Vec<&str> = patches
        .iter()
        .map(|patch| match patch {
            Patch::Replace { .. } => "replace",
            Patch::Props { .. } => "props",
            Patch::Insert { .. } => "insert",
            Patch::Remove { .. } => "remove",
            Patch::Move { .. } => "move",
        })
        .collect();
    assert_eq!(
        kinds,
        ["remove", "move", "props", "insert", "remove", "insert"],
        "{patches:#?}"
    );
    apply(&mut applied, patches).unwrap();
    assert_eq!(applied, new);
    // Nothing changed hands: both inputs of the diff are as they were.
    let mut untouched = old.clone();
    diff(&mut untouched, &mut new);
    assert_eq!(untouched, old);
    assert!(diff(&mut new.clone(), &mut new).is_empty());
}

/// `diff_taking` emits the patches `diff` does, but the subtrees they carry
/// are moved out of the new tree, one empty stand-in left for each.
#[test]
fn a_taking_diff_moves_the_carried_subtrees_out() {
    let old = column(vec![
        keyed("a", "one"),
        column(vec![keyed("x", "x"), keyed("y", "y")]),
        column(vec![text("replaced by a text")]),
    ]);
    let new = column(vec![
        column(vec![keyed("y", "y"), keyed("x", "x"), keyed("z", "z")]),
        keyed("a", "one!"),
        keyed("b", "new"),
        text("a text now"),
    ]);
    let expected = diff(&mut old.clone(), &mut new.clone());
    let mut hollow = new.clone();
    let patches = diff_taking(&mut old.clone(), &mut hollow);
    assert_eq!(patches, expected);
    let carried: usize = patches
        .iter()
        .filter_map(|patch| match patch {
            Patch::Replace { node, .. } | Patch::Insert { node, .. } => Some(node.count()),
            _ => None,
        })
        .sum();
    let holes = patches
        .iter()
        .filter(|patch| matches!(patch, Patch::Replace { .. } | Patch::Insert { .. }))
        .count();
    assert!(carried > holes, "{patches:#?}");
    assert_eq!(
        hollow.count(),
        new.count() - carried + holes,
        "a carried subtree leaves one empty stand-in behind: {hollow:#?}"
    );
}

#[test]
fn a_patch_the_tree_cannot_take_is_refused() {
    let tree = column(vec![keyed("a", "one")]);
    let refused = |patch: Patch| apply(&mut tree.clone(), vec![patch]).unwrap_err();
    assert_eq!(
        refused(Patch::Remove {
            path: vec![7],
            index: 0
        }),
        "a path to no node"
    );
    assert_eq!(
        refused(Patch::Remove {
            path: vec![],
            index: 1
        }),
        "an index past the list"
    );
    assert_eq!(
        refused(Patch::Insert {
            path: vec![0],
            index: 0,
            node: Node::empty()
        }),
        "a list edit on no list"
    );
    assert_eq!(
        refused(Patch::Props {
            path: vec![0],
            node: Node::Deferred {
                priority: 0,
                content: Box::new(Node::empty())
            }
        }),
        "props of another arity"
    );
    let many = vec![
        Patch::Move {
            path: vec![],
            from: 0,
            to: 0
        };
        MAX_PATCHES + 1
    ];
    assert_eq!(
        apply(&mut tree.clone(), many).unwrap_err(),
        "more patches than the host applies"
    );
}

/// A patch is bounded like a tree: the result of applying it is inside
/// every limit, however the patches were shaped.
#[test]
fn an_applied_patch_frame_is_a_sanitized_tree() {
    let mut tree = column(
        (0..MAX_NODES - 1)
            .map(|i| keyed(&i.to_string(), "x"))
            .collect(),
    );
    sanitize_tree(&mut tree).unwrap();
    assert_eq!(tree.count(), MAX_NODES);
    let mut deep = keyed("0", "leaf");
    for _ in 0..MAX_DEPTH {
        deep = column(vec![deep]);
    }
    apply(
        &mut tree,
        vec![
            Patch::Insert {
                path: vec![],
                index: 0,
                node: keyed("inserted", &"y".repeat(MAX_STRING_BYTES + 1)),
            },
            Patch::Replace {
                path: vec![1],
                node: deep,
            },
        ],
    )
    .unwrap();
    assert!(tree.count() <= MAX_NODES, "{}", tree.count());
    let Node::Container(crate::ContainerNode { children, .. }) = &tree else {
        panic!()
    };
    // A new typed ID leaves every existing sibling identity unchanged.
    assert_eq!(children[0].key(), Some("inserted"));
    assert_eq!(children[2].key(), Some("1"));
    let Node::Text(crate::TextNode { content, .. }) = &children[0] else {
        panic!()
    };
    assert_eq!(content.len(), MAX_STRING_BYTES);
    let mut depth = 0;
    let mut node = &children[1];
    while let Node::Container(crate::ContainerNode { children, .. }) = node {
        depth += 1;
        node = &children[0];
    }
    assert!(depth <= MAX_DEPTH, "{depth}");
}

#[test]
fn a_well_behaved_frame_is_untouched() {
    let mut frame = Frame {
        root: Some(column(vec![text("hello")])),
        ..Frame::default()
    };
    let before = frame.clone();
    sanitize(&mut frame).unwrap();
    assert_eq!(frame, before);
}

#[test]
fn a_frame_past_the_text_budget_keeps_its_head_and_loses_its_tail() {
    const NODES: usize = 8;
    const EACH: usize = MAX_TEXT_BYTES_PER_FRAME / 4;

    let children = sanitized_children(column(
        (0..NODES).map(|_| text(&"é".repeat(EACH / 2))).collect(),
    ));
    let shaped: Vec<usize> = children
        .iter()
        .map(|child| match child {
            Node::Text(crate::TextNode { content, .. }) => content.len(),
            other => panic!("{other:?}"),
        })
        .collect();
    assert_eq!(shaped.iter().sum::<usize>(), MAX_TEXT_BYTES_PER_FRAME);
    assert_eq!(&shaped[..4], &[EACH; 4]);
    assert_eq!(&shaped[4..], &[0; NODES - 4]);
}

#[test]
fn display_truncation_shortens_a_placeholder_and_never_a_fields_text() {
    let long = "x".repeat(2 * MAX_TEXT_BYTES_PER_FRAME);
    let value = "v".repeat(MAX_FIELD_BYTES);
    let mut frame = Frame {
        root: Some(column(vec![field("App/e", &value, &long), text(&long)])),
        ..Frame::default()
    };
    assert_eq!(
        sanitize(&mut frame),
        Ok(SanitizeReport {
            display_text_truncated: true
        })
    );
    let Some(Node::Container(crate::ContainerNode { children, .. })) = &frame.root else {
        panic!()
    };
    let Node::Field {
        placeholder,
        value: kept,
        ..
    } = &children[0]
    else {
        panic!()
    };
    assert!(
        placeholder.len() < long.len(),
        "a field's placeholder is display text and spends the frame budget"
    );
    assert_eq!(
        kept, &value,
        "a 1 MiB value crosses whole: the engine adopts it, nothing shapes it"
    );
}

#[test]
fn a_field_off_its_own_text_or_claiming_an_engine_key_is_refused() {
    let over = "x".repeat(MAX_FIELD_BYTES + 1);
    let mut frame = Frame {
        root: Some(column(vec![field("App/e", &over, "")])),
        ..Frame::default()
    };
    assert_eq!(sanitize(&mut frame), Err("field text exceeds its cap"));
    let mut node = field("App/e", "é", "");
    let Node::Field { cursor, .. } = &mut node else {
        unreachable!()
    };
    *cursor = TextRange::caret(1);
    let mut frame = Frame {
        root: Some(column(vec![node.clone()])),
        ..Frame::default()
    };
    assert_eq!(sanitize(&mut frame), Err("field cursor is off its text"));
    let Node::Field { claims, cursor, .. } = &mut node else {
        unreachable!()
    };
    *cursor = TextRange::caret(2);
    *claims = Box::new([KeyClaim {
        key: keyboard::Key::Named(keyboard::Named::Backspace),
        modifiers: Default::default(),
        command: false,
    }]);
    let mut frame = Frame {
        root: Some(column(vec![node])),
        ..Frame::default()
    };
    assert_eq!(
        sanitize(&mut frame),
        Err("field claims a key the engine owns")
    );
}

#[test]
fn every_shaped_string_spends_the_same_budget() {
    let long = "x".repeat(MAX_TEXT_BYTES_PER_FRAME);
    let children = sanitized_children(column(vec![
        {
            let mut details = field("App/e", &long, &long);
            let Node::Field { options, .. } = &mut details else {
                unreachable!()
            };
            options.label = "Details".into();
            details
        },
        Node::Overlay {
            id: ElementIdWire::Name("App/o".into()),
            label: Some(long.clone()),
            style: gpui::StyleRefinement::default(),
            on_dismiss: None,
            children: vec![text("tail")],
        },
    ]));
    let Node::Field {
        placeholder,
        value,
        options,
        ..
    } = &children[0]
    else {
        panic!()
    };
    // The placeholder took the frame's whole budget and everything
    // shaped after it came out empty. The value is not shaped, so it is
    // whole; the accessible name is not shaped either, so it answers to
    // the per-string cap alone.
    assert_eq!(placeholder.len(), MAX_TEXT_BYTES_PER_FRAME);
    assert_eq!(value.len(), long.len());
    assert_eq!(options.label, "");
    let Node::Overlay {
        label, children, ..
    } = &children[1]
    else {
        panic!()
    };
    assert_eq!(label.as_deref().map(str::len), Some(MAX_STRING_BYTES));
    assert_eq!(children[0], text(""));
}

#[test]
fn a_hostile_frame_is_pulled_into_range() {
    let mut deep = text("leaf");
    for _ in 0..MAX_DEPTH + 10 {
        deep = column(vec![deep]);
    }
    let wide = column((0..MAX_NODES + 5).map(|_| text("x")).collect());
    let root = sanitized_root(column(vec![
        Node::Text(crate::TextNode {
            id: Some(ElementIdWire::Name("k".repeat(MAX_STRING_BYTES).into())),
            style: gpui::StyleRefinement::default(),
            content: "é".repeat(MAX_STRING_BYTES),
        }),
        deep,
        wide,
    ]));
    // A container whose child fell past the budget keeps an empty
    // stand-in, one per level at most.
    assert!(root.count() <= MAX_NODES + MAX_DEPTH, "{}", root.count());
    let Node::Container(crate::ContainerNode { children, .. }) = &root else {
        panic!()
    };
    let Node::Text(crate::TextNode { id, content, .. }) = &children[0] else {
        panic!("{:?}", children[0])
    };
    assert_eq!(
        id.as_ref().and_then(ElementIdWire::name).unwrap().len(),
        MAX_STRING_BYTES
    );
    assert!(content.len() <= MAX_STRING_BYTES && content.is_char_boundary(content.len()));
}

#[test]
fn oversized_typed_identity_is_refused_whole_instead_of_truncated() {
    let mut frame = Frame {
        root: Some(keyed(&"k".repeat(MAX_STRING_BYTES + 3), "text")),
        ..Default::default()
    };
    assert_eq!(
        sanitize(&mut frame).unwrap_err(),
        "element identity name is too long"
    );
}

#[test]
fn depth_is_cut_before_the_host_recurses_into_it() {
    let mut deep = text("leaf");
    for _ in 0..MAX_DEPTH * 2 {
        deep = column(vec![deep]);
    }
    let root = sanitized_root(deep);
    let mut depth = 0;
    let mut node = &root;
    while let Node::Container(crate::ContainerNode { children, .. }) = node {
        depth += 1;
        node = &children[0];
    }
    assert!(depth <= MAX_DEPTH, "{depth}");
}
