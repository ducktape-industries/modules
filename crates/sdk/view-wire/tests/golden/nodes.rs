//! Every `Node` variant once, in one tree.
use super::*;

pub fn every_node() -> Node {
    let children = vec![
        Node::RichText {
            id: Some(id("rich")),
            style: style(),
            text: "rich text".into(),
            runs: RichTextRuns::Highlights(vec![(
                0..4,
                RichTextHighlightStyle {
                    color: None,
                    font_weight: Some(gpui::FontWeight::BOLD),
                    font_style: None,
                    background_color: None,
                    underline: None,
                    strikethrough: None,
                    fade_out: None,
                },
            )]),
            font_family_overrides: vec![(Range { start: 5, end: 9 }, "Mono".into())],
            clickable_ranges: vec![0..4, 5..9],
            on_click: Some(1),
            on_hover: Some(2),
            tooltip: None,
        },
        Node::Anchored {
            anchor: Anchor::TopLeft,
            fit: AnchoredFitMode::SnapToWindow,
            position: Some([1.0, 2.0]),
            position_mode: AnchoredPositionMode::Window,
            offset: Some([3.0, 4.0]),
            children: vec![text("anchored")],
        },
        Node::UniformList {
            id: id("uniform"),
            path: vec![id("root"), id("uniform")],
            route: 3,
            style: style(),
            interactivity: Interactivity::default(),
            count: 100,
            measure_index: 0,
            sizing: UniformListSizing::Auto,
            horizontal_sizing: UniformListHorizontalSizing::FitList,
            y_flipped: false,
            scroll_request: Some(UniformListScrollRequest {
                index: 7,
                strategy: UniformListScrollStrategy::Center,
                offset: 0,
                strict: true,
            }),
            indices: vec![0, 1],
            children: vec![text("row 0"), text("row 1")],
        },
        Node::List {
            state: 9,
            path: vec![id("root"), id("list")],
            item_count: 3,
            alignment: ListAlignment::Bottom,
            overdraw: 200.0,
            sizing: ListSizingBehavior::Infer,
            following_tail: true,
            revision: 2,
            commands: vec![
                ListCommand::Reset { count: 3 },
                ListCommand::ScrollTo(ListOffset {
                    item_ix: 1,
                    offset_in_item: 4.0,
                }),
            ],
            request_handler: 4,
            scroll_handler: Some(5),
            range_start: 0,
            style: style(),
            interactivity: every_aria(),
            children: vec![text("item")],
        },
        Node::ResizeHandle {
            id: id("handle"),
            style: style(),
            interactivity: Interactivity {
                role: Some(gpui::Role::Splitter),
                aria: Aria {
                    label: Some("Resize".into()),
                    orientation: Some(gpui::Orientation::Vertical),
                    ..Default::default()
                },
                focusable: true,
                on_key_down: Some(9),
                ..Default::default()
            },
            on_press: Some(6),
            on_release: Some(7),
            on_drag: Some(8),
            cursor: Some(mouse::Cursor::ResizingColumn),
            content: boxed("divider"),
        },
        Node::Deferred {
            priority: 1,
            content: boxed("later"),
        },
        Node::Sensor {
            id: id("sensor"),
            style: style(),
            on_show: Some(16),
            on_resize: Some(17),
            child: boxed("measured"),
        },
        text("plain"),
        Node::Image {
            id: Some(id("image")),
            hash: 42,
            data: Some(ImageData::Rgba {
                width: 1,
                height: 1,
                pixels: vec![255, 0, 0, 255],
            }),
            label: Some("a pixel".into()),
            image_style: ImageStyle {
                grayscale: false,
                object_fit: ImageObjectFit::Contain,
            },
            loading: false,
            fallback: false,
            state_children: vec![text("while loading")],
            style: style(),
            interactivity: Interactivity::default(),
        },
        Node::Svg {
            id: Some(id("svg")),
            source: SvgSource::Data {
                hash: 44,
                bytes: Some(b"<svg/>".to_vec()),
            },
            transformation: SvgTransformation {
                scale: [1.0, 1.0],
                translate: [0.0, 0.0],
                rotate: 0.0,
            },
            label: Some("a picture".into()),
            style: style(),
            interactivity: Interactivity::default(),
        },
        Node::Input {
            options: view_wire::InputOptions {
                invalid: Some(Invalid::True),
                required: true,
                read_only: true,
                ..Default::default()
            },
            id: id("input"),
            placeholder: "Name".into(),
            value: "x".into(),
            on_input: Some(20),
            on_submit: Some(21),
            secure: false,
            style: style(),
        },
        Node::Editor {
            binding: None,
            id: id("editor"),
            style: style(),
            placeholder: "Notes".into(),
            label: Some("notes".into()),
            document: document(9),
            on_document: 22,
            editable: true,
        },
        Node::Space { style: style() },
        Node::Overlay {
            id: id("overlay"),
            label: Some("dialog".into()),
            style: style(),
            on_dismiss: Some(30),
            children: vec![text("base"), text("modal")],
        },
        Node::Canvas {
            style: style(),
            commands: vec![CanvasCommand::Draw {
                shape: CanvasShape::Circle {
                    center: [4.0, 4.0],
                    radius: 3.0,
                },
                fill: Some(gpui::red()),
                even_odd: false,
                stroke: None,
            }],
        },
    ];
    Node::Container(ContainerNode {
        id: Some(id("root")),
        style: style(),
        interactivity: Interactivity::default(),
        children,
    })
}

/// Every `Aria` field the host reads past phase 1, set: `Invalid::True` and
/// `AriaCurrent::False` are the strings `"true"` and `"false"`, not booleans.
fn every_aria() -> Interactivity {
    Interactivity {
        role: Some(gpui::Role::ListBox),
        aria: Aria {
            label: Some("rows".into()),
            live: Some(Live::Polite),
            busy: true,
            required: true,
            read_only: true,
            invalid: Some(Invalid::True),
            has_popup: Some(HasPopup::Listbox),
            current: Some(AriaCurrent::False),
            labelled_by: vec![vec![id("root"), id("caption")]],
            described_by: vec![vec![id("root"), id("hint")]],
            controls: vec![vec![id("root"), id("panel")]],
            error_message: Some(vec![id("root"), id("error")]),
            actions: vec![(Action::ScrollIntoView, 10), (Action::CustomAction, 11)],
            custom_actions: vec![(3, "Pin".into())],
            ..Default::default()
        },
        focusable: true,
        ..Default::default()
    }
}

pub fn node_variant(node: &Node) -> &'static str {
    match node {
        Node::RichText { .. } => "RichText",
        Node::Anchored { .. } => "Anchored",
        Node::UniformList { .. } => "UniformList",
        Node::List { .. } => "List",
        Node::Container(_) => "Container",
        Node::ResizeHandle { .. } => "ResizeHandle",
        Node::Deferred { .. } => "Deferred",
        Node::Sensor { .. } => "Sensor",
        Node::Text(_) => "Text",
        Node::Image { .. } => "Image",
        Node::Svg { .. } => "Svg",
        Node::Input { .. } => "Input",
        Node::Editor { .. } => "Editor",
        Node::Space { .. } => "Space",
        Node::Overlay { .. } => "Overlay",
        Node::Canvas { .. } => "Canvas",
    }
}
