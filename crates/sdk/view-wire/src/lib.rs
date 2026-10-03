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
//! own handler. A text field ([`Node::Field`]) is the host engine's: it
//! sends [`Event::Text`] with the whole text it now holds, and the guest
//! asks for an edit with [`WidgetCommand::Replace`]. A rich text's
//! clickable ranges answer with [`Event::Select`] and the index of the
//! range clicked.
//!
//! The types here are the one definition of the format: the guest serializes
//! them and the host deserializes the same code, so a field neither side can
//! drop silently. A host that reads a frame from an untrusted module runs
//! [`sanitize`] first.

/// The wire this build speaks, computed by `build.rs` from the committed
/// golden files: the bytes of what the fixtures sample (`frame.bin`,
/// `methods.bin`) and the shape of every type that crosses (`schema.txt`:
/// each field and variant the tree reaches, each method's kind, target,
/// request and reply). No one bumps it by hand: a shape change fails the
/// golden until it is regenerated, and regenerating moves it. A string's
/// grammar (a colour, a length) and a method's encode function count only
/// as far as the fixtures' bytes sample them. A view's manifest carries the
/// id it was built with, and a host refuses any other.
pub const WIRE_ID: &str = include!(concat!(env!("OUT_DIR"), "/wire_id.rs"));

/// For `skip_serializing_if`: a value that says nothing is left out.
pub(crate) fn is_default<T: Default + PartialEq>(value: &T) -> bool {
    *value == T::default()
}

pub mod abi;
pub mod manifest;
pub mod methods;

mod sanitization;
pub use sanitization::SanitizeReport;

pub mod text;
pub use text::{
    Edit, KeyClaim, MAX_FIELD_BYTES, MAX_FIELD_CLAIMS, MAX_FIELD_TOKENS, TextChange, TextRange,
    TextToken, changed_span, rebase, validate_field,
};

mod image;
pub use image::ImageData;

mod snapshot;
pub use snapshot::MAX_SNAPSHOT_BYTES;

mod aria;
pub mod click;
pub use aria::{
    Aria, MAX_ARIA_ACTIONS, MAX_ARIA_CUSTOM_ACTIONS, MAX_ARIA_RELATIONS, MAX_ARIA_TEXT_BYTES,
};
/// gpui's own accessibility vocabulary, the one `Aria` and [`Event::A11yAction`] speak.
pub use gpui::accesskit::{Action, ActionData, AriaCurrent, HasPopup, Invalid, Live};
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
    Anchor, AnchoredFitMode, AnchoredPositionMode, ImageObjectFit, ImageStyle, Node, SvgSource,
    SvgTransformation,
};
mod accessibility;
pub use accessibility::{Fault, FaultKind, audit};
mod patch;
pub use patch::{MAX_PATCHES, Patch, apply, diff, diff_taking};

pub mod interactivity;
pub mod keyboard;
pub mod mouse;
pub use interactivity::{DispatchPhase, HoverListenerMode, KeyContext, Tooltip, TooltipResponse};

mod protocol;
pub use protocol::{
    Error, Event, Frame, InputOptions, MAX_CANCELS, MAX_REQUESTS, Request, RichTextHover, code,
};

mod frame_sanitize;
pub mod identity;
pub(crate) use frame_sanitize::{Budgets, finite, sanitize_tree, spend_text, truncate_to};
pub use frame_sanitize::{
    MAX_DEPTH, MAX_FRAME_BYTES, MAX_NODES, MAX_PICTURE_BYTES_PER_FRAME, MAX_PIXELS,
    MAX_STRING_BYTES, MAX_TEXT_BYTES_PER_FRAME, MAX_TEXT_PIXELS, MAX_UNIFORM_LIST_COUNT,
    MAX_UNIFORM_LIST_ROWS, Refused, sanitize, truncate_string,
};

mod codec;
pub(crate) use codec::bounded_vec;
pub use codec::{MAX_DECODED_NODES, decode, encode, encoded_size, try_encode};

#[cfg(test)]
mod tests;
