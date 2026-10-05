use super::*;

impl Node {
    /// The node an empty view renders as.
    pub fn empty() -> Self {
        Self::Space
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
            Self::Field { id, .. }
            | Self::UniformList { id, .. }
            | Self::List { id, .. }
            | Self::ResizeHandle { id, .. }
            | Self::Sensor { id, .. }
            | Self::Overlay { id, .. } => Some(id),
            Self::Space
            | Self::Anchored { .. }
            | Self::Deferred { .. }
            | Self::View { .. }
            | Self::Canvas { .. } => None,
        }
    }

    /// The listener routes, focus and aria of the six variants that carry
    /// them.
    pub fn interactivity(&self) -> Option<&crate::Interactivity> {
        match self {
            Self::Container(crate::ContainerNode { interactivity, .. })
            | Self::UniformList { interactivity, .. }
            | Self::List { interactivity, .. }
            | Self::ResizeHandle { interactivity, .. }
            | Self::Image { interactivity, .. }
            | Self::Svg { interactivity, .. } => interactivity.as_deref(),
            _ => None,
        }
    }

    /// The style the node was authored with, by its id in the tree's
    /// table; `Anchored`, `Deferred` and `Space` carry none.
    pub fn style(&self) -> Option<StyleId> {
        match self {
            Self::Container(crate::ContainerNode { style, .. })
            | Self::Text(crate::TextNode { style, .. })
            | Self::RichText { style, .. }
            | Self::UniformList { style, .. }
            | Self::List { style, .. }
            | Self::ResizeHandle { style, .. }
            | Self::Sensor { style, .. }
            | Self::Image { style, .. }
            | Self::Svg { style, .. }
            | Self::Field { style, .. }
            | Self::Overlay { style, .. }
            | Self::View { style, .. }
            | Self::Canvas { style, .. } => Some(*style),
            Self::Anchored { .. } | Self::Deferred { .. } | Self::Space => None,
        }
    }

    /// Hands `visit` every style id the node itself names, its own and its
    /// conditional ones, to read or to renumber
    /// ([`Interner::retain`](crate::Interner::retain)).
    pub fn styles_mut(&mut self, visit: &mut dyn FnMut(&mut StyleId)) {
        match self {
            Self::Container(crate::ContainerNode { style, .. })
            | Self::Text(crate::TextNode { style, .. })
            | Self::RichText { style, .. }
            | Self::UniformList { style, .. }
            | Self::List { style, .. }
            | Self::ResizeHandle { style, .. }
            | Self::Sensor { style, .. }
            | Self::Image { style, .. }
            | Self::Svg { style, .. }
            | Self::Field { style, .. }
            | Self::Overlay { style, .. }
            | Self::View { style, .. }
            | Self::Canvas { style, .. } => visit(style),
            Self::Anchored { .. } | Self::Deferred { .. } | Self::Space => {}
        }
        if let Self::Container(crate::ContainerNode {
            interactivity: Some(interactivity),
            ..
        })
        | Self::UniformList {
            interactivity: Some(interactivity),
            ..
        }
        | Self::List {
            interactivity: Some(interactivity),
            ..
        }
        | Self::ResizeHandle {
            interactivity: Some(interactivity),
            ..
        }
        | Self::Image {
            interactivity: Some(interactivity),
            ..
        }
        | Self::Svg {
            interactivity: Some(interactivity),
            ..
        } = self
        {
            interactivity.style_slots().for_each(visit);
        }
    }

    /// The text the node itself draws: a text's content, a rich text's
    /// text. A field's text is the host engine's.
    pub fn text(&self) -> Option<&str> {
        match self {
            Self::Text(crate::TextNode { content, .. }) => Some(content),
            Self::RichText { text, .. } => Some(text),
            _ => None,
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
            Self::View { content, .. } => content.as_deref().map_or(&[], std::slice::from_ref),
            Self::RichText { .. }
            | Self::Text(crate::TextNode { .. })
            | Self::Svg { .. }
            | Self::Field { .. }
            | Self::Space
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
            Self::View { content, .. } => {
                content.as_deref_mut().map_or(&mut [], std::slice::from_mut)
            }
            Self::RichText { .. }
            | Self::Text(crate::TextNode { .. })
            | Self::Field { .. }
            | Self::Space
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
            | Self::View { .. }
            | Self::Sensor { .. }
            | Self::ResizeHandle { .. }
            | Self::RichText { .. }
            | Self::Text(crate::TextNode { .. })
            | Self::Field { .. }
            | Self::Space
            | Self::Svg { .. }
            | Self::Canvas { .. } => None,
        }
    }

    /// Every node in the tree, depth first, this one included.
    pub fn count(&self) -> usize {
        1 + self.children().iter().map(Node::count).sum::<usize>()
    }
}
