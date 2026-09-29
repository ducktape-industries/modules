//! Every `Node` variant once, in one tree.
use super::*;

/// Every `StyleRefinement`/`TextStyleRefinement` field set once, so a
/// `#[serde(skip_serializing_if)]` or a rename on a style property shows up
/// in `frame.bin` and not only in `schema.txt`. `debug`/`debug_below` exist
/// only in a `debug_assertions` build, the same build this golden runs
/// under, so they are set behind the same `cfg`.
// One field at a time is far more readable here than one `StyleRefinement`
// literal with ~50 named fields, several nested (`overflow.x`, `text.color`).
#[allow(clippy::field_reassign_with_default)]
fn full_style() -> gpui::StyleRefinement {
    let mut style = gpui::StyleRefinement::default();
    style.display = Some(gpui::Display::Grid);
    style.visibility = Some(gpui::Visibility::Hidden);
    style.overflow.x = Some(gpui::Overflow::Scroll);
    style.overflow.y = Some(gpui::Overflow::Hidden);
    style.scrollbar_width = Some(gpui::px(4.0).into());
    style.allow_concurrent_scroll = Some(true);
    style.restrict_scroll_to_axis = Some(true);
    style.position = Some(gpui::Position::Absolute);
    style.inset.top = Some(gpui::px(1.0).into());
    style.inset.right = Some(gpui::px(1.0).into());
    style.inset.bottom = Some(gpui::px(1.0).into());
    style.inset.left = Some(gpui::px(1.0).into());
    style.size.width = Some(gpui::px(100.0).into());
    style.size.height = Some(gpui::px(50.0).into());
    style.min_size.width = Some(gpui::px(10.0).into());
    style.min_size.height = Some(gpui::px(10.0).into());
    style.max_size.width = Some(gpui::px(200.0).into());
    style.max_size.height = Some(gpui::px(200.0).into());
    style.aspect_ratio = Some(1.5);
    style.margin.top = Some(gpui::px(2.0).into());
    style.margin.right = Some(gpui::px(2.0).into());
    style.margin.bottom = Some(gpui::px(2.0).into());
    style.margin.left = Some(gpui::px(2.0).into());
    style.padding.top = Some(gpui::px(2.0).into());
    style.padding.right = Some(gpui::px(2.0).into());
    style.padding.bottom = Some(gpui::px(2.0).into());
    style.padding.left = Some(gpui::px(2.0).into());
    style.border_widths.top = Some(gpui::px(1.0).into());
    style.border_widths.right = Some(gpui::px(1.0).into());
    style.border_widths.bottom = Some(gpui::px(1.0).into());
    style.border_widths.left = Some(gpui::px(1.0).into());
    style.align_items = Some(gpui::AlignItems::Center);
    style.align_self = Some(gpui::AlignSelf::Center);
    style.align_content = Some(gpui::AlignContent::Center);
    style.justify_content = Some(gpui::JustifyContent::Center);
    style.gap.width = Some(gpui::px(2.0).into());
    style.gap.height = Some(gpui::px(2.0).into());
    style.flex_direction = Some(gpui::FlexDirection::Column);
    style.flex_wrap = Some(gpui::FlexWrap::Wrap);
    style.flex_basis = Some(gpui::px(10.0).into());
    style.flex_grow = Some(1.0);
    style.flex_shrink = Some(1.0);
    style.background = Some(gpui::Fill::Color(gpui::solid_background(gpui::red())));
    style.border_color = Some(gpui::blue());
    style.border_style = Some(gpui::BorderStyle::Dashed);
    style.corner_radii.top_left = Some(gpui::px(1.0).into());
    style.corner_radii.top_right = Some(gpui::px(1.0).into());
    style.corner_radii.bottom_left = Some(gpui::px(1.0).into());
    style.corner_radii.bottom_right = Some(gpui::px(1.0).into());
    style.box_shadow = Some(vec![gpui::BoxShadow {
        color: gpui::black(),
        offset: gpui::point(gpui::px(1.0), gpui::px(1.0)),
        blur_radius: gpui::px(2.0),
        spread_radius: gpui::px(0.0),
        inset: false,
    }]);
    style.mouse_cursor = Some(gpui::CursorStyle::PointingHand);
    style.opacity = Some(0.5);
    style.grid_cols = Some(gpui::GridTemplate {
        repeat: 2,
        min_size: gpui::GridTemplateMinSize::MinContent,
    });
    style.grid_rows = Some(gpui::GridTemplate {
        repeat: 1,
        min_size: gpui::GridTemplateMinSize::MaxContent,
    });
    style.grid_location = Some(gpui::GridLocation {
        row: gpui::GridPlacement::Line(1)..gpui::GridPlacement::Span(2),
        column: gpui::GridPlacement::Auto..gpui::GridPlacement::Auto,
    });
    #[cfg(debug_assertions)]
    {
        style.debug = Some(true);
        style.debug_below = Some(true);
    }
    style.text.color = Some(gpui::red());
    style.text.font_family = Some("Mono".into());
    style.text.font_features = Some(gpui::FontFeatures::default());
    style.text.font_fallbacks = Some(gpui::FontFallbacks::from_fonts(vec!["Sans".into()]));
    style.text.font_size = Some(gpui::px(14.0).into());
    style.text.line_height = Some(gpui::px(18.0).into());
    style.text.font_weight = Some(gpui::FontWeight::BOLD);
    style.text.font_style = Some(gpui::FontStyle::Italic);
    style.text.background_color = Some(gpui::black());
    style.text.underline = Some(gpui::UnderlineStyle {
        thickness: gpui::px(1.0),
        color: Some(gpui::red()),
        wavy: true,
    });
    style.text.strikethrough = Some(gpui::StrikethroughStyle {
        thickness: gpui::px(1.0),
        color: Some(gpui::red()),
    });
    style.text.white_space = Some(gpui::WhiteSpace::Nowrap);
    style.text.text_overflow = Some(gpui::TextOverflow::TruncateMiddle("...".into()));
    style.text.text_align = Some(gpui::TextAlign::Center);
    style.text.line_clamp = Some(3);
    style
}

/// A non-default `Interactivity`: enough that `#[serde(skip_serializing_if)]`
/// never leaves a field's presence unpinned. Reused wherever a node's own
/// `interactivity` only needs to be non-default, not this specific one.
fn full_interactivity() -> Interactivity {
    Interactivity {
        role: Some(gpui::Role::ListBox),
        aria: every_aria(),
        focusable: true,
        tab_stop: Some(true),
        tab_index: Some(1),
        tab_group: true,
        focus: Some(full_style()),
        in_focus: Some({
            let mut style = full_style();
            style.text.text_overflow = Some(gpui::TextOverflow::Truncate("…".into()));
            style
        }),
        focus_visible: Some({
            let mut style = full_style();
            style.text.text_overflow = Some(gpui::TextOverflow::TruncateStart("…".into()));
            style
        }),
        key_context: Some(interactivity::KeyContext {
            entries: vec![interactivity::KeyContextEntry {
                key: "mode".into(),
                value: Some("edit".into()),
            }],
        }),
        focus_handle: Some(42),
        occlude: true,
        block_mouse_except_scroll: true,
        hover_listener_mode: interactivity::HoverListenerMode::InputModalityIndependent,
        group: Some("card".into()),
        hover: Some(full_style()),
        active: Some(full_style()),
        group_hover: Some(GroupRefinement {
            group: "card".into(),
            style: full_style(),
        }),
        group_active: Some(GroupRefinement {
            group: "card".into(),
            style: gpui::StyleRefinement::default(),
        }),
        on_click: Some(60),
        on_aux_click: Some(61),
        on_mouse_down: Some(62),
        capture_mouse_down: Some(63),
        on_mouse_down_out: Some(64),
        on_mouse_up: Some(65),
        capture_mouse_up: Some(66),
        on_mouse_up_out: Some(67),
        on_mouse_pressure: Some(68),
        capture_mouse_pressure: Some(69),
        on_mouse_move: Some(70),
        on_mouse_exit: Some(71),
        on_scroll_wheel: Some(72),
        on_pinch: Some(73),
        capture_pinch: Some(74),
        on_key_down: Some(9),
        capture_key_down: Some(75),
        on_key_up: Some(76),
        capture_key_up: Some(77),
        on_modifiers_changed: Some(78),
        on_hover: Some(79),
        on_file_drop_exit: Some(80),
        tooltip: Some(Tooltip {
            request: 50,
            content: Some(boxed("tooltip content")),
            hoverable: true,
            delay_ms: 300,
        }),
    }
}

pub fn every_node() -> Node {
    let children = vec![
        Node::RichText {
            id: Some(ElementIdWire::View(1)),
            style: style(),
            text: "rich text".into(),
            runs: RichTextRuns::Highlights(vec![(
                0..4,
                RichTextHighlightStyle {
                    color: Some(gpui::red()),
                    font_weight: Some(gpui::FontWeight::BOLD),
                    font_style: Some(gpui::FontStyle::Italic),
                    background_color: Some(gpui::black()),
                    underline: Some(gpui::UnderlineStyle {
                        thickness: gpui::px(1.0),
                        color: Some(gpui::red()),
                        wavy: false,
                    }),
                    strikethrough: Some(gpui::StrikethroughStyle {
                        thickness: gpui::px(1.0),
                        color: Some(gpui::red()),
                    }),
                    fade_out: Some(0.5),
                },
            )]),
            font_family_overrides: vec![(Range { start: 5, end: 9 }, "Mono".into())],
            clickable_ranges: vec![0..4, 5..9],
            on_click: Some(1),
            on_hover: Some(2),
            tooltip: Some(TooltipResponse {
                request: 40,
                character_index: Some(5),
                content: Some(boxed("the paragraph's own tip")),
            }),
        },
        Node::RichText {
            id: None,
            style: style(),
            text: "runs".into(),
            // `rich_text::TextRun` is crate-private; go through gpui's own
            // (public) `TextRun` and the crate's `From` impl instead of
            // naming it.
            runs: RichTextRuns::Runs(vec![
                gpui::TextRun {
                    len: 4,
                    font: gpui::Font {
                        family: "Mono".into(),
                        features: gpui::FontFeatures::default(),
                        fallbacks: Some(gpui::FontFallbacks::from_fonts(vec!["Sans".into()])),
                        weight: gpui::FontWeight::BOLD,
                        style: gpui::FontStyle::Italic,
                    },
                    color: gpui::red(),
                    background_color: Some(gpui::black()),
                    underline: Some(gpui::UnderlineStyle {
                        thickness: gpui::px(1.0),
                        color: Some(gpui::red()),
                        wavy: true,
                    }),
                    strikethrough: Some(gpui::StrikethroughStyle {
                        thickness: gpui::px(1.0),
                        color: Some(gpui::red()),
                    }),
                }
                .into(),
            ]),
            font_family_overrides: vec![],
            clickable_ranges: vec![],
            on_click: None,
            on_hover: None,
            tooltip: None,
        },
        Node::Anchored {
            anchor: Anchor::TopLeft,
            fit: AnchoredFitMode::SnapToWindowWithMargin([1.0, 2.0, 3.0, 4.0]),
            position: Some([1.0, 2.0]),
            position_mode: AnchoredPositionMode::Window,
            offset: Some([3.0, 4.0]),
            children: vec![text("anchored")],
        },
        Node::UniformList {
            id: ElementIdWire::Integer(2),
            path: vec![id("root"), id("uniform")],
            route: 3,
            style: style(),
            interactivity: Interactivity {
                focusable: true,
                ..Default::default()
            },
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
                ListCommand::Splice {
                    start: 0,
                    end: 1,
                    count: 2,
                },
                ListCommand::Remeasure { start: 0, end: 2 },
                ListCommand::ScrollTo(ListOffset {
                    item_ix: 1,
                    offset_in_item: 4.0,
                }),
                ListCommand::ScrollToRevealItem(2),
                ListCommand::SetFollowMode { tail: true },
            ],
            request_handler: 4,
            scroll_handler: Some(5),
            range_start: 0,
            style: style(),
            interactivity: full_interactivity(),
            children: vec![text("item")],
        },
        Node::ResizeHandle {
            id: ElementIdWire::Uuid([9; 16]),
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
            id: ElementIdWire::FocusHandle(11),
            style: style(),
            on_show: Some(16),
            on_resize: Some(17),
            child: boxed("measured"),
        },
        text("plain"),
        Node::Image {
            id: Some(ElementIdWire::NamedInteger("img".into(), 3)),
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
            interactivity: Interactivity {
                focusable: true,
                ..Default::default()
            },
        },
        Node::Image {
            id: None,
            hash: 43,
            data: Some(ImageData::Encoded(vec![0, 255])),
            label: None,
            image_style: ImageStyle {
                grayscale: true,
                object_fit: ImageObjectFit::Fill,
            },
            loading: false,
            fallback: false,
            state_children: vec![],
            style: style(),
            interactivity: Interactivity::default(),
        },
        Node::Image {
            id: None,
            hash: 44,
            data: Some(ImageData::Resource("image:7".into())),
            label: None,
            image_style: ImageStyle {
                grayscale: false,
                object_fit: ImageObjectFit::Cover,
            },
            loading: true,
            fallback: false,
            state_children: vec![],
            style: style(),
            interactivity: Interactivity::default(),
        },
        Node::Image {
            id: None,
            hash: 45,
            data: Some(ImageData::Refusal("resource refused".into())),
            label: None,
            image_style: ImageStyle {
                grayscale: false,
                object_fit: ImageObjectFit::ScaleDown,
            },
            loading: false,
            fallback: true,
            state_children: vec![],
            style: style(),
            interactivity: Interactivity::default(),
        },
        Node::Svg {
            id: Some(ElementIdWire::Path(b"a/b".to_vec())),
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
            interactivity: Interactivity {
                focusable: true,
                ..Default::default()
            },
        },
        Node::Svg {
            id: None,
            source: SvgSource::Asset("icons/check.svg".into()),
            transformation: SvgTransformation {
                scale: [1.0, 1.0],
                translate: [0.0, 0.0],
                rotate: 0.0,
            },
            label: None,
            style: style(),
            interactivity: Interactivity::default(),
        },
        Node::Svg {
            id: None,
            source: SvgSource::External("https://example.com/a.svg".into()),
            transformation: SvgTransformation {
                scale: [1.0, 1.0],
                translate: [0.0, 0.0],
                rotate: 0.0,
            },
            label: None,
            style: style(),
            interactivity: Interactivity::default(),
        },
        Node::Input {
            options: view_wire::InputOptions {
                description: Some("must not be empty".into()),
                invalid: Some(Invalid::True),
                required: true,
                read_only: true,
                ..Default::default()
            },
            id: ElementIdWire::CodeLocation {
                file: "view.rs".into(),
                line: 10,
                column: 2,
            },
            placeholder: "Name".into(),
            value: "x".into(),
            on_input: Some(20),
            on_submit: Some(21),
            secure: false,
            style: style(),
        },
        Node::Editor {
            binding: Some(Box::new(EditorBinding {
                claims: vec![EditorKeyClaim {
                    key: keyboard::Key::Named(keyboard::Named::Tab),
                    modifiers: gpui::Modifiers::default(),
                    command: true,
                }],
                on_request: 90,
                on_event: 91,
            })),
            id: ElementIdWire::OpaqueId([5; 20]),
            style: style(),
            placeholder: "Notes".into(),
            label: Some("notes".into()),
            document: document(9),
            on_document: 22,
            editable: true,
        },
        Node::Space { style: style() },
        Node::Overlay {
            id: ElementIdWire::NamedChild {
                base: ElementIdAtom::Name("overlay".into()),
                names: vec!["dialog".into()],
            },
            label: Some("dialog".into()),
            style: style(),
            on_dismiss: Some(30),
            children: vec![text("base"), text("modal")],
        },
        Node::Canvas {
            style: style(),
            commands: vec![
                CanvasCommand::Push {
                    translate: [1.0, 1.0],
                    rotate: 0.1,
                    scale: [1.0, 1.0],
                    clip: Some([0.0, 0.0, 10.0, 10.0]),
                },
                CanvasCommand::Draw {
                    shape: CanvasShape::Rectangle {
                        position: [0.0, 0.0],
                        size: [4.0, 4.0],
                        radius: [1.0, 1.0, 1.0, 1.0],
                    },
                    fill: Some(gpui::red()),
                    even_odd: true,
                    stroke: Some(CanvasStroke {
                        color: gpui::black(),
                        width: 1.0,
                        cap: CanvasLineCap::Round,
                        join: CanvasLineJoin::Bevel,
                        dash: vec![1.0, 2.0],
                        dash_offset: 1,
                    }),
                },
                CanvasCommand::Draw {
                    shape: CanvasShape::Line {
                        from: [0.0, 0.0],
                        to: [4.0, 4.0],
                    },
                    fill: None,
                    even_odd: false,
                    stroke: None,
                },
                CanvasCommand::Draw {
                    shape: CanvasShape::Circle {
                        center: [4.0, 4.0],
                        radius: 3.0,
                    },
                    fill: Some(gpui::red()),
                    even_odd: false,
                    stroke: None,
                },
                CanvasCommand::Draw {
                    shape: CanvasShape::Path(vec![
                        CanvasSegment::Rectangle {
                            position: [0.0, 0.0],
                            size: [1.0, 1.0],
                            radius: [0.0, 0.0, 0.0, 0.0],
                        },
                        CanvasSegment::Circle {
                            center: [1.0, 1.0],
                            radius: 1.0,
                        },
                        CanvasSegment::Move([0.0, 0.0]),
                        CanvasSegment::Line([1.0, 1.0]),
                        CanvasSegment::Arc {
                            center: [1.0, 1.0],
                            radius: 1.0,
                            start: 0.0,
                            end: 1.0,
                        },
                        CanvasSegment::ArcTo {
                            a: [0.0, 0.0],
                            b: [1.0, 1.0],
                            radius: 0.5,
                        },
                        CanvasSegment::Ellipse {
                            center: [1.0, 1.0],
                            radius: [2.0, 1.0],
                            rotation: 0.2,
                            start: 0.0,
                            end: 1.0,
                        },
                        CanvasSegment::Bezier {
                            a: [0.0, 0.0],
                            b: [1.0, 1.0],
                            end: [2.0, 2.0],
                        },
                        CanvasSegment::Quadratic {
                            control: [1.0, 0.0],
                            end: [2.0, 1.0],
                        },
                        CanvasSegment::Close,
                    ]),
                    fill: None,
                    even_odd: false,
                    stroke: None,
                },
                CanvasCommand::Pop,
            ],
        },
    ];
    Node::Container(ContainerNode {
        id: Some(id("root")),
        style: style(),
        interactivity: Interactivity {
            focusable: true,
            ..Default::default()
        },
        children,
    })
}

/// Every `Aria` field the host reads past phase 1, set: `Invalid::True` and
/// `AriaCurrent::False` are the strings `"true"` and `"false"`, not booleans.
fn every_aria() -> Aria {
    Aria {
        author_id: Some("author-1".into()),
        label: Some("rows".into()),
        description: Some("a list of rows".into()),
        keyshortcuts: Some("Ctrl+K".into()),
        active_descendant: true,
        value: Some("42".into()),
        placeholder: Some("type here".into()),
        selected: Some(true),
        expanded: Some(true),
        disabled: Some(false),
        numeric_value: Some(3.0),
        numeric_value_step: Some(1.0),
        min_numeric_value: Some(0.0),
        max_numeric_value: Some(10.0),
        level: Some(1),
        position_in_set: Some(2),
        size_of_set: Some(5),
        row_index: Some(0),
        column_index: Some(1),
        row_count: Some(3),
        column_count: Some(4),
        toggled: Some(gpui::Toggled::Mixed),
        orientation: Some(gpui::Orientation::Horizontal),
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
