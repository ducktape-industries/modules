//! GPUI-styled wire payloads; the host assigns these refinements unchanged
//! after the frame sanitizer has checked them.
use crate::{ElementIdWire, Interactivity, Node};
use gpui::{StyleRefinement, Styled};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct ContainerNode {
    pub id: Option<ElementIdWire>,
    pub style: StyleRefinement,
    #[serde(default, skip_serializing_if = "crate::is_default")]
    pub interactivity: Box<Interactivity>,
    pub children: Vec<Node>,
}
impl Styled for ContainerNode {
    fn style(&mut self) -> &mut StyleRefinement {
        &mut self.style
    }
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct TextNode {
    pub id: Option<ElementIdWire>,
    pub style: StyleRefinement,
    pub content: String,
}
impl Styled for TextNode {
    fn style(&mut self) -> &mut StyleRefinement {
        &mut self.style
    }
}
