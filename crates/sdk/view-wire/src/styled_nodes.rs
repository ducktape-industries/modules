//! The two commonest nodes: a container and a run of text.
use crate::{ElementIdWire, Interactivity, Node, StyleId};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ContainerNode {
    pub id: Option<ElementIdWire>,
    pub style: StyleId,
    pub interactivity: Box<Interactivity>,
    pub children: Vec<Node>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct TextNode {
    pub id: Option<ElementIdWire>,
    pub style: StyleId,
    pub content: String,
}
