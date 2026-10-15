use crate::*;

/// A tree deeper than this is cut off: a view cannot make the host's
/// layout recurse without bound.
pub const MAX_DEPTH: usize = 64;

/// The frame's style entries go into `styles`, the table the host holds
/// beside the tree: each is read and bounded once, here, and the frame is
/// left without them. A whole tree's entries replace the table. A node that
/// names a style the table does not hold refuses the frame, and a refused
/// frame leaves the table as it was: it is still the table of the tree the
/// host holds.
///
/// A frame that arrived as bytes has passed [`decode`] first, which refuses
/// one nested deeper than this walk goes. A frame that carries `patches`
/// instead of a tree is bounded by [`apply`], since every bound is on the
/// tree the patches make and only the host holds it.
pub fn sanitize(frame: &mut Frame, styles: &mut Styles) -> Result<SanitizeReport, Refused> {
    let entries = std::mem::take(&mut frame.styles);
    if frame.root.is_some() {
        let mut table = Styles::default();
        table.extend(entries)?;
        let report = sanitize_against(frame, &table)?;
        *styles = table;
        return Ok(report);
    }
    let held = styles.len();
    let taken = styles
        .extend(entries)
        .map_err(Refused::from)
        .and_then(|()| sanitize_against(frame, styles));
    if taken.is_err() {
        styles.truncate(held);
    }
    taken
}

/// [`sanitize`] of a frame whose style entries `styles` already holds.
fn sanitize_against(frame: &mut Frame, styles: &Styles) -> Result<SanitizeReport, Refused> {
    let mut walk = Walk::new(styles);
    if let Some(root) = &mut frame.root {
        sanitize_tree_with(root, &mut walk)?;
    }
    for response in &mut frame.tooltip_responses {
        if let Some(content) = &mut response.content {
            sanitize_tree_with(content, &mut walk)?;
        }
    }
    Ok(walk.cuts)
}

pub(crate) fn sanitize_tree(root: &mut Node, styles: &Styles) -> Result<SanitizeReport, Refused> {
    let mut walk = Walk::new(styles);
    sanitize_tree_with(root, &mut walk)?;
    Ok(walk.cuts)
}

fn sanitize_tree_with(root: &mut Node, walk: &mut Walk) -> Result<(), Refused> {
    sanitize_node(root, 0, walk, &mut identity::Scopes::default(), None)
}

/// Why the host refuses a frame or a patch: a bound or a shape it breaks,
/// or one typed id claimed twice, with the site that claimed it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Refused {
    Invalid(&'static str),
    Duplicate(identity::DuplicateIdentity),
}

impl From<&'static str> for Refused {
    fn from(reason: &'static str) -> Self {
        Self::Invalid(reason)
    }
}

impl From<identity::DuplicateIdentity> for Refused {
    fn from(duplicate: identity::DuplicateIdentity) -> Self {
        Self::Duplicate(duplicate)
    }
}

impl std::fmt::Display for Refused {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Invalid(reason) => f.write_str(reason),
            Self::Duplicate(duplicate) => duplicate.fmt(f),
        }
    }
}

impl std::error::Error for Refused {}

pub(crate) struct Walk {
    /// How many styles the tree's table holds: the ids a node may name.
    styles: usize,
    /// A node already claimed the active descendant.
    pub(crate) active_descendant: bool,
    /// The child indices from the root to the node being walked.
    pub(crate) at: Vec<u32>,
    /// What the walk has cut so far.
    pub(crate) cuts: SanitizeReport,
}

impl Walk {
    pub(crate) fn new(styles: &Styles) -> Self {
        Self {
            styles: styles.len(),
            active_descendant: false,
            at: Vec::new(),
            cuts: SanitizeReport::default(),
        }
    }

    /// A style a node names is one the table holds, or the frame is
    /// refused: there is no style to draw it with, and a default one is a
    /// look the view never wrote.
    pub(crate) fn style(&self, id: StyleId) -> Result<(), &'static str> {
        match (id.0 as usize) < self.styles {
            true => Ok(()),
            false => Err("a node names a style its table does not hold"),
        }
    }

    /// Counts `count` cuts of one kind, and where the first one fell.
    pub(crate) fn cut(&mut self, kind: fn(&mut SanitizeReport) -> &mut usize, count: usize) {
        if count == 0 {
            return;
        }
        *kind(&mut self.cuts) += count;
        if self.cuts.first.is_none() {
            self.cuts.first = Some(self.at.clone());
        }
    }
}

mod node;
use node::sanitize_node;

mod numbers;
pub(crate) use numbers::{bounded, finite};

mod interactivity;
use interactivity::sanitize_interactivity;
