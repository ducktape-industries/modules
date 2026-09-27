use super::*;

impl Node {
    /// The node an empty view renders as.
    pub fn empty() -> Self {
        Self::Space {
            style: gpui::StyleRefinement::default(),
        }
    }

    pub fn key(&self) -> Option<&str> {
        self.identity().and_then(ElementIdWire::name)
    }

    /// The node's typed identity, when the variant carries one.
    pub fn identity(&self) -> Option<&ElementIdWire> {
        match self {
            Self::Container(crate::ContainerNode { id, .. })
            | Self::Text(crate::TextNode { id, .. })
            | Self::Image { id, .. }
            | Self::Svg { id, .. }
            | Self::RichText { id, .. } => id.as_ref(),
            Self::Input { id, .. }
            | Self::Editor { id, .. }
            | Self::UniformList { id, .. }
            | Self::ResizeHandle { id, .. }
            | Self::Sensor { id, .. }
            | Self::Overlay { id, .. } => Some(id),
            Self::List { .. }
            | Self::Space { .. }
            | Self::Anchored { .. }
            | Self::Deferred { .. }
            | Self::Canvas { .. } => None,
        }
    }

    /// The node's children in order. One arm per variant, here and in
    /// [`Node::children_mut`] and [`Node::child_list_mut`]: everything that
    /// walks, diffs or patches a tree goes through these three, so a new
    /// variant is a new arm in each and nothing else.
    pub fn children(&self) -> &[Node] {
        match self {
            Self::Container(crate::ContainerNode { children, .. })
            | Self::Overlay { children, .. }
            | Self::List { children, .. }
            | Self::UniformList { children, .. }
            | Self::Anchored { children, .. }
            | Self::Image {
                state_children: children,
                ..
            } => children,
            Self::Deferred { content, .. }
            | Self::Sensor { child: content, .. }
            | Self::ResizeHandle { content, .. } => std::slice::from_ref(content),
            Self::RichText { .. }
            | Self::Text(crate::TextNode { .. })
            | Self::Svg { .. }
            | Self::Input { .. }
            | Self::Editor { .. }
            | Self::Space { .. }
            | Self::Canvas { .. } => &[],
        }
    }

    /// Runs `visit` on every node in the tree, depth first, this one first.
    pub fn for_each_mut(&mut self, visit: &mut impl FnMut(&mut Node)) {
        visit(self);
        for child in self.children_mut() {
            child.for_each_mut(visit);
        }
    }

    pub fn children_mut(&mut self) -> &mut [Node] {
        match self {
            Self::Container(crate::ContainerNode { children, .. })
            | Self::Overlay { children, .. }
            | Self::List { children, .. }
            | Self::UniformList { children, .. }
            | Self::Anchored { children, .. }
            | Self::Image {
                state_children: children,
                ..
            } => children,
            Self::Deferred { content, .. }
            | Self::Sensor { child: content, .. }
            | Self::ResizeHandle { content, .. } => std::slice::from_mut(content),
            Self::RichText { .. }
            | Self::Text(crate::TextNode { .. })
            | Self::Input { .. }
            | Self::Editor { .. }
            | Self::Space { .. }
            | Self::Svg { .. }
            | Self::Canvas { .. } => &mut [],
        }
    }

    /// The children as a list that can grow and shrink, for the variants
    /// that hold one; a fixed-arity node (a container's one content) has
    /// none, and no patch may insert into, remove from or move within it.
    pub fn child_list_mut(&mut self) -> Option<&mut Vec<Node>> {
        match self {
            Self::Container(crate::ContainerNode { children, .. })
            | Self::List { children, .. }
            | Self::UniformList { children, .. }
            | Self::Anchored { children, .. }
            | Self::Image {
                state_children: children,
                ..
            }
            | Self::Overlay { children, .. } => Some(children),
            Self::Deferred { .. }
            | Self::Sensor { .. }
            | Self::ResizeHandle { .. }
            | Self::RichText { .. }
            | Self::Text(crate::TextNode { .. })
            | Self::Input { .. }
            | Self::Editor { .. }
            | Self::Space { .. }
            | Self::Svg { .. }
            | Self::Canvas { .. } => None,
        }
    }

    /// Every node in the tree, depth first, this one included.
    pub fn count(&self) -> usize {
        1 + self.children().iter().map(Node::count).sum::<usize>()
    }
}
