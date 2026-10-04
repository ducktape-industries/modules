use super::*;

/// The report says what was cut and where the first cut fell, and no
/// more: a frame sent on after sanitizing has nothing left to cut, and a
/// frame inside every bound reports nothing.
#[test]
fn a_cut_string_is_reported_with_where_it_fell() {
    let mut frame = Frame {
        root: Some(column(vec![
            text("complete"),
            text(&"x".repeat(MAX_STRING_BYTES + 1)),
        ])),
        ..Default::default()
    };
    let report = sanitize(&mut frame).unwrap();
    assert_eq!(
        report,
        SanitizeReport {
            strings: 1,
            first: Some(vec![1]),
            ..Default::default()
        }
    );
    let mut received: Frame = decode(&encode(&frame)).unwrap();
    assert!(sanitize(&mut received).unwrap().is_empty());
    let mut small = Frame {
        root: Some(text("complete")),
        ..Default::default()
    };
    assert!(sanitize(&mut small).unwrap().is_empty());
}

/// Nodes past the node budget and subtrees past the depth bound are cuts,
/// each at its own place; a clamped number is not.
#[test]
fn dropped_nodes_and_cut_depth_are_reported_but_clamps_are_not() {
    let mut wide = column((0..MAX_NODES + 9).map(|_| text("x")).collect());
    let report = sanitize_tree(&mut wide).unwrap();
    assert_eq!(report.nodes, 10, "{report:?}");
    assert_eq!(report.first, Some(vec![(MAX_NODES - 1) as u32]));
    let mut deep = text("leaf");
    for _ in 0..MAX_DEPTH {
        deep = column(vec![deep]);
    }
    let report = sanitize_tree(&mut deep).unwrap();
    assert_eq!((report.depth, report.nodes), (1, 0), "{report:?}");
    assert_eq!(report.first.map(|at| at.len()), Some(MAX_DEPTH));
    let mut clamped = Node::Container(crate::ContainerNode {
        style: gpui::Styled::rounded_full(gpui::StyleRefinement::default()),
        ..Default::default()
    });
    assert!(sanitize_tree(&mut clamped).unwrap().is_empty());
}

#[test]
fn applied_aggregate_text_and_rich_text_loss_is_reported_but_removal_is_not() {
    let rich = Node::RichText {
        id: Some(ElementIdWire::Name("rich".into())),
        style: gpui::StyleRefinement::default(),
        text: "y".repeat(MAX_TEXT_BYTES_PER_FRAME / 2),
        runs: RichTextRuns::default(),
        font_family_overrides: vec![],
        clickable_ranges: vec![],
        on_click: None,
        on_hover: None,
        tooltip: None,
    };
    let mut root = Node::Container(crate::ContainerNode {
        id: Some(ElementIdWire::Name("root".into())),
        style: gpui::StyleRefinement::default(),
        interactivity: Default::default(),
        children: vec![text(&"x".repeat(MAX_TEXT_BYTES_PER_FRAME / 2 + 1))],
    });
    let report = apply(
        &mut root,
        vec![Patch::Insert {
            path: vec![],
            index: 1,
            node: rich,
        }],
    )
    .unwrap();
    assert_eq!(
        (report.text, report.first),
        (1, Some(vec![1])),
        "each node fits, but the applied aggregate loses tail text"
    );
    let Node::RichText { text, .. } = &root.children()[1] else {
        panic!("the rich text")
    };
    assert_eq!(text.len(), MAX_TEXT_BYTES_PER_FRAME / 2 - 1);
    let report = apply(
        &mut root,
        vec![Patch::Remove {
            path: vec![],
            index: 0,
        }],
    )
    .unwrap();
    assert!(
        report.is_empty(),
        "intentional removal precedes the measured sanitizer pass"
    );
}

#[test]
fn encoded_size_matches_named_messagepack_without_a_second_buffer() {
    for count in [0, 1, 16, 256, 2000] {
        let frame = Frame {
            root: Some(column(
                (0..count)
                    .map(|index| keyed(&index.to_string(), "한é"))
                    .collect(),
            )),
            ..Default::default()
        };
        let bytes = encode(&frame);
        assert_eq!(bytes, rmp_serde::to_vec_named(&frame).unwrap());
        assert_eq!(encoded_size(&frame), bytes.len() as u64);
        assert_eq!(decode::<Frame>(&bytes).unwrap(), frame);
    }
}

#[test]
fn tooltip_responses_share_the_frame_node_budget() {
    let response = || TooltipResponse {
        request: 1,
        character_index: None,
        content: Some(Box::new(column(
            (0..MAX_NODES).map(|_| Node::empty()).collect(),
        ))),
    };
    let mut frame = Frame {
        tooltip_responses: vec![response(), response()],
        ..Default::default()
    };
    sanitize(&mut frame).unwrap();
    assert_eq!(frame.tooltip_responses.len(), 1);
    assert!(
        frame.tooltip_responses[0]
            .content
            .as_ref()
            .is_some_and(|content| content.count() <= MAX_NODES)
    );
}

#[test]
fn a_frame_round_trips() {
    let frame = Frame {
        tooltip_responses: Vec::new(),
        root: Some(column(vec![text("hello"), {
            let mut input = field("App/i", "x", "Name");
            let Node::Field {
                tokens, on_submit, ..
            } = &mut input
            else {
                unreachable!()
            };
            *tokens = Box::new([TextToken {
                range: TextRange::from(0..1),
                id: "<@1>".into(),
            }]);
            *on_submit = Some(4);
            input
        }])),
        patches: vec![Patch::Remove {
            path: vec![0, 1],
            index: 2,
        }],
        requests: vec![Request {
            id: 1,
            kind: "host.echo".into(),
            payload: b"hi".to_vec(),
        }],
        cancels: vec![2],
        unchanged: false,
        busy: false,
    };
    assert_eq!(decode::<Frame>(&encode(&frame)).unwrap(), frame);
    let events = vec![
        Event::Message(3),
        Event::Text {
            handler: 0,
            change: TextChange {
                generation: 1,
                revision: 3,
                edit: Some(Edit {
                    range: TextRange::from(1..2),
                    len: 1,
                }),
                text: "xy".into(),
                cursor: TextRange::caret(2),
                preedit: Some(TextRange::from(1..2)),
                tokens: Default::default(),
            },
        },
        Event::Select {
            handler: 3,
            index: 1,
        },
        Event::ScrollOffset {
            handler: 6,
            x: 24.0,
            y: 50.0,
            relative_x: 0.2,
            relative_y: 0.1,
        },
        Event::Response {
            id: 1,
            result: Err(Error::new("module", "nope")),
            done: true,
        },
        Event::Resync,
    ];
    assert_eq!(decode::<Vec<Event>>(&encode(&events)).unwrap(), events);
}
