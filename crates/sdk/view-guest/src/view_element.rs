use crate::element::{Element, IntoElement, Lowering, RenderOnce};
use crate::kept::ChildRenderer;
use crate::wire;
use gpui::StyleRefinement;

/// Any entity that renders, type-erased, as native GPUI's `AnyView`: a
/// child entity in its parent's tree, or a tooltip's content.
pub struct AnyView {
    render: ChildRenderer,
    id: u64,
    type_name: &'static str,
}

impl<V: crate::Render> From<crate::Entity<V>> for AnyView {
    fn from(entity: crate::Entity<V>) -> Self {
        Self {
            id: entity.id,
            type_name: std::any::type_name::<V>(),
            render: std::rc::Rc::new(move |window, app| {
                entity.update_in_window(app, window, |view, window, cx| {
                    view.render(window, cx).into_any_element()
                })
            }),
        }
    }
}

/// A child entity is a child element: `.child(self.sidebar.clone())`. It
/// renders and lowers whenever its parent does.
impl<V: crate::Render> IntoElement for crate::Entity<V> {
    type Element = ViewElement<AnyView>;

    fn into_element(self) -> Self::Element {
        AnyView::from(self).into_element()
    }
}

impl<V: crate::Render> crate::Entity<V> {
    /// This entity's subtree, kept across frames in a box styled `style`
    /// (gpui's `Entity::cached`): the entity is rendered, lowered and
    /// diffed only on a tick after it, or an entity rendered inside it,
    /// called `cx.notify()`; otherwise the last subtree stands. The box is
    /// laid out from `style` alone, never from its content, which is laid
    /// out as a root inside it: a content root that is to fill the box says
    /// `size_full()`. The style's paint half (`bg`, border, padding) is
    /// drawn on the box. A resize of the box renders nothing again (the
    /// host lays the kept tree out every frame), and a render that reads a
    /// scroll handle's value, which moves without a notify, is named by the
    /// debug check: that read belongs in an entity that renders every frame.
    ///
    /// ```
    /// # use serde::{Deserialize, Serialize};
    /// # use ducktape_view_guest::{View, prelude::*, StyleRefinement};
    /// # struct Sidebar;
    /// # impl Render for Sidebar {
    /// #     fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
    /// #         div().size_full()
    /// #     }
    /// # }
    /// # #[derive(Default, Serialize, Deserialize)]
    /// # struct Shell { #[serde(skip)] sidebar: Option<Entity<Sidebar>> }
    /// # impl View for Shell {
    /// #     const NAME: &'static str = "Shell";
    /// #     fn attach(&mut self, _: &mut Window, cx: &mut Context<Self>) {
    /// #         self.sidebar = Some(cx.new(|_| Sidebar));
    /// #     }
    /// # }
    /// # impl Render for Shell {
    /// #     fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
    /// let sidebar = self.sidebar.clone().expect("attach built it");
    /// div().child(sidebar.cached(StyleRefinement::default().w(px(240.)).h_full()))
    /// #     }
    /// # }
    /// ```
    pub fn cached(self, style: StyleRefinement) -> ViewElement<AnyView> {
        AnyView::from(self).cached(style)
    }
}

impl AnyView {
    /// [`Entity::cached`](crate::Entity::cached) on an erased entity.
    pub fn cached(self, style: StyleRefinement) -> ViewElement<AnyView> {
        let mut element = self.into_element();
        element.cached = Some(style);
        element
    }
}

impl RenderOnce for AnyView {
    fn render(self, window: &mut crate::Window, cx: &mut crate::App) -> impl IntoElement {
        (self.render)(window, cx)
    }
}

impl IntoElement for AnyView {
    type Element = ViewElement<Self>;

    fn into_element(self) -> Self::Element {
        let entity = Child {
            id: self.id,
            type_name: self.type_name,
            render: self.render.clone(),
        };
        ViewElement {
            view: self,
            entity: Some(entity),
            cached: None,
        }
    }
}

/// The entity behind a [`ViewElement`] built from one: what the lowering
/// records a child by, and runs to render it.
pub(crate) struct Child {
    pub id: u64,
    pub type_name: &'static str,
    pub render: ChildRenderer,
}

/// The guest counterpart of GPUI's `ViewElement`: it defers a `RenderOnce`
/// component until the lowering pass reaches this element.
#[doc(hidden)]
pub struct ViewElement<V: RenderOnce> {
    view: V,
    /// The entity, when the component is one: a plain child records its
    /// parent; a cached one keeps its subtree.
    entity: Option<Child>,
    /// The box a cached entity's subtree is kept in: set only through
    /// [`Entity::cached`](crate::Entity::cached) and [`AnyView::cached`],
    /// since keeping is sound only for an entity, whose `cx.notify()` is
    /// the contract.
    cached: Option<StyleRefinement>,
}

impl<V: RenderOnce> ViewElement<V> {
    #[track_caller]
    pub fn new(view: V) -> Self {
        Self {
            view,
            entity: None,
            cached: None,
        }
    }
}

impl<V: RenderOnce> Element for ViewElement<V> {
    fn defers(&self) -> bool {
        true
    }

    fn lower(self: Box<Self>, lowering: &mut Lowering<'_>) -> wire::Node {
        let this = *self;
        match (this.entity, this.cached) {
            (None, _) => lowering.render_once(this.view),
            (Some(child), None) => lowering.lower_child(&child),
            (Some(child), Some(style)) => lowering.lower_cached(&child, &style),
        }
    }
}

impl<V: RenderOnce> IntoElement for ViewElement<V> {
    type Element = Self;

    fn into_element(self) -> Self::Element {
        self
    }
}

impl<V: RenderOnce> gpui::prelude::FluentBuilder for ViewElement<V> {}
