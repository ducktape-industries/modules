use super::*;

#[test]
fn actual_display_truncation_report_survives_encoding_and_resanitizing() {
    let mut frame = Frame {
        root: Some(text(&"x".repeat(MAX_STRING_BYTES + 1))),
        ..Default::default()
    };
    let report = sanitize(&mut frame).unwrap();
    assert!(
        report.display_text_truncated,
        "actual shortened text must be reported"
    );
    assert_eq!(frame.upstream_sanitization, report);
    let mut received: Frame = decode(&encode(&frame)).unwrap();
    assert!(
        !sanitize(&mut received).unwrap().display_text_truncated,
        "receiver observes no additional shortening"
    );
    assert!(
        received.upstream_sanitization.display_text_truncated,
        "producer loss cannot disappear across a wire hop"
    );
    let mut small = Frame {
        root: Some(text("complete")),
        ..Default::default()
    };
    assert_eq!(sanitize(&mut small).unwrap(), SanitizeReport::default());
    assert_eq!(small.upstream_sanitization, SanitizeReport::default());
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
    assert!(
        report.display_text_truncated,
        "each node fits, but the applied aggregate loses tail text"
    );
    assert_eq!(text_amounts(&root), MAX_TEXT_BYTES_PER_FRAME);
    let report = apply(
        &mut root,
        vec![Patch::Remove {
            path: vec![],
            index: 0,
        }],
    )
    .unwrap();
    assert!(
        !report.display_text_truncated,
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
fn rich_tooltip_cache_and_explicit_none_share_the_frame_budget() {
    let mut frame = Frame {
        root: Some(Node::RichText {
            id: Some(ElementIdWire::Name("rich".into())),
            style: gpui::StyleRefinement::default(),
            text: "text".into(),
            runs: RichTextRuns::default(),
            font_family_overrides: Vec::new(),
            clickable_ranges: Vec::new(),
            on_click: None,
            on_hover: None,
            tooltip: Some(TooltipResponse {
                request: 2,
                character_index: Some(0),
                content: Some(Box::new(column(
                    (0..MAX_NODES).map(|_| Node::empty()).collect(),
                ))),
            }),
        }),
        tooltip_responses: vec![TooltipResponse {
            request: 2,
            character_index: Some(1),
            content: None,
        }],
        ..Default::default()
    };
    sanitize(&mut frame).unwrap();
    let Node::RichText {
        tooltip: Some(tooltip),
        ..
    } = frame.root.unwrap()
    else {
        panic!("rich tooltip")
    };
    assert!(
        tooltip
            .content
            .as_ref()
            .is_some_and(|content| content.count() < MAX_NODES)
    );
    assert_eq!(frame.tooltip_responses.len(), 1);
    assert!(frame.tooltip_responses[0].content.is_none());
}

#[test]
fn a_frame_round_trips() {
    let frame = Frame {
        upstream_sanitization: Default::default(),
        tooltip_responses: Vec::new(),
        root: Some(column(vec![text("hello"), {
            let mut input = field("App/i", "x", "Name");
            let Node::Field {
                tokens, on_submit, ..
            } = &mut input
            else {
                unreachable!()
            };
            *tokens = vec![TextToken {
                range: TextRange::from(0..1),
                id: "<@1>".into(),
            }];
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
                tokens: Vec::new(),
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
