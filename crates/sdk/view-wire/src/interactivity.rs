//! Lossless payloads for GPUI element listeners.
//!
//! These values describe the platform event data that a guest callback can
//! observe. They intentionally do not contain native hitboxes, focus handles,
//! windows, or action objects: those stay on the host side of the wire.

use crate::{click, keyboard, mouse};
use gpui::{Pixels, Point};
use serde::{Deserialize, Serialize};

pub const MAX_KEY_CONTEXT_ENTRIES: usize = 64;

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct KeyContext {
    pub entries: Vec<KeyContextEntry>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct KeyContextEntry {
    pub key: gpui::SharedString,
    pub value: Option<gpui::SharedString>,
}

impl KeyContext {
    pub fn from_gpui(context: &gpui::KeyContext) -> Self {
        Self {
            entries: context
                .primary()
                .into_iter()
                .chain(context.secondary())
                .map(|entry| KeyContextEntry {
                    key: entry.key.clone(),
                    value: entry.value.clone(),
                })
                .collect(),
        }
    }

    pub fn to_gpui(&self) -> gpui::KeyContext {
        let mut context = gpui::KeyContext::default();
        for entry in &self.entries {
            if let Some(value) = &entry.value {
                context.set(entry.key.clone(), value.clone());
            } else {
                context.add(entry.key.clone());
            }
        }
        context
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum DispatchPhase {
    Capture,
    Bubble,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum TouchPhase {
    Started,
    Moved,
    Ended,
    Cancelled,
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct MouseDown {
    pub button: click::MouseButton,
    pub position: Point<Pixels>,
    pub modifiers: gpui::Modifiers,
    pub click_count: u32,
    pub first_mouse: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct MouseUp {
    pub button: click::MouseButton,
    pub position: Point<Pixels>,
    pub modifiers: gpui::Modifiers,
    pub click_count: u32,
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct MouseMove {
    pub position: Point<Pixels>,
    pub pressed_button: Option<click::MouseButton>,
    pub modifiers: gpui::Modifiers,
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct MouseExit {
    pub position: Point<Pixels>,
    pub pressed_button: Option<click::MouseButton>,
    pub modifiers: gpui::Modifiers,
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct MousePressure {
    pub pressure: f32,
    pub stage: PressureStage,
    pub position: Point<Pixels>,
    pub modifiers: gpui::Modifiers,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
pub enum PressureStage {
    #[default]
    Zero,
    Normal,
    Force,
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct ScrollWheel {
    pub position: Point<Pixels>,
    pub delta: mouse::ScrollDelta,
    pub modifiers: gpui::Modifiers,
    pub touch_phase: TouchPhase,
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct Pinch {
    pub position: Point<Pixels>,
    pub delta: f32,
    pub modifiers: gpui::Modifiers,
    pub phase: TouchPhase,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct KeyDown {
    pub state: keyboard::KeyState,
    pub repeat: bool,
    pub prefer_character_input: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct KeyUp {
    pub state: keyboard::KeyState,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ModifiersChanged {
    pub modifiers: gpui::Modifiers,
    pub capslock: bool,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum HoverListenerMode {
    #[default]
    InputModalityAware,
    InputModalityIndependent,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Tooltip {
    /// Current-frame route used to construct the tooltip after native hover.
    pub request: u32,
    /// Host-filled response cache for the currently displayed frame.
    pub content: Option<Box<crate::Node>>,
    pub hoverable: bool,
    pub delay_ms: u64,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct TooltipResponse {
    pub request: u32,
    /// Echoes the rich-text character index; ordinary tooltips use `None`.
    pub character_index: Option<u32>,
    /// `None` is an explicit result from an index-sensitive builder.
    pub content: Option<Box<crate::Node>>,
}

/// One bounded response cache for a native `InteractiveText` tooltip.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct RichTextTooltip {
    pub request: u32,
    pub character_index: Option<u32>,
    pub content: Option<Box<crate::Node>>,
}

mod convert;
