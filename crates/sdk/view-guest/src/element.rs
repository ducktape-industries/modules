//! Guest-side GPUI-shaped authoring.
//!
//! The fluent style methods are the real `gpui::Styled` implementation. The
//! element and interaction traits are deliberately local: native GPUI
//! elements require a native layout arena, window, and application, none of
//! which exists in a wasm guest. Lowering turns this small recipe into wire
//! data once per frame.

use crate::interactivity::{EventListener, Interactivity};
use crate::{App, Window, slots, wire};
use gpui::{
    ElementId, ListHorizontalSizingBehavior, ListSizingBehavior, Overflow, ScrollStrategy,
    SharedString, StyleRefinement, Styled,
};
use std::borrow::Cow;
use std::cell::RefCell;
use std::ops::Range;
use std::rc::Rc;

/// A guest element that can be lowered by the driver.
///
/// This is intentionally a guest-side boundary with the same name as GPUI's
/// native trait. GPUI's real `Element` requires native layout and paint state;
/// a wasm guest has neither, so lowering is the only operation it can perform.
pub trait Element: 'static + IntoElement {
    /// The authored identity that enters the typed ancestry while this element lowers.
    fn id(&self) -> Option<ElementId> {
        None
    }

    #[doc(hidden)]
    fn lower(self: Box<Self>, lowering: &mut Lowering<'_>) -> wire::Node;

    #[doc(hidden)]
    fn into_any(self) -> AnyElement {
        AnyElement(Box::new(self))
    }
}

/// A value that can be converted into a guest element recipe.
pub trait IntoElement: Sized {
    type Element: Element;

    fn into_element(self) -> Self::Element;

    fn into_any_element(self) -> AnyElement {
        self.into_element().into_any()
    }
}

trait ElementObject {
    fn id(&self) -> Option<ElementId>;
    fn lower(self: Box<Self>, lowering: &mut Lowering<'_>) -> wire::Node;
}

impl<T: Element> ElementObject for T {
    fn id(&self) -> Option<ElementId> {
        Element::id(self)
    }

    fn lower(self: Box<Self>, lowering: &mut Lowering<'_>) -> wire::Node {
        Element::lower(self, lowering)
    }
}

/// A type-erased guest element, used for conditional children and components.
pub struct AnyElement(Box<dyn ElementObject>);

impl Element for AnyElement {
    fn id(&self) -> Option<ElementId> {
        self.0.id()
    }

    fn lower(self: Box<Self>, lowering: &mut Lowering<'_>) -> wire::Node {
        self.0.lower(lowering)
    }
}

impl IntoElement for AnyElement {
    type Element = Self;

    fn into_element(self) -> Self {
        self
    }

    fn into_any_element(self) -> AnyElement {
        self
    }
}

impl gpui::prelude::FluentBuilder for AnyElement {}

/// The explicit lowering context for one driver frame.
pub struct Lowering<'a> {
    window: &'a mut Window,
    app: &'a mut App,
    authored_path: Vec<wire::ElementIdWire>,
}

/// An authored [`ElementId`] as the wire carries it. Every id a view can
/// author fits; the only refusals are ids gpui itself could not name.
pub(crate) fn wire_id(id: ElementId) -> wire::ElementIdWire {
    wire::ElementIdWire::from_gpui(id)
        .expect("element ID must be portable across the view boundary")
}

impl<'a> Lowering<'a> {
    pub(crate) fn new(window: &'a mut Window, app: &'a mut App) -> Self {
        Self {
            window,
            app,
            authored_path: Vec::new(),
        }
    }

    pub(crate) fn app(&mut self) -> &mut App {
        self.app
    }

    pub(crate) fn slots(&self) -> &slots::Context {
        &self.app.inner.slots
    }

    pub(crate) fn theme(&self) -> crate::Theme {
        *self.app.global::<crate::Theme>()
    }

    pub(crate) fn parts(&mut self) -> (&mut Window, &mut App) {
        (self.window, self.app)
    }

    pub(crate) fn render_once(&mut self, component: impl RenderOnce) -> wire::Node {
        let element = component.render(self.window, self.app).into_element();
        self.lower_element(element)
    }

    pub(crate) fn lower<E: IntoElement>(&mut self, element: E) -> wire::Node {
        self.lower_element(element.into_element())
    }

    pub(crate) fn lower_element<E: Element>(&mut self, element: E) -> wire::Node {
        let id = element.id().map(wire_id);
        if let Some(id) = &id {
            self.authored_path.push(id.clone());
        }
        let node = Element::lower(Box::new(element), self);
        if id.is_some() {
            self.authored_path.pop();
        }
        node
    }

    pub(crate) fn current_path(&self) -> &[wire::ElementIdWire] {
        &self.authored_path
    }

    pub(crate) fn click(&self, listener: EventListener<gpui::ClickEvent>) -> u32 {
        slots::click(&self.app.inner.slots, listener)
    }

    pub(crate) fn route<A: 'static>(
        &self,
        listener: impl Fn(&A, &mut Window, &mut App) + 'static,
    ) -> u32 {
        slots::route(&self.app.inner.slots, listener)
    }

    pub(crate) fn enter_row(&self, key: u64) -> Option<slots::Row> {
        slots::enter_row(&self.app.inner.slots, key)
    }

    pub(crate) fn leave_row(&self, outer: Option<slots::Row>) {
        slots::leave_row(&self.app.inner.slots, outer)
    }

    pub(crate) fn message_route(
        &self,
        listener: impl Fn(&(), &mut Window, &mut App) + 'static,
    ) -> u32 {
        slots::message_route(&self.app.inner.slots, listener)
    }

    pub(crate) fn picture(&self, bytes: impl AsRef<[u8]>, cost: usize) -> (u64, Option<Vec<u8>>) {
        slots::picture(&self.app.inner.slots, bytes, cost)
    }

    pub(crate) fn tooltip(&self, build: slots::TooltipBuilder) -> u32 {
        slots::tooltip(&self.app.inner.slots, build)
    }

    pub(crate) fn rich_text_tooltip(&self, build: slots::RichTextTooltipBuilder) -> u32 {
        slots::rich_text_tooltip(&self.app.inner.slots, build)
    }
}

/// A guest container backed by a real GPUI style refinement.
#[derive(Default)]
pub struct Div {
    pub(crate) interactivity: Interactivity,
    children: Vec<AnyElement>,
}

impl Styled for Div {
    fn style(&mut self) -> &mut StyleRefinement {
        &mut self.interactivity.base_style
    }
}

impl Element for Div {
    fn id(&self) -> Option<ElementId> {
        self.interactivity.id.clone()
    }

    fn lower(self: Box<Self>, lowering: &mut Lowering<'_>) -> wire::Node {
        let Self {
            interactivity,
            children,
        } = *self;
        let id = interactivity.id.as_ref().map(|_| {
            lowering
                .current_path()
                .last()
                .cloned()
                .expect("identified div must lower inside its authored scope")
        });
        let mut style = interactivity.base_style.clone();
        if id.is_some() {
            bar_gutter(&mut style);
        }
        let (_, wire_interactivity) = interactivity.into_wire(lowering);
        let children = children
            .into_iter()
            .map(|child| lowering.lower_element(child))
            .collect();
        wire::Node::Container(crate::wire::ContainerNode {
            id,
            style,
            interactivity: wire_interactivity,
            children,
        })
    }
}

/// A scroller the host gives a vertical bar (one with an id) keeps its
/// right edge for that bar: the bar paints over the scroller, so content
/// inset less than its width would sit under it.
fn bar_gutter(style: &mut StyleRefinement) {
    use gpui::{AbsoluteLength, DefiniteLength};
    if style.overflow.y != Some(Overflow::Scroll) {
        return;
    }
    let bar = crate::design::size::SCROLLBAR;
    let right = &mut style.padding.right;
    match right {
        Some(DefiniteLength::Absolute(AbsoluteLength::Pixels(inset))) if *inset >= bar => {}
        _ => *right = Some(bar.into()),
    }
}

impl IntoElement for Div {
    type Element = Self;

    fn into_element(self) -> Self {
        self
    }
}

impl gpui::prelude::FluentBuilder for Div {}

/// Construct an empty guest container.
pub fn div() -> Div {
    Div::default()
}

/// A single-line host text input. GPUI core has no text-input element, so this
/// recipe carries a typed identity and lowers to the host's native field. Its
/// label is what assistive technology calls it: a field has one from birth.
pub struct Input {
    id: ElementId,
    value: String,
    placeholder: String,
    options: wire::InputOptions,
    secure: bool,
    style: StyleRefinement,
    on_input: Option<EventListener<String>>,
    on_submit: Option<EventListener<()>>,
}

impl Input {
    pub fn new(id: impl Into<ElementId>, label: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            value: String::new(),
            placeholder: String::new(),
            options: wire::InputOptions {
                label: label.into(),
                ..Default::default()
            },
            secure: false,
            style: StyleRefinement::default(),
            on_input: None,
            on_submit: None,
        }
    }

    pub fn value(mut self, value: impl Into<String>) -> Self {
        self.value = value.into();
        self
    }

    pub fn placeholder(mut self, placeholder: impl Into<String>) -> Self {
        self.placeholder = placeholder.into();
        self
    }

    pub fn description(mut self, description: impl Into<String>) -> Self {
        self.options.description = Some(description.into());
        self
    }

    pub fn disabled(mut self, disabled: bool) -> Self {
        self.options.disabled = disabled;
        self
    }

    /// The value is wrong; [`Self::description`] says why.
    pub fn invalid(mut self, invalid: gpui::accesskit::Invalid) -> Self {
        self.options.invalid = Some(invalid);
        self
    }

    pub fn required(mut self, required: bool) -> Self {
        self.options.required = required;
        self
    }

    pub fn read_only(mut self, read_only: bool) -> Self {
        self.options.read_only = read_only;
        self
    }

    pub fn secure(mut self, secure: bool) -> Self {
        self.secure = secure;
        self
    }

    pub fn on_input(mut self, listener: impl Fn(&String, &mut Window, &mut App) + 'static) -> Self {
        self.on_input = Some(Box::new(listener));
        self
    }

    pub fn on_submit(mut self, listener: impl Fn(&(), &mut Window, &mut App) + 'static) -> Self {
        self.on_submit = Some(Box::new(listener));
        self
    }
}

impl Styled for Input {
    fn style(&mut self) -> &mut StyleRefinement {
        &mut self.style
    }
}

impl Element for Input {
    fn lower(self: Box<Self>, lowering: &mut Lowering<'_>) -> wire::Node {
        let this = *self;
        let id = wire_id(this.id);
        let on_input = this.on_input.map(|listener| lowering.route(listener));
        let on_submit = this
            .on_submit
            .map(|listener| lowering.message_route(listener));
        wire::Node::Input {
            options: this.options,
            id,
            placeholder: this.placeholder,
            value: this.value,
            on_input,
            on_submit,
            secure: this.secure,
            style: this.style,
        }
    }
}

impl IntoElement for Input {
    type Element = Self;

    fn into_element(self) -> Self {
        self
    }
}

impl gpui::prelude::FluentBuilder for Input {}

/// Add children to an element recipe.
pub trait ParentElement {
    fn extend(&mut self, elements: impl IntoIterator<Item = AnyElement>);

    fn child(mut self, child: impl IntoElement) -> Self
    where
        Self: Sized,
    {
        self.extend(std::iter::once(child.into_any_element()));
        self
    }

    fn children(mut self, children: impl IntoIterator<Item = impl IntoElement>) -> Self
    where
        Self: Sized,
    {
        self.extend(children.into_iter().map(IntoElement::into_any_element));
        self
    }
}

impl ParentElement for Div {
    fn extend(&mut self, elements: impl IntoIterator<Item = AnyElement>) {
        self.children.extend(elements);
    }
}

impl Element for SharedString {
    fn lower(self: Box<Self>, _lowering: &mut Lowering<'_>) -> wire::Node {
        wire::Node::Text(crate::wire::TextNode {
            id: None,
            style: StyleRefinement::default(),
            content: self.to_string(),
        })
    }
}

impl Element for &'static str {
    fn lower(self: Box<Self>, lowering: &mut Lowering<'_>) -> wire::Node {
        lowering.lower((*self).to_owned())
    }
}

impl IntoElement for String {
    type Element = SharedString;

    fn into_element(self) -> Self::Element {
        self.into()
    }
}

impl IntoElement for &'static str {
    type Element = Self;

    fn into_element(self) -> Self::Element {
        self
    }
}

impl IntoElement for SharedString {
    type Element = Self;

    fn into_element(self) -> Self::Element {
        self
    }
}

impl IntoElement for Cow<'static, str> {
    type Element = SharedString;

    fn into_element(self) -> Self::Element {
        self.into()
    }
}

/// A one-shot component with the same call shape as GPUI's `RenderOnce`.
pub trait RenderOnce: 'static {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement;
}

mod uniform_list;
pub(crate) use uniform_list::UniformLists;
pub use uniform_list::{UniformList, UniformListScrollHandle, uniform_list};

#[cfg(test)]
mod tests;

#[cfg(test)]
mod ancestry_tests;

#[cfg(test)]
mod bar_gutter_tests {
    use super::*;
    use crate::prelude::*;
    use gpui::px;

    #[test]
    fn a_scroller_keeps_its_right_edge_for_the_bar() {
        let bar = crate::design::size::SCROLLBAR;
        let mut inset = crate::div().id("s").overflow_y_scroll().p(px(8.));
        let mut style = inset.style().clone();
        bar_gutter(&mut style);
        assert_eq!(style.padding.right, Some(bar.into()));
        assert_eq!(
            style.padding.left,
            Some(px(8.).into()),
            "only the bar's edge"
        );
        let mut wide = crate::div().id("s").overflow_y_scroll().pr(px(24.));
        let mut style = wide.style().clone();
        bar_gutter(&mut style);
        assert_eq!(style.padding.right, Some(px(24.).into()));
        let mut still = crate::div().p(px(8.));
        let mut style = still.style().clone();
        bar_gutter(&mut style);
        assert_eq!(
            style.padding.right,
            Some(px(8.).into()),
            "no scroll, no bar"
        );
    }
}
