//! What sanitizing a frame cut: how much of each kind, and where the first
//! cut fell. A clamp is not a cut: a number made finite is the value the
//! host draws. A cut is something the frame said that the host does not
//! show at all: a child its parent has no place for, or a subtree nested
//! past [`crate::MAX_DEPTH`]. No user text is retained.

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SanitizeReport {
    pub nodes: usize,
    /// Subtrees cut off at [`crate::MAX_DEPTH`].
    pub depth: usize,
    /// Where the first cut fell: the child indices from the root, or from
    /// a tooltip's content when the cut fell inside one.
    pub first: Option<Vec<u32>>,
}

impl SanitizeReport {
    /// Nothing was cut.
    pub fn is_empty(&self) -> bool {
        *self == Self::default()
    }
    pub fn merge(&mut self, other: Self) {
        self.nodes += other.nodes;
        self.depth += other.depth;
        if self.first.is_none() {
            self.first = other.first;
        }
    }
}
