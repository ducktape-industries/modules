//! The widget tree itself: the [`Node`] enum every frame carries, and the
//! walks over it a host and the differ share.

use crate::*;
use serde::{Deserialize, Serialize};

/// How assistive technology announces a change to a [`Node::Text`] it is
/// not focused on.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub enum Live {
    /// When the reader is idle.
    Polite,
    /// At once, interrupting.
    Assertive,
}

/// The reference point used by the native GPUI anchored element.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Anchor {
    TopLeft,
    TopRight,
    BottomLeft,
    BottomRight,
    TopCenter,
    BottomCenter,
    LeftCenter,
    RightCenter,
}

/// How an anchored child is kept inside the host viewport.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub enum AnchoredFitMode {
    SnapToWindow,
    SnapToWindowWithMargin([f32; 4]),
    SwitchAnchor,
}

/// Coordinate space for an anchored position.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum AnchoredPositionMode {
    Window,
    Local,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum ImageObjectFit {
    Fill,
    Contain,
    Cover,
    ScaleDown,
    None,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ImageStyle {
    pub grayscale: bool,
    pub object_fit: ImageObjectFit,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum SvgSource {
    None,
    Data { hash: u64, bytes: Option<Vec<u8>> },
    Asset(String),
    External(String),
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct SvgTransformation {
    pub scale: [f32; 2],
    pub translate: [f32; 2],
    pub rotate: f32,
}

/// One widget. Retained elements carry their native typed identity across
/// frames for host state, focus, and accessibility ancestry.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[allow(clippy::large_enum_variant)]
pub enum Node {
    /// One native GPUI paragraph with optional interactive byte ranges.
    RichText {
        id: Option<ElementIdWire>,
        style: gpui::StyleRefinement,
        text: String,
        runs: RichTextRuns,
        font_family_overrides: Vec<(std::ops::Range<usize>, gpui::SharedString)>,
        clickable_ranges: Vec<std::ops::Range<usize>>,
        on_click: Option<u32>,
        on_hover: Option<u32>,
        /// The response cache for the paragraph's native tooltip.
        tooltip: Option<crate::TooltipResponse>,
    },
    /// A native GPUI anchored element. The host owns fitting and clipping.
    Anchored {
        anchor: Anchor,
        fit: AnchoredFitMode,
        position: Option<[f32; 2]>,
        position_mode: AnchoredPositionMode,
        offset: Option<[f32; 2]>,
        #[serde(deserialize_with = "decode_children")]
        children: Vec<Node>,
    },
    /// A GPUI uniform-height list. The host owns the native viewport; the
    /// guest carries only the row indices the host has requested.
    UniformList {
        id: ElementIdWire,
        path: Vec<ElementIdWire>,
        route: u32,
        style: gpui::StyleRefinement,
        #[serde(default, skip_serializing_if = "crate::is_default")]
        interactivity: Interactivity,
        count: usize,
        measure_index: usize,
        sizing: crate::list::UniformListSizing,
        horizontal_sizing: crate::list::UniformListHorizontalSizing,
        y_flipped: bool,
        scroll_request: Option<crate::list::UniformListScrollRequest>,
        #[serde(deserialize_with = "list::decode_indices")]
        indices: Vec<u32>,
        #[serde(deserialize_with = "decode_children")]
        children: Vec<Node>,
    },
    /// A native variable-height GPUI list with a bounded frame-owned row window.
    List {
        state: u64,
        #[serde(deserialize_with = "list::decode_path")]
        path: Vec<ElementIdWire>,
        item_count: usize,
        alignment: ListAlignment,
        overdraw: f32,
        sizing: ListSizingBehavior,
        following_tail: bool,
        revision: u64,
        #[serde(deserialize_with = "list::decode_commands")]
        commands: Vec<ListCommand>,
        request_handler: u32,
        scroll_handler: Option<u32>,
        range_start: usize,
        style: gpui::StyleRefinement,
        #[serde(deserialize_with = "decode_children")]
        children: Vec<Node>,
    },
    Container(ContainerNode),
    /// A grabbed divider: local movement deltas and native cursor; one child.
    ResizeHandle {
        id: ElementIdWire,
        style: gpui::StyleRefinement,
        on_press: Option<u32>,
        on_release: Option<u32>,
        on_drag: Option<u32>,
        cursor: Option<mouse::Cursor>,
        #[serde(deserialize_with = "decode_child")]
        content: Box<Node>,
    },
    /// A deferred draw, painted after everything in the frame that is not.
    Deferred {
        priority: usize,
        #[serde(deserialize_with = "decode_child")]
        content: Box<Node>,
    },
    /// Watches its child's laid-out size. `on_show` hears the size when the
    /// child first comes into view, `on_resize` every change after, both as
    /// [`Event::Size`].
    Sensor {
        id: ElementIdWire,
        style: gpui::StyleRefinement,
        on_show: Option<u32>,
        on_resize: Option<u32>,
        #[serde(deserialize_with = "decode_child")]
        child: Box<Node>,
    },
    Text(TextNode),
    /// A raster picture sent once per typed content hash.
    Image {
        id: Option<ElementIdWire>,
        hash: u64,
        data: Option<ImageData>,
        label: Option<String>,
        image_style: ImageStyle,
        loading: bool,
        fallback: bool,
        #[serde(deserialize_with = "decode_children")]
        state_children: Vec<Node>,
        style: gpui::StyleRefinement,
        #[serde(default, skip_serializing_if = "crate::is_default")]
        interactivity: Interactivity,
    },
    /// A vector picture. Its bytes cross ONCE: the frame that first shows a
    /// picture carries them under `hash`, and every frame after — a changed
    /// tree re-sends every node — names the hash alone. The host keeps what
    /// it decoded by hash for as long as the guest runs; a hash it has not
    /// seen draws as empty space of the node's size.
    Svg {
        id: Option<ElementIdWire>,
        source: SvgSource,
        transformation: SvgTransformation,
        label: Option<String>,
        style: gpui::StyleRefinement,
        #[serde(default, skip_serializing_if = "crate::is_default")]
        interactivity: Interactivity,
    },
    Input {
        options: InputOptions,
        id: ElementIdWire,
        placeholder: String,
        /// Copied document state, adopted by reset and host observation revision.
        value: String,
        on_input: u32,
        on_submit: Option<u32>,
        secure: bool,
        style: gpui::StyleRefinement,
    },
    /// A multiline text editor. The host owns the native editor's text and
    /// selection — native widget interaction — and the guest sees document
    /// state, unlike [`Node::Input`]. Presentation crosses as copied data.
    Editor {
        options: Box<EditorOptions>,
        id: ElementIdWire,
        style: gpui::StyleRefinement,
        placeholder: String,
        /// The accessible name.
        label: Option<String>,
        /// A shared logical document; its bytes travel only through a requested transfer.
        document: editor_document::EditorDocumentRef,
        /// Mutable guest state route, present even while editing is disabled.
        on_document: u32,
        editable: bool,
    },
    Space {
        style: gpui::StyleRefinement,
    },
    /// A base plus an optional modal layer. Closing removes the second child.
    Overlay {
        id: ElementIdWire,
        /// The accessible name of the dialog; the variant is its role.
        label: Option<String>,
        style: gpui::StyleRefinement,
        on_dismiss: Option<u32>,
        #[serde(deserialize_with = "decode_children")]
        children: Vec<Node>,
    },
    /// Bounded geometry painted by the host, in widget-local coordinates.
    Canvas {
        style: gpui::StyleRefinement,
        #[serde(deserialize_with = "canvas::decode_parts")]
        commands: Vec<CanvasCommand>,
    },
}

mod impls;
