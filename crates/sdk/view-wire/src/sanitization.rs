//! What sanitizing a frame cut: how much of each kind, and where the first
//! cut fell. A clamp is not a cut: a number held to its range, or a field
//! held to its own small bound (an aria label, a font name), is the value
//! the host draws. A cut is something the frame said that the host does
//! not show at all. No user text is retained.

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SanitizeReport {
    /// Nodes dropped past [`crate::MAX_NODES`], each dropped subtree
    /// counted whole.
    pub nodes: usize,
    /// Subtrees cut off at [`crate::MAX_DEPTH`].
    pub depth: usize,
    /// Strings cut at [`crate::MAX_STRING_BYTES`].
    pub strings: usize,
    /// Shaped strings cut by the frame's text budget
    /// ([`crate::MAX_TEXT_BYTES_PER_FRAME`]).
    pub text: usize,
    /// Pictures dropped by the frame's picture budget
    /// ([`crate::MAX_PICTURE_BYTES_PER_FRAME`]).
    pub pictures: usize,
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
        self.strings += other.strings;
        self.text += other.text;
        self.pictures += other.pictures;
        if self.first.is_none() {
            self.first = other.first;
        }
    }
}
