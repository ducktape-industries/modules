//! Wire-backed behavior elements that have no native GPUI element equivalent.
use crate::Element;

use crate::element::wire_id;
use crate::interactivity::EventListener;
use crate::{
    AnyElement, App, ElementId, InteractiveElement, Interactivity, IntoElement, Lowering,
    ParentElement, StatefulInteractiveElement, Window, div, wire,
};
use gpui::{Bounds, CursorStyle, Hsla, Pixels, StyleRefinement, Styled};

/// Tells a view where layout put `child`: gpui hands an element its bounds
/// in its prepaint and paint closures, which cannot cross to a guest, so
/// the host tells them as an event.
pub struct Sensor {
    id: ElementId,
    child: AnyElement,
    on_bounds: Option<EventListener<Bounds<Pixels>>>,
    style: Box<StyleRefinement>,
}

pub fn sensor(id: impl Into<ElementId>, child: impl IntoElement) -> Sensor {
    Sensor {
        id: id.into(),
        child: child.into_any_element(),
        on_bounds: None,
        style: Box::default(),
    }
}

impl Sensor {
    /// Hears the child's bounds, in the window's pixels (the pixels a
    /// mouse event's `position` is in, so `event.position - bounds.origin`
    /// is the pointer inside the child): when the child comes into view,
    /// and whenever its origin or its size differs from the last heard
    /// while it is in view. Until the first, layout has not run: the view
    /// has no bounds to draw from.
    pub fn on_bounds(
        mut self,
        listener: impl Fn(&Bounds<Pixels>, &mut Window, &mut App) + 'static,
    ) -> Self {
        self.on_bounds = Some(Box::new(listener));
        self
    }
}

impl IntoElement for Sensor {
    type Element = Self;
    fn into_element(self) -> Self {
        self
    }
}

impl Element for Sensor {
    fn id(&self) -> Option<ElementId> {
        Some(self.id.clone())
    }

    fn lower(self: Box<Self>, lowering: &mut Lowering<'_>) -> wire::Node {
        wire::Node::Sensor {
            id: wire_id(self.id),
            on_bounds: self
                .on_bounds
                .map(|listener| lowering.route(crate::slots::Kind::Bounds, listener)),
            child: Box::new(lowering.lower(self.child)),
            style: lowering.style(&self.style),
        }
    }
}

impl Styled for Sensor {
    fn style(&mut self) -> &mut StyleRefinement {
        &mut self.style
    }
}

pub struct ResizeHandle {
    interactivity: Box<Interactivity>,
    child: AnyElement,
    on_drag: Option<EventListener<(Pixels, Pixels)>>,
    cursor: Option<CursorStyle>,
}

pub fn resize_handle(id: impl Into<ElementId>, child: impl IntoElement) -> ResizeHandle {
    let mut interactivity = Box::<Interactivity>::default();
    interactivity.id = Some(id.into());
    ResizeHandle {
        interactivity,
        child: child.into_any_element(),
        on_drag: None,
        cursor: Some(CursorStyle::ResizeLeftRight),
    }
}

impl ResizeHandle {
    pub fn on_drag(
        mut self,
        listener: impl Fn(&(Pixels, Pixels), &mut Window, &mut App) + 'static,
    ) -> Self {
        self.on_drag = Some(Box::new(listener));
        self
    }

    pub fn cursor(mut self, cursor: CursorStyle) -> Self {
        self.cursor = Some(cursor);
        self
    }
}

impl IntoElement for ResizeHandle {
    type Element = Self;
    fn into_element(self) -> Self {
        self
    }
}

impl InteractiveElement for ResizeHandle {
    fn interactivity(&mut self) -> &mut Interactivity {
        &mut self.interactivity
    }
}
impl StatefulInteractiveElement for ResizeHandle {}

impl Element for ResizeHandle {
    fn id(&self) -> Option<ElementId> {
        self.interactivity.id.clone()
    }

    fn lower(self: Box<Self>, lowering: &mut Lowering<'_>) -> wire::Node {
        let style = lowering.style(&self.interactivity.base_style);
        let interactivity = self.interactivity.into_wire(lowering);
        wire::Node::ResizeHandle {
            id: lowering.own_id(),
            on_press: None,
            on_release: None,
            on_drag: self
                .on_drag
                .map(|listener| lowering.route(crate::slots::Kind::Drag, listener)),
            cursor: self.cursor.map(wire_cursor),
            content: Box::new(lowering.lower(self.child)),
            style,
            interactivity,
        }
    }
}

pub struct ModalOverlay {
    id: ElementId,
    base: AnyElement,
    modal: Option<AnyElement>,
    label: String,
    style: Box<StyleRefinement>,
    backdrop: Hsla,
    on_dismiss: Option<EventListener<()>>,
}

/// `modal` over `base`, named `label`: what the dialog is, as assistive
/// technology announces it. An open modal has a name: an empty `label`
/// panics, where the host would draw the modal as no dialog at all.
///
/// `base` sits under `id` whether or not a modal is open: the host keeps a
/// list's scroll and a field's state by the ids above it, so a modal
/// opening must not move the base to a new path. With no modal it is a
/// plain full-size container; the overlay's style, backdrop and dismiss
/// apply only while one is open.
pub fn modal_overlay(
    id: impl Into<ElementId>,
    label: impl Into<String>,
    base: impl IntoElement,
    modal: Option<impl IntoElement>,
) -> ModalOverlay {
    let label = label.into();
    assert!(
        modal.is_none() || !label.is_empty(),
        "an open modal_overlay is a dialog, and a dialog has a name: its label is empty"
    );
    ModalOverlay {
        id: id.into(),
        base: base.into_any_element(),
        modal: modal.map(IntoElement::into_any_element),
        label,
        style: Box::default(),
        backdrop: Hsla::transparent_black(),
        on_dismiss: None,
    }
}

impl ModalOverlay {
    pub fn backdrop(mut self, color: impl Into<Hsla>) -> Self {
        self.backdrop = color.into();
        self
    }
    pub fn on_dismiss(mut self, listener: impl Fn(&(), &mut Window, &mut App) + 'static) -> Self {
        self.on_dismiss = Some(Box::new(listener));
        self
    }
}

impl IntoElement for ModalOverlay {
    type Element = Self;
    fn into_element(self) -> Self {
        self
    }
}

impl Element for ModalOverlay {
    fn id(&self) -> Option<ElementId> {
        Some(self.id.clone())
    }

    fn lower(self: Box<Self>, lowering: &mut Lowering<'_>) -> wire::Node {
        let Some(modal) = self.modal else {
            // the id is already on the path: lower the container in place
            let base = div().id(self.id).size_full().child(self.base);
            return Element::lower(Box::new(base), lowering);
        };
        wire::Node::Overlay {
            id: wire_id(self.id),
            label: Some(self.label),
            style: lowering.style(&self.style.bg(self.backdrop)),
            on_dismiss: self
                .on_dismiss
                .map(|listener| lowering.route(crate::slots::Kind::Dismiss, listener)),
            children: vec![lowering.lower(self.base), lowering.lower(modal)],
        }
    }
}

impl Styled for ModalOverlay {
    fn style(&mut self) -> &mut StyleRefinement {
        &mut self.style
    }
}

fn wire_cursor(cursor: CursorStyle) -> wire::mouse::Cursor {
    use wire::mouse::Cursor;
    match cursor {
        CursorStyle::Arrow => Cursor::Idle,
        CursorStyle::IBeam | CursorStyle::IBeamCursorForVerticalLayout => Cursor::Text,
        CursorStyle::Crosshair => Cursor::Crosshair,
        CursorStyle::ClosedHand => Cursor::Grabbing,
        CursorStyle::OpenHand => Cursor::Grab,
        CursorStyle::PointingHand => Cursor::Pointer,
        CursorStyle::ResizeLeft | CursorStyle::ResizeRight | CursorStyle::ResizeLeftRight => {
            Cursor::ResizingHorizontally
        }
        CursorStyle::ResizeUp | CursorStyle::ResizeDown | CursorStyle::ResizeUpDown => {
            Cursor::ResizingVertically
        }
        CursorStyle::ResizeUpLeftDownRight => Cursor::ResizingDiagonallyUp,
        CursorStyle::ResizeUpRightDownLeft => Cursor::ResizingDiagonallyDown,
        CursorStyle::ResizeColumn => Cursor::ResizingColumn,
        CursorStyle::ResizeRow => Cursor::ResizingRow,
        CursorStyle::OperationNotAllowed => Cursor::NotAllowed,
        CursorStyle::DragLink => Cursor::Alias,
        CursorStyle::DragCopy => Cursor::Copy,
        CursorStyle::ContextualMenu => Cursor::ContextMenu,
    }
}

impl gpui::prelude::FluentBuilder for Sensor {}
impl gpui::prelude::FluentBuilder for ResizeHandle {}
impl gpui::prelude::FluentBuilder for ModalOverlay {}

#[cfg(test)]
mod tests;
