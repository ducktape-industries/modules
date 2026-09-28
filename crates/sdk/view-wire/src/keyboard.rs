//! The keys of a keystroke, named as gpui names them; its modifiers are
//! gpui's own.
use serde::{Deserialize, Serialize};

/// Every key gpui names in a `Keystroke` besides a printable character:
/// what a host can send, so what a guest can match on.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Named {
    Enter,
    Tab,
    Space,
    Escape,
    Backspace,
    Delete,
    Insert,
    ArrowUp,
    ArrowDown,
    ArrowLeft,
    ArrowRight,
    Home,
    End,
    PageUp,
    PageDown,
    Shift,
    Control,
    Alt,
    Super,
    Fn,
    BrowserBack,
    BrowserForward,
    F1,
    F2,
    F3,
    F4,
    F5,
    F6,
    F7,
    F8,
    F9,
    F10,
    F11,
    F12,
    F13,
    F14,
    F15,
    F16,
    F17,
    F18,
    F19,
    F20,
    F21,
    F22,
    F23,
    F24,
    F25,
    F26,
    F27,
    F28,
    F29,
    F30,
    F31,
    F32,
    F33,
    F34,
    F35,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Location {
    Standard,
    Left,
    Right,
    Numpad,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Key {
    Named(Named),
    Character(String),
    Unidentified,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum NativeCode {
    Unidentified,
    Android(u32),
    MacOS(u16),
    Windows(u16),
    Xkb(u32),
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Physical {
    Unidentified(NativeCode),
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct KeyState {
    pub key: Key,
    pub modified_key: Key,
    pub physical_key: Physical,
    pub location: Location,
    pub modifiers: gpui::Modifiers,
}
