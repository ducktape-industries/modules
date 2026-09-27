//! The wire between a host and a view running in wasm.
//!
//! The guest ships a WIDGET TREE, not a picture: every tick it returns the
//! [`Node`] its view built, with every value inlined — text, colours, sizes —
//! and the host's own toolkit does layout, render, fonts, IME, clipboard and
//! scroll. The guest never learns where anything landed, which is the point:
//! there is nothing in it to draw with.
//!
//! Interaction goes back as MEANING, not input. An element's
//! [`Interactivity`] carries, per listener (`on_click`, `on_mouse_down`,
//! `on_key_down`, ...), the index of the handler the guest registered this
//! frame; the host sends [`Event::Click`], [`Event::MouseDown`],
//! [`Event::KeyDown`] and the rest with that index and the guest runs its
//! own handler. A text field carries a handler index; the host owns the text
//! and sends [`Event::Input`] with what it now reads; a multiline editor the
//! same, with [`Event::EditorTransaction`]. A rich text's clickable ranges
//! answer with [`Event::Select`] and the index of the range clicked.
//!
//! The types here are the one definition of the format: the guest serializes
//! them and the host deserializes the same code, so a field neither side can
//! drop silently. A host that reads a frame from an untrusted module runs
//! [`sanitize`] first.

/// Exact named-MessagePack protocol implemented by this build. Bump on serialized shape changes,
/// in the SAME commit as the shape change: a view built against the old shape is
/// refused at load instead of faulting on its first frame.
/// This is independent of the calling convention ([`abi`]) and the manifest text format.
/// `tests/golden.rs` holds the bytes of every node, event and method: it fails on
/// any change and says to bump this and regenerate with `WIRE_GOLDEN_WRITE=1`.
/// Within an epoch the methods only grow; a moved or dropped method is a new epoch.
pub const WIRE_EPOCH: u32 = 2;

/// For `skip_serializing_if`: a value that says nothing is left out.
pub(crate) fn is_default<T: Default + PartialEq>(value: &T) -> bool {
    *value == T::default()
}

pub mod abi;
pub mod manifest;
pub mod methods;

mod sanitization;
pub use sanitization::SanitizeReport;

mod subscription;
pub mod task;
pub use subscription::{Recipe, Subscription};
pub use task::Task;

mod editor;
pub mod editor_document;
pub mod editor_presentation;
pub mod editor_rich;
pub mod editor_transaction;
pub use editor_transaction::{
    EditorBinding, EditorDecision, EditorEditKind, EditorFault, EditorHistoryEffect,
    EditorKeyClaim, EditorPatch, EditorPatchError, EditorRequest, EditorRequestInput,
    EditorResponse, EditorTransactionEvent, EditorTransactionId, patched_editor_text,
};

pub use editor::{EditorCursor, EditorPosition, editor_lines, editor_offset, editor_position};

mod image;
pub use image::ImageData;

mod snapshot;
pub use snapshot::MAX_SNAPSHOT_BYTES;

mod aria;
pub mod click;
pub use aria::Aria;
mod element_id;
pub use element_id::{ElementIdAtom, ElementIdWire, MAX_ELEMENT_ID_DEPTH};
mod style;
mod style_sanitize;
pub use style::{GroupRefinement, Interactivity};

mod qr;
pub use qr::{Qr, QrCorrection, QrSize, QrVersion};
mod rich_text;
pub use rich_text::{HighlightStyle as RichTextHighlightStyle, Runs as RichTextRuns};
mod canvas;
pub mod list;
pub use canvas::{
    CanvasCommand, CanvasLineCap, CanvasLineJoin, CanvasSegment, CanvasShape, CanvasStroke,
    MAX_CANVAS_PARTS,
};
pub use list::{
    ListAlignment, ListCommand, ListKey, ListOffset, ListRequest, ListScroll, ListSizingBehavior,
    MAX_LIST_COMMANDS, MAX_LIST_ITEMS, MAX_LIST_ROWS,
};
mod widget;
pub use widget::{WidgetCommand, WidgetTarget};

mod styled_nodes;
pub use styled_nodes::{ContainerNode, TextNode};
mod node;
pub use node::{
    Anchor, AnchoredFitMode, AnchoredPositionMode, ImageObjectFit, ImageStyle, Live, Node,
    SvgSource, SvgTransformation,
};
mod accessibility;
pub use accessibility::{Fault, FaultKind, accessibility_faults};
mod patch;
pub use patch::{MAX_PATCHES, Patch, apply, diff};

pub mod interactivity;
pub mod keyboard;
pub mod mouse;
pub use interactivity::{DispatchPhase, HoverListenerMode, KeyContext, Tooltip, TooltipResponse};

mod protocol;
pub use protocol::{
    EditorOptions, Error, Event, Frame, InputOptions, Request, RichTextHover, code,
};

mod frame_sanitize;
pub(crate) use frame_sanitize::{Budgets, finite, sanitize_tree, spend_text, truncate_to};
pub use frame_sanitize::{
    MAX_DEPTH, MAX_FRAME_BYTES, MAX_NODES, MAX_PICTURE_BYTES_PER_FRAME, MAX_PIXELS,
    MAX_STRING_BYTES, MAX_TEXT_BYTES_PER_FRAME, MAX_TEXT_PIXELS, MAX_UNIFORM_LIST_COUNT,
    MAX_UNIFORM_LIST_ROWS, sanitize,
};

mod codec;
pub use codec::{MAX_DECODED_NODES, decode, encode, encoded_size};
pub(crate) use codec::{bounded_vec, budget, decode_child, decode_children};

#[cfg(test)]
mod tests;
