//! The two commonest nodes: a container and a run of text.
use crate::{ElementIdWire, Interactivity, Node, StyleId};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ContainerNode {
    pub id: Option<ElementIdWire>,
    pub style: StyleId,
    #[serde(with = "crate::style::optional")]
    pub interactivity: Option<Box<Interactivity>>,
    pub children: Vec<Node>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct TextNode {
    pub id: Option<ElementIdWire>,
    pub style: StyleId,
    /// The words as written: what the node is named by, so what a screen
    /// reader hears, and what a selection copies.
    pub content: String,
    /// A change of case the host draws `content` in; absent, it draws the
    /// words as written. Left out of the bytes when absent.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub transform: Option<TextTransform>,
}

/// A change of case that is drawing only, as CSS `text-transform` and
/// Expo's `textTransform` are: the host draws the changed words and still
/// names the node by the words as written, so a screen reader hears the
/// sentence, not a run of capitals. A transform this host does not know
/// does not decode.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum TextTransform {
    /// Every letter in capitals, as `str::to_uppercase` has them.
    Uppercase,
}
