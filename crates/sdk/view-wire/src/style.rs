//! The style refinements and listener routes an element lowers to native GPUI.
use gpui::{SharedString, StyleRefinement};
use serde::{Deserialize, Serialize};

/// Declarative interactivity lowered into native GPUI's `Interactivity`.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Interactivity {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub role: Option<gpui::Role>,
    #[serde(skip_serializing_if = "crate::is_default")]
    pub aria: crate::Aria,
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub focusable: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tab_stop: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tab_index: Option<i32>,
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub tab_group: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub focus: Option<Box<StyleRefinement>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub in_focus: Option<Box<StyleRefinement>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub focus_visible: Option<Box<StyleRefinement>>,
    /// Guest-app-local opaque focus allocation. It is never an authored element ID.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub focus_handle: Option<u64>,
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub occlude: bool,
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub block_mouse_except_scroll: bool,
    #[serde(skip_serializing_if = "crate::is_default")]
    pub hover_listener_mode: crate::HoverListenerMode,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub group: Option<SharedString>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hover: Option<Box<StyleRefinement>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub active: Option<Box<StyleRefinement>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub group_hover: Option<GroupRefinement>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub group_active: Option<GroupRefinement>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub on_click: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub on_aux_click: Option<u32>,
    /// The click consumes its press: the host stops the pointer's click at
    /// this node, so nothing under it hears the same press.
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub consumes_click: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub on_mouse_down: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub capture_mouse_down: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub on_mouse_down_out: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub on_mouse_up: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub capture_mouse_up: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub on_mouse_up_out: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub on_mouse_pressure: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub capture_mouse_pressure: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub on_mouse_move: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub on_mouse_exit: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub on_scroll_wheel: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub on_pinch: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub capture_pinch: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub on_key_down: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub capture_key_down: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub on_key_up: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub capture_key_up: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub on_modifiers_changed: Option<u32>,
    /// Keystrokes in gpui's words (`"escape"`, `"shift-tab"`) this node
    /// takes, with its key-down listeners or its keyboard click: the host
    /// stops each one here once they have heard it.
    #[serde(
        skip_serializing_if = "Vec::is_empty",
        deserialize_with = "crate::interactivity::decode_consumed_keys"
    )]
    pub consumes_keys: Vec<SharedString>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub on_hover: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub on_file_drop_exit: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tooltip: Option<crate::Tooltip>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct GroupRefinement {
    pub group: SharedString,
    pub style: Box<StyleRefinement>,
}
