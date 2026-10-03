//! Where the keyboard is, by the host's rules: a node holds focus when a
//! test focuses it, when a press lands on it or inside it, when the view
//! moves focus there itself (`Window::focus`, `FocusHandle::focus`), or
//! when a dialog opens and takes the keyboard; it loses focus when it
//! leaves the tree. Keys go down the focus path and back up it, and
//! nowhere else.

use super::{authored_path, chain};
use crate::wire::{ElementIdWire, Node, WidgetCommand};

/// The node that holds focus, as the host keeps it: by the ids it is filed
/// under, or by the view's own focus handle.
#[derive(Clone, Debug, PartialEq)]
pub(super) enum Focus {
    Path(Vec<ElementIdWire>),
    Handle(u64),
}

impl Focus {
    /// The chain from the root down to the focused node, while it is in
    /// the tree.
    pub(super) fn chain<'a>(&self, root: &'a Node) -> Option<Vec<&'a Node>> {
        chain(root, &mut |chain| {
            let node = chain.last().unwrap();
            match self {
                Self::Path(path) => {
                    node.identity().is_some() && authored_path(chain).ends_with(path)
                }
                Self::Handle(handle) => node
                    .interactivity()
                    .is_some_and(|interactivity| interactivity.focus_handle == Some(*handle)),
            }
        })
    }

    /// Focus on the last node of `chain`.
    pub(super) fn at(chain: &[&Node]) -> Self {
        let node = chain.last().expect("a node");
        match node.interactivity().and_then(|i| i.focus_handle) {
            Some(handle) => Self::Handle(handle),
            None => Self::Path(authored_path(chain)),
        }
    }

    /// Where a widget command the view sent moves focus, if it does. The
    /// host focuses any container a `Focus` names, and ignores one it
    /// cannot find.
    pub(super) fn moved_by(command: &WidgetCommand, root: &Node) -> Option<Self> {
        let focus = match command {
            WidgetCommand::Focus { target } => Self::Path(target.clone()),
            WidgetCommand::FocusHandle { handle } => Self::Handle(*handle),
            WidgetCommand::FocusNext | WidgetCommand::FocusPrevious => {
                panic!("the test host does not model `{command:?}`: no view sends it yet")
            }
            _ => return None,
        };
        focus.chain(root).is_some().then_some(focus)
    }
}

/// Whether a person can put the keyboard on `node`: by click or Tab.
pub(super) fn holds_focus(node: &Node) -> bool {
    match node {
        Node::Input { .. } | Node::Editor { .. } => true,
        _ => node.interactivity().is_some_and(|interactivity| {
            interactivity.focusable || interactivity.focus_handle.is_some()
        }),
    }
}

/// Whether Tab stops at `node`.
fn tab_stop(node: &Node) -> bool {
    match node {
        Node::Input { .. } | Node::Editor { .. } => true,
        _ => node.interactivity().is_some_and(|interactivity| {
            interactivity
                .tab_stop
                .unwrap_or(interactivity.tab_index.is_some())
                && holds_focus(node)
        }),
    }
}

/// Where a press on the last node of `chain` puts the keyboard: on the
/// innermost node of the chain that can hold it (gpui's mouse-down rule).
pub(super) fn pressed(chain: &[&Node]) -> Option<Focus> {
    (1..=chain.len())
        .rev()
        .find(|end| holds_focus(chain[end - 1]))
        .map(|end| Focus::at(&chain[..end]))
}

/// The dialogs open in `root`, by the ids they are filed under: named
/// overlays showing their modal layer.
pub(super) fn open_dialogs(root: &Node) -> Vec<Vec<ElementIdWire>> {
    let mut out = Vec::new();
    chain(root, &mut |chain| {
        if let Node::Overlay {
            label: Some(_),
            children,
            ..
        } = chain.last().unwrap()
            && children.len() == 2
        {
            out.push(authored_path(chain));
        }
        false
    });
    out
}

/// Where a dialog that just opened at `dialog` puts the keyboard: on its
/// first Tab stop, unless focus is in the dialog already.
pub(super) fn entered(
    root: &Node,
    dialog: &[ElementIdWire],
    focus: Option<&Focus>,
) -> Option<Focus> {
    // in the modal layer, the overlay's second child, not the base under it
    let inside = |chain: &[&Node]| {
        chain
            .windows(2)
            .enumerate()
            .any(|(at, pair)| match pair[0] {
                Node::Overlay { children, .. } => {
                    children.len() == 2
                        && std::ptr::eq(pair[1], &children[1])
                        && authored_path(&chain[..=at]) == dialog
                }
                _ => false,
            })
    };
    if focus
        .and_then(|focus| focus.chain(root))
        .is_some_and(|chain| inside(&chain))
    {
        return None;
    }
    // ponytail: Tab order is document order here; gpui also orders by
    // `tab_index` and Tab groups, which no shipped dialog uses.
    chain(root, &mut |chain| {
        inside(chain) && tab_stop(chain.last().unwrap())
    })
    .map(|chain| Focus::at(&chain))
}
