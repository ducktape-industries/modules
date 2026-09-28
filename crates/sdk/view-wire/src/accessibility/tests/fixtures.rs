//! The trees the rules are tried on.
use super::*;
use crate::InputOptions;
use gpui::StyleRefinement;

pub(super) fn text(content: &str) -> Node {
    Node::Text(TextNode {
        id: None,
        style: StyleRefinement::default(),
        content: content.into(),
    })
}

pub(super) fn el(key: &str, interactivity: Interactivity, children: Vec<Node>) -> Node {
    Node::Container(ContainerNode {
        id: Some(ElementIdWire::Name(key.into())),
        style: StyleRefinement::default(),
        interactivity,
        children,
    })
}

pub(super) fn roled(role: Role) -> Interactivity {
    Interactivity {
        role: Some(role),
        ..Default::default()
    }
}

/// Roled, focusable and answering a click.
pub(super) fn control(role: Role) -> Interactivity {
    Interactivity {
        focusable: true,
        on_click: Some(1),
        ..roled(role)
    }
}

pub(super) fn labelled(interactivity: Interactivity, label: &str) -> Interactivity {
    Interactivity {
        aria: Aria {
            label: Some(label.into()),
            ..interactivity.aria
        },
        ..interactivity
    }
}

pub(super) fn button(key: &str, label: &str) -> Node {
    el(key, control(Role::Button), vec![text(label)])
}

pub(super) fn input(label: &str) -> Node {
    Node::Input {
        options: InputOptions {
            label: label.into(),
            ..Default::default()
        },
        id: ElementIdWire::Name("name".into()),
        placeholder: String::new(),
        value: String::new(),
        on_input: Some(1),
        on_submit: None,
        secure: false,
        style: StyleRefinement::default(),
    }
}

/// A paragraph with one clickable byte range.
pub(super) fn rich(text: &str, clickable: std::ops::Range<usize>) -> Node {
    Node::RichText {
        id: None,
        style: StyleRefinement::default(),
        text: text.into(),
        runs: Default::default(),
        font_family_overrides: Vec::new(),
        clickable_ranges: vec![clickable],
        on_click: Some(1),
        on_hover: None,
        tooltip: None,
    }
}

pub(super) fn handle(interactivity: Interactivity) -> Node {
    Node::ResizeHandle {
        id: ElementIdWire::Name("divider".into()),
        style: StyleRefinement::default(),
        interactivity,
        on_press: None,
        on_release: None,
        on_drag: Some(1),
        cursor: None,
        content: Box::new(Node::empty()),
    }
}

pub(super) fn image(interactivity: Interactivity, label: Option<&str>) -> Node {
    Node::Image {
        id: None,
        hash: 1,
        data: None,
        label: label.map(Into::into),
        image_style: crate::ImageStyle {
            grayscale: false,
            object_fit: crate::ImageObjectFit::Contain,
        },
        loading: false,
        fallback: false,
        state_children: Vec::new(),
        style: StyleRefinement::default(),
        interactivity,
    }
}
