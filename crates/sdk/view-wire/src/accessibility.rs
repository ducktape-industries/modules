//! The one rule for what assistive technology cannot name: the views' test
//! gate (`view-guest` testing) asks [`accessibility_faults`].

use crate::Node;

/// A node assistive technology cannot name or place.
#[derive(Clone, Debug, PartialEq)]
pub struct Fault {
    /// The `key`s from the root down to the node.
    pub path: Vec<String>,
    pub kind: FaultKind,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum FaultKind {
    /// A clickable container with a role, or an overlay, that nothing names.
    Unnamed,
    /// A container that answers a click without saying what it is.
    NoRole,
    /// A text field or editor without a label.
    UnlabeledInput,
    /// An earlier sibling already holds this `key`.
    DuplicateKey,
}

/// Every fault in the tree, depth first. An empty string is no name.
///
/// A container answers a click when its interactivity has `on_click`; it
/// is named by its aria label or by any non-empty [`Node::Text`] inside
/// it. An overlay is named by its `label` alone: the text inside a dialog
/// is what it says, not what it is. [`Node::Image`] and [`Node::Svg`] are
/// skipped: they carry no handler, so whatever makes them interactive is
/// the node that names them.
pub fn accessibility_faults(root: &Node) -> Vec<Fault> {
    let mut faults = Vec::new();
    walk(root, None, &mut faults);
    faults
}

/// The keys above a node, innermost first, kept on the walk's own stack.
struct Path<'a> {
    key: &'a str,
    parent: Option<&'a Path<'a>>,
}

impl Path<'_> {
    fn keys(&self) -> Vec<String> {
        let mut keys = Vec::new();
        let mut at = Some(self);
        while let Some(step) = at {
            keys.push(step.key.to_owned());
            at = step.parent;
        }
        keys.reverse();
        keys
    }
}

fn walk(node: &Node, parent: Option<&Path<'_>>, faults: &mut Vec<Fault>) {
    let path = Path {
        key: node.key().unwrap_or_default(),
        parent,
    };
    if let Some(kind) = fault(node) {
        faults.push(Fault {
            path: path.keys(),
            kind,
        });
    }
    let children = node.children();
    for (index, child) in children.iter().enumerate() {
        // Quadratic in a node's children, which `MAX_NODES` bounds.
        if let Some(key) = child.key()
            && children[..index]
                .iter()
                .any(|earlier| earlier.key() == Some(key))
        {
            faults.push(Fault {
                path: Path {
                    key,
                    parent: Some(&path),
                }
                .keys(),
                kind: FaultKind::DuplicateKey,
            });
        }
        walk(child, Some(&path), faults);
    }
}

fn fault(node: &Node) -> Option<FaultKind> {
    match node {
        Node::Container(crate::ContainerNode { interactivity, .. })
            if interactivity.on_click.is_some() =>
        {
            if interactivity.role.is_none() {
                return Some(FaultKind::NoRole);
            }
            let named = interactivity
                .aria
                .label
                .as_ref()
                .is_some_and(|label| !label.is_empty());
            (!named && !has_text(node)).then_some(FaultKind::Unnamed)
        }
        Node::Input { options, .. } => options
            .label
            .is_empty()
            .then_some(FaultKind::UnlabeledInput),
        Node::Editor { label, .. } => (!named(label)).then_some(FaultKind::UnlabeledInput),
        Node::Overlay { label, .. } => (!named(label)).then_some(FaultKind::Unnamed),
        _ => None,
    }
}

fn named(label: &Option<String>) -> bool {
    label.as_deref().is_some_and(|label| !label.is_empty())
}

fn has_text(node: &Node) -> bool {
    matches!(node, Node::Text (crate::TextNode { content, .. }) if !content.is_empty())
        || node.children().iter().any(has_text)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{ElementIdWire, Interactivity};
    use gpui::StyleRefinement;

    fn text(key: &str, content: &str) -> Node {
        Node::Text(crate::TextNode {
            id: Some(ElementIdWire::Name(key.into())),
            style: StyleRefinement::default(),
            content: content.into(),
        })
    }

    fn column(key: &str, children: Vec<Node>) -> Node {
        Node::Container(crate::ContainerNode {
            id: Some(ElementIdWire::Name(key.into())),
            style: StyleRefinement::default(),
            interactivity: Interactivity::default(),
            children,
        })
    }

    fn clickable(key: &str, role: Option<gpui::Role>, content: Node) -> Node {
        Node::Container(crate::ContainerNode {
            id: Some(ElementIdWire::Name(key.into())),
            style: StyleRefinement::default(),
            interactivity: Interactivity {
                role,
                on_click: Some(1),
                ..Default::default()
            },
            children: vec![content],
        })
    }

    #[test]
    fn named_trees_pass_and_unlabeled_clickables_are_reported() {
        let faulty = column(
            "App",
            vec![
                clickable("App/open", None, text("App/open/t", "Open")),
                text("App/dup", "a"),
                text("App/dup", "b"),
                Node::Overlay {
                    id: ElementIdWire::Name("ask".into()),
                    label: Some(String::new()),
                    style: Default::default(),
                    on_dismiss: None,
                    children: vec![],
                },
            ],
        );
        assert!(
            accessibility_faults(&faulty)
                .iter()
                .any(|fault| fault.kind == FaultKind::NoRole)
        );
        assert!(
            accessibility_faults(&faulty)
                .iter()
                .any(|fault| fault.kind == FaultKind::DuplicateKey)
        );
        assert!(
            accessibility_faults(&faulty)
                .iter()
                .any(|fault| fault.kind == FaultKind::Unnamed)
        );
        let named = column(
            "App",
            vec![clickable(
                "App/open",
                Some(gpui::Role::Link),
                text("App/open/t", "Open"),
            )],
        );
        assert!(accessibility_faults(&named).is_empty());
    }
}
