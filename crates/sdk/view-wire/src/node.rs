//! The widget tree itself: the [`Node`] enum every frame carries, and the
//! walks over it a host and the differ share.

use crate::*;
use serde::{Deserialize, Serialize};

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
///
/// The shape is derived; `Serialize` and `Deserialize` are the codec's
/// (`codec.rs`), which decodes every node inside the frame's budget.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(remote = "Self")]
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
        interactivity: Box<Interactivity>,
        count: usize,
        measure_index: usize,
        sizing: crate::list::UniformListSizing,
        horizontal_sizing: crate::list::UniformListHorizontalSizing,
        y_flipped: bool,
        scroll_request: Option<crate::list::UniformListScrollRequest>,
        #[serde(deserialize_with = "list::decode_indices")]
        indices: Vec<u32>,
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
        #[serde(default, skip_serializing_if = "crate::is_default")]
        interactivity: Box<Interactivity>,
        children: Vec<Node>,
    },
    Container(ContainerNode),
    /// A grabbed divider: local movement deltas and native cursor; one child.
    ResizeHandle {
        id: ElementIdWire,
        style: gpui::StyleRefinement,
        #[serde(default, skip_serializing_if = "crate::is_default")]
        interactivity: Box<Interactivity>,
        on_press: Option<u32>,
        on_release: Option<u32>,
        on_drag: Option<u32>,
        cursor: Option<mouse::Cursor>,
        content: Box<Node>,
    },
    /// A deferred draw, painted after everything in the frame that is not.
    Deferred {
        priority: usize,
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
        state_children: Vec<Node>,
        style: gpui::StyleRefinement,
        #[serde(default, skip_serializing_if = "crate::is_default")]
        interactivity: Box<Interactivity>,
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
        interactivity: Box<Interactivity>,
    },
    /// A text field, one line or many, whose text the host's editing engine
    /// owns. `value`, `cursor` and `tokens` are the guest's copy: the host
    /// adopts them on a fresh mount and whenever `generation` moves (a reset
    /// the guest means), and otherwise reports its own text through
    /// `on_change` as [`Event::Text`]. `revision` is the host's, as of the
    /// last change the guest applied, so the host can let go of the edits
    /// before it. A key in `claims` reaches the guest as [`Event::KeyDown`]
    /// on `on_key` instead of editing; the engine's own keys cannot be
    /// claimed ([`KeyClaim::engine_owned`]). Enter in a one-line field is
    /// `on_submit`, as [`Event::Message`].
    Field {
        id: ElementIdWire,
        multiline: bool,
        value: String,
        cursor: TextRange,
        generation: u64,
        revision: u64,
        #[serde(deserialize_with = "text::decode_token_slice")]
        tokens: Box<[TextToken]>,
        #[serde(deserialize_with = "text::decode_claim_slice")]
        claims: Box<[KeyClaim]>,
        options: Box<InputOptions>,
        placeholder: String,
        secure: bool,
        on_change: Option<u32>,
        on_key: Option<u32>,
        on_submit: Option<u32>,
        style: gpui::StyleRefinement,
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
