use super::*;

#[test]
fn primitive_geometry_and_state_children_are_bounded_in_the_main_walk() {
    let image = Node::Image {
        id: Some(ElementIdWire::Integer(1)),
        hash: 1,
        data: Some(ImageData::Refusal("x".into())),
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
        source: SvgSource::External("x".into()),
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
    let Node::Image { state_children, .. } = &children[0] else {
        panic!()
    };
    assert_eq!(state_children.len(), 2);
    let Node::Svg { transformation, .. } = &children[1] else {
        panic!()
    };
    assert_eq!(transformation.scale, [0., f32::MAX]);
    assert_eq!(transformation.translate, [f32::MIN, f32::MAX]);
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
    assert_eq!(*position, Some([f32::MAX, f32::MIN]));
    assert_eq!(*offset, Some([0., 3.]));
    assert_eq!(
        *fit,
        AnchoredFitMode::SnapToWindowWithMargin([0., f32::MAX, 0., 4.])
    );
    assert!(matches!(
        children[0],
        Node::Deferred {
            priority: usize::MAX,
            ..
        }
    ));
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

/// A web, camera or microphone piece the view's manifest leaves out is
/// found wherever a frame carries a node: deep in its tree, in a patch, in
/// a tooltip's content; declared, it is not.
#[test]
fn an_undeclared_piece_is_found_in_the_tree_a_patch_and_a_tooltip() {
    use crate::methods::Capability;
    let web = Node::Web {
        id: ElementIdWire::Name("page".into()),
        url: "https://example.com/".into(),
        style: StyleId(0),
        revision: 0,
        commands: Vec::new(),
        on_state: None,
    };
    let microphone = Node::Microphone {
        id: ElementIdWire::Name("mic".into()),
        style: StyleId(0),
        device: None,
        output: None,
        monitor: false,
        revision: 0,
        commands: Vec::new(),
        on_state: None,
    };
    let camera = Node::Camera {
        id: ElementIdWire::Name("camera".into()),
        style: StyleId(0),
        device: None,
        mirror: false,
        object_fit: ImageObjectFit::Contain,
        on_state: None,
    };
    let in_tree = Frame {
        root: Some(column(vec![text("a"), column(vec![web])])),
        ..Frame::default()
    };
    let in_patch = Frame {
        patches: vec![Patch::Insert {
            path: Vec::new(),
            index: 0,
            node: microphone,
        }],
        ..Frame::default()
    };
    let in_tooltip = Frame {
        tooltip_responses: vec![TooltipResponse {
            request: 0,
            character_index: None,
            content: Some(Box::new(column(vec![camera]))),
        }],
        ..Frame::default()
    };
    for (frame, gate) in [
        (in_tree, Capability::Web),
        (in_patch, Capability::Microphone),
        (in_tooltip, Capability::Camera),
    ] {
        assert_eq!(frame.undeclared(&[Capability::Host]), Some(gate));
        assert_eq!(frame.undeclared(&[Capability::Host, gate]), None);
    }
}
