use crate::*;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct RichTextHover {
    pub index: Option<u32>,
    pub position: gpui::Point<gpui::Pixels>,
    pub pressed_button: Option<click::MouseButton>,
    pub modifiers: gpui::Modifiers,
}

/// Something the host tells the guest.
///
/// A `handler` (a `request` for a tooltip) is a route: the number the guest
/// gave a listener in the tree it sent. A route is the listener's authored
/// identity, not its position in the frame: the guest hands a number out
/// the first time it lowers that listener (the path of the element carrying
/// it, what the listener is for, and which of that kind under that path) and
/// keeps it while the element is lowered with it, so an event the host took
/// from the frame it last painted runs the same listener in the frame after
/// a structural change. A frame lowered without the listener frees its
/// route, and a freed route names nothing again.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum Event {
    /// The user activated the widget the guest gave this message route to
    /// (a button press, an input submit).
    Message(u32),
    /// A GPUI click, kept distinct from message routes and carrying its input data.
    Click {
        handler: u32,
        event: click::Click,
    },
    /// GPUI element listener payloads.
    MouseDown {
        handler: u32,
        phase: interactivity::DispatchPhase,
        event: interactivity::MouseDown,
    },
    MouseUp {
        handler: u32,
        phase: interactivity::DispatchPhase,
        event: interactivity::MouseUp,
    },
    MouseDownOut {
        handler: u32,
        event: interactivity::MouseDown,
    },
    MouseUpOut {
        handler: u32,
        event: interactivity::MouseUp,
    },
    MousePressure {
        handler: u32,
        phase: interactivity::DispatchPhase,
        event: interactivity::MousePressure,
    },
    MouseMove {
        handler: u32,
        phase: interactivity::DispatchPhase,
        event: interactivity::MouseMove,
    },
    MouseExit {
        handler: u32,
        phase: interactivity::DispatchPhase,
        event: interactivity::MouseExit,
    },
    ScrollWheel {
        handler: u32,
        phase: interactivity::DispatchPhase,
        event: interactivity::ScrollWheel,
    },
    Pinch {
        handler: u32,
        phase: interactivity::DispatchPhase,
        event: interactivity::Pinch,
    },
    KeyDown {
        handler: u32,
        phase: interactivity::DispatchPhase,
        event: interactivity::KeyDown,
    },
    KeyUp {
        handler: u32,
        phase: interactivity::DispatchPhase,
        event: interactivity::KeyUp,
    },
    ModifiersChanged {
        handler: u32,
        event: interactivity::ModifiersChanged,
    },
    Hover {
        handler: u32,
        hovered: bool,
    },
    FileDropExit {
        handler: u32,
    },
    AuxClick {
        handler: u32,
        event: click::Click,
    },
    /// Native hover asked the guest to build a tooltip for the displayed frame.
    TooltipRequest {
        request: u32,
        /// Native InteractiveText byte index; ordinary tooltips use None.
        character_index: Option<u32>,
    },
    /// A field's text moved under the host's engine (typed, pasted, undone,
    /// a guest `Replace` landed, a composition advanced): the whole text as
    /// the host now holds it, at its `revision`.
    Text {
        handler: u32,
        change: TextChange,
    },
    /// The host theme changed; the guest stores the corresponding `Theme` global.
    Theme {
        dark: bool,
    },
    /// A rich text's clickable range at `index`.
    Select {
        handler: u32,
        index: u32,
    },
    /// A native interactive-text hover changed character index.
    RichTextHover {
        handler: u32,
        event: RichTextHover,
    },
    /// A [`Node::Sensor`]'s child was measured: shown at, or resized to,
    /// `width` by `height` — the child's own laid-out size in logical
    /// pixels, never where it sits in the window. `handler` is the node's
    /// `on_show` or `on_resize`.
    ///
    /// Delivered after layout, like a DOM `ResizeObserver`: the host lays
    /// the tree out, the sensor reads its child's size, and the event goes
    /// to the guest on the next tick. A guest whose answer changes the
    /// tree so the child measures differently again is measured again;
    /// a host bounds how many times in a row that may drive a tick before
    /// it stops delivering and logs `sensor loop limit exceeded`.
    Size {
        handler: u32,
        width: f32,
        height: f32,
    },
    /// Accumulated logical-pixel movement of a grabbed resize handle.
    Drag {
        handler: u32,
        dx: f64,
        dy: f64,
    },
    /// A scrollable's content offset in logical pixels and anchor-relative
    /// fractions, emitted only when its native viewport changes. No window
    /// coordinates cross the wire.
    ScrollOffset {
        handler: u32,
        x: f32,
        y: f32,
        relative_x: f32,
        relative_y: f32,
    },
    /// The native uniform-list viewport needs these rows on the next guest
    /// frame. The route fences a stale request from an older list instance.
    UniformListRange {
        path: Vec<ElementIdWire>,
        route: u32,
        start: u32,
        end: u32,
    },
    UniformListState {
        path: Vec<ElementIdWire>,
        route: u32,
        top_index: u32,
        scrollable: bool,
        scrolled_to_end: Option<bool>,
    },
    /// A native variable-height list requested a bounded item window.
    ListRequest {
        handler: u32,
        request: ListRequest,
    },
    /// Settled native list geometry, emitted after layout state is released.
    ListScroll {
        handler: u32,
        event: ListScroll,
    },
    /// One answer to a [`Request`]. A one-shot request gets exactly one with
    /// `done`; a subscription gets many, the last one `done`.
    Response {
        id: u64,
        result: Result<Vec<u8>, Error>,
        done: bool,
    },
    /// The host no longer holds the tree the guest is patching — a patch it
    /// could not apply, a tree it dropped — and wants the next frame whole.
    Resync,
    /// An assistive-technology action request on the node that advertised
    /// `(action, handler)` in [`Aria::actions`](crate::Aria::actions).
    A11yAction {
        handler: u32,
        data: Option<ActionData>,
    },
}

/// Why a request failed, as the guest gets it: the one [`Error`] — a stable
/// snake_case [`code`] to branch on and the refusing module's `message`,
/// verbatim — so an error a module wrote and one the host wrote are the same
/// type end to end. `Display` writes `code: message`; a screen that wants the
/// message alone reads it.
pub use ::error::{Error, code};

/// Something the guest asked the host for. The guest never blocks on it: a
/// future (or stream) inside the guest waits for the matching
/// [`Event::Response`]s, which the host delivers on its own schedule.
///
/// `kind` is `<capability>.<operation>`; the host refuses a capability the
/// app's manifest did not declare.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Request {
    pub id: u64,
    pub kind: String,
    pub payload: Vec<u8>,
}

/// The most requests one frame may carry: what a host accepts per tick.
pub const MAX_REQUESTS: usize = 256;

/// The most cancels one frame may carry: a tick may cancel twice what it may
/// request, so a view dropping many live subscriptions at once is not faulted.
pub const MAX_CANCELS: usize = 2 * MAX_REQUESTS;

fn decode_patches<'de, D: serde::Deserializer<'de>>(d: D) -> Result<Vec<Patch>, D::Error> {
    crate::bounded_vec(d, MAX_PATCHES, "more patches than the host applies")
}
fn decode_tooltip_responses<'de, D: serde::Deserializer<'de>>(
    d: D,
) -> Result<Vec<TooltipResponse>, D::Error> {
    crate::bounded_vec(d, MAX_PATCHES, "too many tooltip responses")
}
fn decode_requests<'de, D: serde::Deserializer<'de>>(d: D) -> Result<Vec<Request>, D::Error> {
    crate::bounded_vec(d, MAX_REQUESTS, "too many requests")
}
fn decode_cancels<'de, D: serde::Deserializer<'de>>(d: D) -> Result<Vec<u64>, D::Error> {
    crate::bounded_vec(d, MAX_CANCELS, "too many cancels")
}

/// What one tick of the guest produced.
///
/// The tree crosses one of three ways: whole in `root`; not at all, with
/// `unchanged` set; or as `patches` against the tree the host holds, with
/// `root` empty and `unchanged` clear.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Frame {
    /// Tooltip subtrees built only after a native hover request.
    #[serde(deserialize_with = "decode_tooltip_responses")]
    pub tooltip_responses: Vec<TooltipResponse>,
    /// The tree to show. `None` with `unchanged` set means "what you have";
    /// `None` otherwise means "what you have, with `patches` applied".
    pub root: Option<Node>,
    /// Edits to the tree the host holds, in order, when `root` is `None`
    /// and `unchanged` is clear. The host applies them with [`apply`].
    #[serde(deserialize_with = "decode_patches")]
    pub patches: Vec<Patch>,
    /// What the guest asked for while producing this frame.
    #[serde(deserialize_with = "decode_requests")]
    pub requests: Vec<Request>,
    /// Requests the guest stopped waiting on — a dropped future or stream.
    /// The host frees whatever it kept for them and sends no more answers.
    #[serde(deserialize_with = "decode_cancels")]
    pub cancels: Vec<u64>,
    /// `root` is `None` because the tree is the one the guest sent last:
    /// the host keeps what it has instead of decoding it again. Requests and
    /// cancels still cross.
    pub unchanged: bool,
    /// The guest ran out of tick budget with work still ready — a task that
    /// yields more than one tick runs, a handler chain longer than one
    /// round — and wants the next tick now, not at the next event or answer.
    pub busy: bool,
}

/// Copied input accessibility and native layout options.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct InputOptions {
    pub label: String,
    pub description: Option<String>,
    pub disabled: bool,
    /// The value is wrong; `description` says why (AX-108).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub invalid: Option<Invalid>,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub required: bool,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub read_only: bool,
}
