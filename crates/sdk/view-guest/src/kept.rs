//! The kept table: the cached entities (`entity.cached(style)`) whose
//! subtrees the last frame holds, and how a frame fills its stand-ins from
//! that base and frees what no longer lives.
//!
//! The one invariant: an entry exists only while the base (`Driver::last_root`)
//! holds that entity's filled `Node::View`. A frame's lowering emits a hollow
//! `View` for every kept entity it met clean at the same path, and
//! [`fill`] moves each kept subtree out of the base into its stand-in before
//! anything reads the new tree; the base is then hollow exactly there, and
//! the differ emits nothing for it.
use crate::wire::{ElementIdWire, Node};
use crate::{AnyElement, App, Window};
use std::collections::{HashMap, HashSet};
use std::rc::Rc;

/// Renders a child entity: what its `View` element runs when the entity
/// is lowered, kept by the table so the debug check can run it again.
pub(crate) type ChildRenderer = Rc<dyn Fn(&mut Window, &mut App) -> AnyElement>;

/// One cached entity the base holds.
pub(crate) struct Kept {
    /// The cached entity (or the root) whose render placed it.
    pub parent: u64,
    /// The authored path its box sits at; it is reused there, under
    /// `parent`, and nowhere else.
    pub path: Vec<ElementIdWire>,
    /// The cached boundaries open around it when it was lowered, outermost
    /// first: each owner, and how many segments of `path` were in before
    /// it. The debug check re-enters them so scope numbers agree.
    #[cfg(all(debug_assertions, not(target_arch = "wasm32")))]
    pub boundaries: Vec<(usize, u64)>,
    /// Its render, for the debug check to run again.
    #[cfg(all(debug_assertions, not(target_arch = "wasm32")))]
    pub render: ChildRenderer,
    /// The entity's type, for the debug check's panic.
    #[cfg(all(debug_assertions, not(target_arch = "wasm32")))]
    pub type_name: &'static str,
    /// This frame's lowering met it (as a stand-in or lowered).
    pub seen: bool,
    /// This frame's lowering rendered it.
    pub lowered: bool,
}

/// Starts a frame: nothing met, nothing lowered yet.
pub(crate) fn begin_frame(kept: &mut HashMap<u64, Kept>) {
    for entry in kept.values_mut() {
        entry.seen = false;
        entry.lowered = false;
    }
}

/// Ends a frame: an entry lives while this frame met it, or while its
/// parent lives and was not lowered (it stands under a stand-in, with
/// everything under it); the rest are freed. Answers every live owner,
/// the root first, with whether its render ran.
pub(crate) fn end_frame(kept: &mut HashMap<u64, Kept>, root: u64) -> crate::slots::Owners {
    fn live(id: u64, kept: &HashMap<u64, Kept>, root: u64, memo: &mut HashMap<u64, bool>) -> bool {
        if id == root {
            return true;
        }
        if let Some(known) = memo.get(&id) {
            return *known;
        }
        // the root's render ran: this frame rendered
        let lowered = |id: u64| id == root || kept.get(&id).is_some_and(|entry| entry.lowered);
        let alive = kept.get(&id).is_some_and(|entry| {
            entry.seen || (live(entry.parent, kept, root, memo) && !lowered(entry.parent))
        });
        memo.insert(id, alive);
        alive
    }
    let mut memo = HashMap::new();
    let ids: Vec<u64> = kept.keys().copied().collect();
    for id in ids {
        if !live(id, kept, root, &mut memo) {
            kept.remove(&id);
        }
    }
    let mut owners: crate::slots::Owners = kept
        .iter()
        .map(|(id, entry)| (*id, entry.lowered))
        .collect();
    owners.insert(root, true);
    owners
}

/// Moves every kept subtree the new tree stands in for out of the base
/// and into its stand-in. The base is hollow exactly there afterwards.
pub(crate) fn fill(root: &mut Node, base: Option<&mut Node>) {
    let mut wanted = HashSet::new();
    root.for_each_mut(&mut |node| {
        if let Node::View {
            view,
            content: None,
            ..
        } = node
        {
            wanted.insert(*view);
        }
    });
    if wanted.is_empty() {
        return;
    }
    let mut taken: HashMap<u64, Box<Node>> = HashMap::new();
    if let Some(base) = base {
        base.for_each_mut(&mut |node| {
            if let Node::View { view, content, .. } = node
                && wanted.contains(view)
                && let Some(content) = content.take()
            {
                taken.insert(*view, content);
            }
        });
    }
    root.for_each_mut(&mut |node| {
        if let Node::View { view, content, .. } = node
            && content.is_none()
        {
            *content = taken.remove(view);
            debug_assert!(
                content.is_some(),
                "a stand-in for a kept entity the base does not hold"
            );
        }
    });
}

/// The filled `View` of `view` in `tree`, if it holds one.
#[cfg(all(debug_assertions, not(target_arch = "wasm32")))]
pub(crate) fn find(tree: &Node, view: u64) -> Option<&Node> {
    if matches!(tree, Node::View { view: held, .. } if *held == view) {
        return Some(tree);
    }
    tree.children().iter().find_map(|child| find(child, view))
}

/// Whether a scratch lowering's `shown` is the subtree the host `held`: the
/// same nodes with the same fields, a hollow view on the scratch side
/// standing for the base's filled view of that entity (that entity has a
/// check of its own).
#[cfg(all(debug_assertions, not(target_arch = "wasm32")))]
pub(crate) fn same_shown(shown: &Node, held: &Node) -> bool {
    if let (
        Node::View {
            view: a,
            style: x,
            content: None,
        },
        Node::View {
            view: b, style: y, ..
        },
    ) = (shown, held)
    {
        return a == b && x == y;
    }
    // a node's own fields, less what crosses once and is not state: a
    // uniform list's scroll request and a list's commands ride the frame
    // that carries them and are gone from the next render either way
    let own = |node: &Node| {
        let mut own = node.clone();
        for child in own.children_mut() {
            *child = Node::empty();
        }
        match &mut own {
            Node::UniformList { scroll_request, .. } => *scroll_request = None,
            Node::List { commands, .. } => commands.clear(),
            _ => {}
        }
        own
    };
    own(shown) == own(held)
        && shown.children().len() == held.children().len()
        && shown
            .children()
            .iter()
            .zip(held.children())
            .all(|(shown, held)| same_shown(shown, held))
}
