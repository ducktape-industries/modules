//! The stateful half of gpui's interaction API: [`Stateful`] and the
//! setters gpui keeps on [`StatefulInteractiveElement`].
use super::{InteractiveElement, Interactivity, TooltipBuilder};
use crate::{
    AnyElement, AnyView, App, Element, IntoElement, Lowering, ParentElement, Window, wire,
};
use gpui::{ClickEvent, ElementId, SharedString, StyleRefinement, Styled};
use std::time::Duration;

/// The stateful wrapper returned by [`InteractiveElement::id`].
pub struct Stateful<E> {
    pub(crate) element: E,
}

impl<E: Styled> Styled for Stateful<E> {
    fn style(&mut self) -> &mut StyleRefinement {
        self.element.style()
    }
}

impl<E: Element> Element for Stateful<E> {
    fn id(&self) -> Option<ElementId> {
        self.element.id()
    }

    fn lower(self: Box<Self>, lowering: &mut Lowering<'_>) -> wire::Node {
        Element::lower(Box::new(self.element), lowering)
    }
}

impl<E: Element> IntoElement for Stateful<E> {
    type Element = Self;

    fn into_element(self) -> Self {
        self
    }
}

impl<E: ParentElement> ParentElement for Stateful<E> {
    fn extend(&mut self, elements: impl IntoIterator<Item = AnyElement>) {
        self.element.extend(elements);
    }
}

impl<E: InteractiveElement> InteractiveElement for Stateful<E> {
    fn interactivity(&mut self) -> &mut Interactivity {
        self.element.interactivity()
    }
}

/// Stateful interaction methods with GPUI's public names and signatures where
/// the value can be represented by the guest wire contract.
pub trait StatefulInteractiveElement: InteractiveElement {
    fn role(mut self, role: gpui::Role) -> Self {
        self.interactivity().role = Some(role);
        self
    }
    fn focusable(mut self) -> Self {
        self.interactivity().focusable = true;
        self
    }
    fn accessibility_id(mut self, id: impl Into<SharedString>) -> Self {
        self.interactivity().aria.author_id = Some(id.into());
        self
    }
    fn aria_label(mut self, value: impl Into<SharedString>) -> Self {
        self.interactivity().aria.label = Some(value.into());
        self
    }
    fn aria_description(mut self, value: impl Into<SharedString>) -> Self {
        self.interactivity().aria.description = Some(value.into());
        self
    }
    fn aria_keyshortcuts(mut self, value: impl Into<SharedString>) -> Self {
        self.interactivity().aria.keyshortcuts = Some(value.into());
        self
    }
    fn aria_active_descendant(mut self) -> Self {
        self.interactivity().aria.active_descendant = true;
        self
    }
    fn aria_value(mut self, value: impl Into<SharedString>) -> Self {
        self.interactivity().aria.value = Some(value.into());
        self
    }
    fn aria_placeholder(mut self, value: impl Into<SharedString>) -> Self {
        self.interactivity().aria.placeholder = Some(value.into());
        self
    }
    fn aria_selected(mut self, value: bool) -> Self {
        self.interactivity().aria.selected = Some(value);
        self
    }
    fn aria_expanded(mut self, value: bool) -> Self {
        self.interactivity().aria.expanded = Some(value);
        self
    }
    fn aria_disabled(mut self, value: bool) -> Self {
        self.interactivity().aria.disabled = Some(value);
        self
    }
    fn aria_numeric_value(mut self, value: f64) -> Self {
        self.interactivity().aria.numeric_value = Some(value);
        self
    }
    fn aria_numeric_value_step(mut self, value: f64) -> Self {
        self.interactivity().aria.numeric_value_step = Some(value);
        self
    }
    fn aria_min_numeric_value(mut self, value: f64) -> Self {
        self.interactivity().aria.min_numeric_value = Some(value);
        self
    }
    fn aria_max_numeric_value(mut self, value: f64) -> Self {
        self.interactivity().aria.max_numeric_value = Some(value);
        self
    }
    fn aria_level(mut self, value: usize) -> Self {
        self.interactivity().aria.level = Some(value);
        self
    }
    fn aria_position_in_set(mut self, value: usize) -> Self {
        self.interactivity().aria.position_in_set = Some(value);
        self
    }
    fn aria_size_of_set(mut self, value: usize) -> Self {
        self.interactivity().aria.size_of_set = Some(value);
        self
    }
    fn aria_row_index(mut self, value: usize) -> Self {
        self.interactivity().aria.row_index = Some(value);
        self
    }
    fn aria_column_index(mut self, value: usize) -> Self {
        self.interactivity().aria.column_index = Some(value);
        self
    }
    fn aria_row_count(mut self, value: usize) -> Self {
        self.interactivity().aria.row_count = Some(value);
        self
    }
    fn aria_column_count(mut self, value: usize) -> Self {
        self.interactivity().aria.column_count = Some(value);
        self
    }
    fn aria_toggled(mut self, value: gpui::Toggled) -> Self {
        self.interactivity().aria.toggled = Some(value);
        self
    }
    fn aria_orientation(mut self, value: gpui::Orientation) -> Self {
        self.interactivity().aria.orientation = Some(value);
        self
    }
    fn overflow_scroll(mut self) -> Self {
        self.interactivity().base_style.overflow.x = Some(gpui::Overflow::Scroll);
        self.interactivity().base_style.overflow.y = Some(gpui::Overflow::Scroll);
        self
    }
    fn overflow_x_scroll(mut self) -> Self {
        self.interactivity().base_style.overflow.x = Some(gpui::Overflow::Scroll);
        self
    }
    fn overflow_y_scroll(mut self) -> Self {
        self.interactivity().base_style.overflow.y = Some(gpui::Overflow::Scroll);
        self
    }
    fn restrict_scroll_to_axis(mut self) -> Self {
        self.interactivity().base_style.restrict_scroll_to_axis = Some(true);
        self
    }
    fn active(mut self, f: impl FnOnce(StyleRefinement) -> StyleRefinement) -> Self {
        self.interactivity().active = Some(f(StyleRefinement::default()));
        self
    }
    fn group_active(
        mut self,
        group: impl Into<SharedString>,
        f: impl FnOnce(StyleRefinement) -> StyleRefinement,
    ) -> Self {
        self.interactivity().group_active = Some((group.into(), f(StyleRefinement::default())));
        self
    }
    fn on_click(mut self, listener: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static) -> Self {
        self.interactivity().on_click = Some(Box::new(listener));
        self
    }
    fn on_aux_click(
        mut self,
        listener: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
    ) -> Self {
        self.interactivity().on_aux_click = Some(Box::new(listener));
        self
    }
    fn on_hover(mut self, listener: impl Fn(&bool, &mut Window, &mut App) + 'static) -> Self {
        self.interactivity().on_hover = Some(Box::new(listener));
        self
    }
    fn hover_listener_mode(mut self, mode: gpui::HoverListenerMode) -> Self {
        self.interactivity().hover_listener_mode = mode;
        self
    }
    fn tooltip(
        mut self,
        build_tooltip: impl Fn(&mut Window, &mut App) -> AnyView + 'static,
    ) -> Self {
        self.interactivity().tooltip = Some(TooltipBuilder {
            build: Box::new(build_tooltip),
            hoverable: false,
        });
        self
    }
    fn hoverable_tooltip(
        mut self,
        build_tooltip: impl Fn(&mut Window, &mut App) -> AnyView + 'static,
    ) -> Self {
        self.interactivity().tooltip = Some(TooltipBuilder {
            build: Box::new(build_tooltip),
            hoverable: true,
        });
        self
    }
    fn tooltip_show_delay(mut self, delay: Duration) -> Self {
        self.interactivity().tooltip_show_delay = Some(delay);
        self
    }
}

impl<T: InteractiveElement> StatefulInteractiveElement for Stateful<T> {}

impl<E> gpui::prelude::FluentBuilder for Stateful<E> {}
