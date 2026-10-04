use super::*;

/// Children past the node budget are dropped, not stood in for.
#[test]
fn a_container_is_cut_at_the_node_budget() {
    let root = sanitized_root(Node::Container(crate::ContainerNode {
        id: Some(ElementIdWire::Name("App/cells".into())),
        style: StyleId(0),
        interactivity: Default::default(),
        children: (0..MAX_NODES + 5).map(|_| text("x")).collect(),
    }));
    let Node::Container(crate::ContainerNode { children, .. }) = &root else {
        panic!()
    };
    assert_eq!(children.len(), MAX_NODES - 1);
    assert_eq!(root.count(), MAX_NODES);
}

#[test]
fn svg_and_raster_images_share_the_frame_picture_budget() {
    let image = Node::Image {
        id: Some(ElementIdWire::Name("App/raster".into())),
        hash: 8,
        data: Some(ImageData::Encoded(vec![
            0;
            MAX_PICTURE_BYTES_PER_FRAME / 2 + 1
        ])),
        label: None,
        image_style: ImageStyle {
            grayscale: false,
            object_fit: ImageObjectFit::Contain,
        },
        loading: false,
        fallback: false,
        state_children: vec![],
        style: StyleId(0),
        interactivity: Default::default(),
    };
    let mut frame = Frame {
        root: Some(column(vec![
            picture(Some(vec![0; MAX_PICTURE_BYTES_PER_FRAME / 2])),
            image,
        ])),
        ..Frame::default()
    };
    sanitize_plain(&mut frame).unwrap();
    let children = frame.root.as_ref().unwrap().children();
    assert!(matches!(
        children[0],
        Node::Svg {
            source: SvgSource::Data { bytes: Some(_), .. },
            ..
        }
    ));
    assert!(
        matches!(children[1], Node::Image { data: None, .. }),
        "SVG consumption must reduce raster admission"
    );
    assert!(decode::<Frame>(&encode(&frame)).is_ok());
}

#[test]
fn primitive_geometry_and_state_children_are_bounded_in_the_main_walk() {
    let image = Node::Image {
        id: Some(ElementIdWire::Integer(1)),
        hash: 1,
        data: Some(ImageData::Refusal("x".repeat(MAX_STRING_BYTES + 1))),
        label: None,
        image_style: ImageStyle {
            grayscale: false,
            object_fit: ImageObjectFit::Contain,
        },
        loading: true,
        fallback: true,
        state_children: vec![text("loading"), text("fallback"), text("extra")],
        style: StyleId(0),
        interactivity: Default::default(),
    };
    let svg = Node::Svg {
        id: Some(ElementIdWire::Integer(2)),
        source: SvgSource::External("x".repeat(MAX_STRING_BYTES + 1)),
        transformation: SvgTransformation {
            scale: [f32::NAN, f32::INFINITY],
            translate: [f32::NEG_INFINITY, f32::INFINITY],
            rotate: f32::NAN,
        },
        label: None,
        style: StyleId(0),
        interactivity: Default::default(),
    };
    let anchored = Node::Anchored {
        anchor: Anchor::TopLeft,
        fit: AnchoredFitMode::SnapToWindowWithMargin([f32::NAN, f32::INFINITY, -1., 4.]),
        position: Some([f32::INFINITY, f32::NEG_INFINITY]),
        position_mode: AnchoredPositionMode::Local,
        offset: Some([f32::NAN, 3.]),
        children: vec![Node::Deferred {
            priority: usize::MAX,
            content: Box::new(text("child")),
        }],
    };
    let children = sanitized_children(column(vec![image, svg, anchored]));
    let Node::Image {
        data: Some(ImageData::Refusal(reason)),
        state_children,
        ..
    } = &children[0]
    else {
        panic!()
    };
    assert!(reason.len() <= MAX_STRING_BYTES);
    assert_eq!(state_children.len(), 2);
    let Node::Svg {
        source: SvgSource::External(path),
        transformation,
        ..
    } = &children[1]
    else {
        panic!()
    };
    assert!(path.len() <= MAX_STRING_BYTES);
    assert_eq!(transformation.scale, [0., MAX_PIXELS]);
    assert_eq!(transformation.translate, [-MAX_PIXELS, MAX_PIXELS]);
    assert_eq!(transformation.rotate, 0.);
    let Node::Anchored {
        fit,
        position,
        offset,
        children,
        ..
    } = &children[2]
    else {
        panic!()
    };
    assert_eq!(*position, Some([MAX_PIXELS, -MAX_PIXELS]));
    assert_eq!(*offset, Some([0., 3.]));
    assert_eq!(
        *fit,
        AnchoredFitMode::SnapToWindowWithMargin([0., MAX_PIXELS, 0., 4.])
    );
    assert!(matches!(children[0], Node::Deferred { priority: 16, .. }));
}

/// A picture past what is left of the frame's budget is dropped whole,
/// never cut: half an SVG is not an SVG. The head of the frame keeps
/// its pictures; a hash without bytes passes as the reference it is.
#[test]
fn a_frame_past_the_picture_budget_drops_whole_pictures_from_its_tail() {
    const EACH: usize = MAX_PICTURE_BYTES_PER_FRAME / 4 * 3;
    let children = sanitized_children(column(vec![
        picture(Some(vec![b'<'; EACH])),
        picture(Some(vec![b'<'; EACH])),
        picture(None),
        picture(Some(vec![b'<'; MAX_PICTURE_BYTES_PER_FRAME / 4])),
    ]));
    let carried: Vec<Option<usize>> = children
        .iter()
        .map(|child| match child {
            Node::Svg {
                source: SvgSource::Data { bytes, .. },
                ..
            } => bytes.as_ref().map(Vec::len),
            other => panic!("{other:?}"),
        })
        .collect();
    assert_eq!(
        carried,
        [
            Some(EACH),
            None,
            None,
            Some(MAX_PICTURE_BYTES_PER_FRAME / 4)
        ]
    );
}

#[test]
fn a_text_with_no_heading_or_live_region_round_trips() {
    let text = Node::Text(crate::TextNode {
        id: Some(ElementIdWire::Name("text".into())),
        style: StyleId(0),
        content: "huge".into(),
    });
    assert_eq!(decode::<Node>(&encode(&text)).unwrap(), text);
}

/// An aria field a view did not set costs no bytes: the default is the
/// empty map, and one field set alone is a map of one.
#[test]
fn an_unset_aria_field_sends_no_bytes() {
    assert_eq!(encode(&Aria::default()), [0x80]);
    let one_each = [
        Aria {
            live: Some(Live::Polite),
            ..Default::default()
        },
        Aria {
            busy: true,
            ..Default::default()
        },
        Aria {
            required: true,
            ..Default::default()
        },
        Aria {
            read_only: true,
            ..Default::default()
        },
        Aria {
            invalid: Some(Invalid::True),
            ..Default::default()
        },
        Aria {
            has_popup: Some(HasPopup::Menu),
            ..Default::default()
        },
        Aria {
            current: Some(AriaCurrent::False),
            ..Default::default()
        },
        Aria {
            actions: vec![(Action::Increment, 1)],
            ..Default::default()
        },
        Aria {
            custom_actions: vec![(1, "Pin".into())],
            ..Default::default()
        },
    ];
    for aria in one_each {
        let bytes = encode(&aria);
        assert_eq!(bytes[0], 0x81, "one key for {aria:?}");
        assert_eq!(decode::<Aria>(&bytes).unwrap(), aria);
    }
}
