//! Data for a QR code the host encodes and paints in its own shell.
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum QrCorrection {
    Low,
    Medium,
    Quartile,
    High,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum QrVersion {
    Normal(u8),
    Micro(u8),
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub enum QrSize {
    Cell(f32),
    Total(f32),
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Qr {
    pub payload: Option<Vec<u8>>,
    pub correction: Option<QrCorrection>,
    pub version: Option<QrVersion>,
    pub size: Option<QrSize>,
    pub cell: Option<gpui::Hsla>,
    pub background: Option<gpui::Hsla>,
}
