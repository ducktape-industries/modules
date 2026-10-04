//! A view's tests: [`TestAppContext`] drives one view as the app does, over
//! a [`FakeHost`] that answers its requests by their shape, and holds every
//! frame to the host's rules: its sanitizer, the accessibility audit, the
//! per-frame request budget, and keys delivered only along the focus path.
//!
//! ```
//! # use serde::{Deserialize, Serialize};
//! # use view_guest::prelude::*;
//! # use view_guest::{View, testing::TestAppContext};
//! #[derive(Default, Serialize, Deserialize)]
//! struct Rows;
//! impl View for Rows {
//!     const NAME: &'static str = "Rows";
//! }
//! impl Render for Rows {
//!     fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
//!         uniform_list("rows", 100, |range, _, _| {
//!             range.map(|row| div().id(row).child(format!("row {row}"))).collect()
//!         })
//!     }
//! }
//! let mut cx = TestAppContext::new();
//! cx.open::<Rows>();
//! // the first frame holds the one row the host measures
//! assert_eq!(cx.texts(), ["row 0"]);
//! // a pane ten rows tall asks for the rows it shows
//! cx.simulate_viewport(10);
//! assert_eq!(cx.texts().len(), 10);
//! ```

use crate::wire::{Node, TooltipResponse};

/// Every text the tree shows, depth first: text nodes and the value or
/// placeholder of a field (the view's copy of it, which in a test is the
/// host's).
pub(crate) fn texts(root: Option<&Node>) -> Vec<String> {
    let mut out = Vec::new();
    if let Some(root) = root {
        collect_texts(root, &mut out);
    }
    out
}

fn collect_texts(node: &Node, out: &mut Vec<String>) {
    match node {
        Node::RichText { text, .. } => out.push(text.clone()),
        Node::Text(crate::wire::TextNode { content, .. }) => out.push(content.clone()),
        Node::Field {
            value: text,
            placeholder,
            ..
        } => out.push(if text.is_empty() {
            placeholder.clone()
        } else {
            text.clone()
        }),
        _ => node
            .children()
            .iter()
            .for_each(|child| collect_texts(child, out)),
    }
}

/// What the host's sanitizer answered for a frame, which it must take
/// whole: no refusal, and nothing cut (a clamp is no cut).
pub(crate) fn assert_taken_whole(taken: Result<crate::wire::SanitizeReport, crate::wire::Refused>) {
    match taken {
        Err(refused) => panic!("the host refuses this frame: {refused}"),
        Ok(cut) if !cut.is_empty() => {
            panic!("the host would cut this frame, it is past a frame budget: {cut:?}")
        }
        Ok(_) => {}
    }
}

/// [`assert_accessible`] on the tree the host holds and on the content of
/// every tooltip a frame answered, which the host renders too.
pub(crate) fn assert_frame_accessible(root: Option<&Node>, tooltips: &[TooltipResponse]) {
    let tooltips = tooltips
        .iter()
        .filter_map(|response| response.content.as_deref());
    root.into_iter().chain(tooltips).for_each(assert_accessible);
}

/// Panics listing each node assistive technology cannot name, place or
/// reach, by its key path and fault.
pub(crate) fn assert_accessible(tree: &Node) {
    let faults = crate::wire::audit(tree);
    assert!(
        faults.is_empty(),
        "{} accessibility fault(s):\n{}",
        faults.len(),
        faults
            .iter()
            .map(|fault| format!("  {:?} at {}", fault.kind, fault.path.join(" > ")))
            .collect::<Vec<_>>()
            .join("\n")
    );
}

/// The nodes from the root down to the first node, depth first, whose
/// chain `matches` accepts.
pub(crate) fn chain<'a>(
    root: &'a Node,
    matches: &mut dyn FnMut(&[&'a Node]) -> bool,
) -> Option<Vec<&'a Node>> {
    fn walk<'a>(
        node: &'a Node,
        chain: &mut Vec<&'a Node>,
        matches: &mut dyn FnMut(&[&'a Node]) -> bool,
    ) -> bool {
        chain.push(node);
        if matches(chain)
            || node
                .children()
                .iter()
                .any(|child| walk(child, chain, matches))
        {
            return true;
        }
        chain.pop();
        false
    }
    let mut out = Vec::new();
    walk(root, &mut out, matches).then_some(out)
}

/// The chain down to the node under `key`.
pub(crate) fn chain_to<'a>(root: &'a Node, key: &str) -> Option<Vec<&'a Node>> {
    chain(root, &mut |chain| chain.last().unwrap().key() == Some(key))
}

/// The ids the host files a node under: its own and its ancestors', a
/// list row's index for a row with none ([`crate::wire::identity`]).
pub(crate) fn authored_path(chain: &[&Node]) -> Vec<crate::wire::ElementIdWire> {
    let row = |at: usize| {
        let parent = chain[at.checked_sub(1)?];
        let index = parent
            .children()
            .iter()
            .position(|child| std::ptr::eq(child, chain[at]))?;
        crate::wire::identity::row(parent, index)
    };
    (0..chain.len())
        .filter_map(|at| crate::wire::identity::segment(chain[at].identity().cloned(), row(at)))
        .collect()
}

/// The button whose key, label or accessible name is `name`.
pub(crate) fn button<'a>(root: &'a Node, name: &str) -> Option<Vec<&'a Node>> {
    chain(root, &mut |chain| match chain.last().unwrap() {
        node @ Node::Container(crate::wire::ContainerNode { interactivity, .. })
            if interactivity.on_click.is_some() =>
        {
            let mut labels = Vec::new();
            collect_texts(node, &mut labels);
            node.key() == Some(name)
                || interactivity.aria.label.as_deref() == Some(name)
                || labels.iter().any(|label| label == name)
        }
        _ => false,
    })
}

/// The field whose key or placeholder is `name`.
pub(crate) fn input<'a>(root: &'a Node, name: &str) -> Option<Vec<&'a Node>> {
    chain(root, &mut |chain| match chain.last().unwrap() {
        Node::Field {
            id, placeholder, ..
        } => id.name() == Some(name) || placeholder == name,
        _ => false,
    })
}

/// Every node key in the tree, depth first.
pub(crate) fn keys(root: Option<&Node>) -> Vec<String> {
    fn collect(node: &Node, out: &mut Vec<String>) {
        if let Some(key) = node.key() {
            out.push(key.to_string());
        }
        node.children().iter().for_each(|child| collect(child, out));
    }
    let mut out = Vec::new();
    if let Some(root) = root {
        collect(root, &mut out);
    }
    out
}

mod context;
mod fake_host;
mod focus;
pub use context::{TestAppContext, TickReport};
pub use fake_host::{FakeHost, StreamSender};

/// A program for the SDK's own tests to follow with `Changes<Probe>`.
#[cfg(test)]
pub(crate) struct Probe;
#[cfg(test)]
impl crate::methods::Program for Probe {
    const NAME: &'static str = "probe";
    type Op = ();
    type Query = ();
    type Reply = ();
}
