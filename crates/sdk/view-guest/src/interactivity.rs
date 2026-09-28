//! GPUI-shaped interaction recipes lowered into driver-owned frame routes.

use crate::{App, Div, Lowering, Window, slots, wire};
use gpui::{ClickEvent, ElementId, FileDropEvent, MouseButton, SharedString, StyleRefinement};
use std::time::Duration;

mod bindings;
use bindings::ButtonBinding;
pub(crate) use bindings::EventListener;

mod focus;
pub use focus::FocusHandle;

struct TooltipBuilder {
    build: slots::TooltipBuilder,
    hoverable: bool,
}

/// The explicit state carried by guest interactivity until frame lowering.
#[derive(Default)]
pub struct Interactivity {
    pub(crate) role: Option<gpui::Role>,
    pub(crate) aria: wire::Aria,
    pub(crate) focusable: bool,
    pub(crate) tab_stop: Option<bool>,
    pub(crate) tab_index: Option<i32>,
    pub(crate) tab_group: bool,
    pub(crate) focus: Option<StyleRefinement>,
    pub(crate) in_focus: Option<StyleRefinement>,
    pub(crate) focus_visible: Option<StyleRefinement>,
    pub(crate) key_context: Option<wire::KeyContext>,
    pub(crate) focus_handle: Option<FocusHandle>,
    pub(crate) group: Option<SharedString>,
    pub(crate) hover: Option<StyleRefinement>,
    pub(crate) active: Option<StyleRefinement>,
    pub(crate) group_hover: Option<(SharedString, StyleRefinement)>,
    pub(crate) group_active: Option<(SharedString, StyleRefinement)>,
    pub(crate) on_click: Option<EventListener<ClickEvent>>,
    pub(crate) on_aux_click: Option<EventListener<ClickEvent>>,
    mouse_down: Vec<ButtonBinding<gpui::MouseDownEvent>>,
    capture_mouse_down: Vec<EventListener<gpui::MouseDownEvent>>,
    mouse_down_out: Vec<EventListener<gpui::MouseDownEvent>>,
    mouse_up: Vec<ButtonBinding<gpui::MouseUpEvent>>,
    capture_mouse_up: Vec<EventListener<gpui::MouseUpEvent>>,
    mouse_up_out: Vec<ButtonBinding<gpui::MouseUpEvent>>,
    mouse_pressure: Vec<EventListener<gpui::MousePressureEvent>>,
    capture_mouse_pressure: Vec<EventListener<gpui::MousePressureEvent>>,
    mouse_move: Vec<EventListener<gpui::MouseMoveEvent>>,
    mouse_exit: Vec<EventListener<gpui::MouseExitEvent>>,
    scroll_wheel: Vec<EventListener<gpui::ScrollWheelEvent>>,
    pinch: Vec<EventListener<gpui::PinchEvent>>,
    capture_pinch: Vec<EventListener<gpui::PinchEvent>>,
    key_down: Vec<EventListener<gpui::KeyDownEvent>>,
    capture_key_down: Vec<EventListener<gpui::KeyDownEvent>>,
    key_up: Vec<EventListener<gpui::KeyUpEvent>>,
    capture_key_up: Vec<EventListener<gpui::KeyUpEvent>>,
    modifiers_changed: Vec<EventListener<gpui::ModifiersChangedEvent>>,
    on_hover: Option<EventListener<bool>>,
    hover_listener_mode: gpui::HoverListenerMode,
    on_file_drop_exit: Vec<EventListener<FileDropEvent>>,
    tooltip: Option<TooltipBuilder>,
    tooltip_show_delay: Option<Duration>,
    occlude: bool,
    block_mouse_except_scroll: bool,
    pub(crate) base_style: StyleRefinement,
    pub(crate) id: Option<ElementId>,
}

/// Add GPUI's stateless and stateful interactivity declarations to an element.
pub trait InteractiveElement: Sized {
    fn interactivity(&mut self) -> &mut Interactivity;

    fn id(mut self, id: impl Into<ElementId>) -> Stateful<Self> {
        self.interactivity().id = Some(id.into());
        Stateful { element: self }
    }

    fn track_focus(mut self, focus_handle: &FocusHandle) -> Self {
        self.interactivity().focusable = true;
        self.interactivity().focus_handle = Some(focus_handle.clone());
        self
    }

    fn tab_stop(mut self, tab_stop: bool) -> Self {
        self.interactivity().tab_stop = Some(tab_stop);
        self
    }

    fn tab_index(mut self, index: isize) -> Self {
        self.interactivity().focusable = true;
        self.interactivity().tab_index = i32::try_from(index).ok();
        self.interactivity().tab_stop = Some(true);
        self
    }

    fn tab_group(mut self) -> Self {
        self.interactivity().tab_group = true;
        if self.interactivity().tab_index.is_none() {
            self.interactivity().tab_index = Some(0);
        }
        self
    }

    fn key_context<C, E>(mut self, key_context: C) -> Self
    where
        C: TryInto<gpui::KeyContext, Error = E>,
        E: std::fmt::Display,
    {
        if let Ok(key_context) = key_context.try_into() {
            self.interactivity().key_context = Some(wire::KeyContext::from_gpui(&key_context));
        }
        self
    }

    fn group(mut self, group: impl Into<SharedString>) -> Self {
        self.interactivity().group = Some(group.into());
        self
    }

    fn hover(mut self, f: impl FnOnce(StyleRefinement) -> StyleRefinement) -> Self {
        self.interactivity().hover = Some(f(StyleRefinement::default()));
        self
    }

    fn group_hover(
        mut self,
        group: impl Into<SharedString>,
        f: impl FnOnce(StyleRefinement) -> StyleRefinement,
    ) -> Self {
        self.interactivity().group_hover = Some((group.into(), f(StyleRefinement::default())));
        self
    }

    fn on_mouse_down(
        mut self,
        button: MouseButton,
        listener: impl Fn(&gpui::MouseDownEvent, &mut Window, &mut App) + 'static,
    ) -> Self {
        self.interactivity().mouse_down.push(ButtonBinding {
            button: Some(button),
            listener: Box::new(listener),
        });
        self
    }

    fn capture_any_mouse_down(
        mut self,
        listener: impl Fn(&gpui::MouseDownEvent, &mut Window, &mut App) + 'static,
    ) -> Self {
        self.interactivity()
            .capture_mouse_down
            .push(Box::new(listener));
        self
    }

    fn on_any_mouse_down(
        mut self,
        listener: impl Fn(&gpui::MouseDownEvent, &mut Window, &mut App) + 'static,
    ) -> Self {
        self.interactivity().mouse_down.push(ButtonBinding {
            button: None,
            listener: Box::new(listener),
        });
        self
    }

    fn on_mouse_down_out(
        mut self,
        listener: impl Fn(&gpui::MouseDownEvent, &mut Window, &mut App) + 'static,
    ) -> Self {
        self.interactivity().mouse_down_out.push(Box::new(listener));
        self
    }

    fn on_mouse_up(
        mut self,
        button: MouseButton,
        listener: impl Fn(&gpui::MouseUpEvent, &mut Window, &mut App) + 'static,
    ) -> Self {
        self.interactivity().mouse_up.push(ButtonBinding {
            button: Some(button),
            listener: Box::new(listener),
        });
        self
    }

    fn capture_any_mouse_up(
        mut self,
        listener: impl Fn(&gpui::MouseUpEvent, &mut Window, &mut App) + 'static,
    ) -> Self {
        self.interactivity()
            .capture_mouse_up
            .push(Box::new(listener));
        self
    }

    fn on_any_mouse_up(
        mut self,
        listener: impl Fn(&gpui::MouseUpEvent, &mut Window, &mut App) + 'static,
    ) -> Self {
        self.interactivity().mouse_up.push(ButtonBinding {
            button: None,
            listener: Box::new(listener),
        });
        self
    }

    fn on_mouse_up_out(
        mut self,
        button: MouseButton,
        listener: impl Fn(&gpui::MouseUpEvent, &mut Window, &mut App) + 'static,
    ) -> Self {
        self.interactivity().mouse_up_out.push(ButtonBinding {
            button: Some(button),
            listener: Box::new(listener),
        });
        self
    }

    fn on_mouse_pressure(
        mut self,
        listener: impl Fn(&gpui::MousePressureEvent, &mut Window, &mut App) + 'static,
    ) -> Self {
        self.interactivity().mouse_pressure.push(Box::new(listener));
        self
    }

    fn capture_mouse_pressure(
        mut self,
        listener: impl Fn(&gpui::MousePressureEvent, &mut Window, &mut App) + 'static,
    ) -> Self {
        self.interactivity()
            .capture_mouse_pressure
            .push(Box::new(listener));
        self
    }

    fn on_mouse_move(
        mut self,
        listener: impl Fn(&gpui::MouseMoveEvent, &mut Window, &mut App) + 'static,
    ) -> Self {
        self.interactivity().mouse_move.push(Box::new(listener));
        self
    }

    fn on_mouse_exit(
        mut self,
        listener: impl Fn(&gpui::MouseExitEvent, &mut Window, &mut App) + 'static,
    ) -> Self {
        self.interactivity().mouse_exit.push(Box::new(listener));
        self
    }

    fn on_scroll_wheel(
        mut self,
        listener: impl Fn(&gpui::ScrollWheelEvent, &mut Window, &mut App) + 'static,
    ) -> Self {
        self.interactivity().scroll_wheel.push(Box::new(listener));
        self
    }

    fn on_pinch(
        mut self,
        listener: impl Fn(&gpui::PinchEvent, &mut Window, &mut App) + 'static,
    ) -> Self {
        self.interactivity().pinch.push(Box::new(listener));
        self
    }

    fn capture_pinch(
        mut self,
        listener: impl Fn(&gpui::PinchEvent, &mut Window, &mut App) + 'static,
    ) -> Self {
        self.interactivity().capture_pinch.push(Box::new(listener));
        self
    }

    fn on_key_down(
        mut self,
        listener: impl Fn(&gpui::KeyDownEvent, &mut Window, &mut App) + 'static,
    ) -> Self {
        self.interactivity().key_down.push(Box::new(listener));
        self
    }

    fn capture_key_down(
        mut self,
        listener: impl Fn(&gpui::KeyDownEvent, &mut Window, &mut App) + 'static,
    ) -> Self {
        self.interactivity()
            .capture_key_down
            .push(Box::new(listener));
        self
    }

    fn on_key_up(
        mut self,
        listener: impl Fn(&gpui::KeyUpEvent, &mut Window, &mut App) + 'static,
    ) -> Self {
        self.interactivity().key_up.push(Box::new(listener));
        self
    }

    fn capture_key_up(
        mut self,
        listener: impl Fn(&gpui::KeyUpEvent, &mut Window, &mut App) + 'static,
    ) -> Self {
        self.interactivity().capture_key_up.push(Box::new(listener));
        self
    }

    fn on_modifiers_changed(
        mut self,
        listener: impl Fn(&gpui::ModifiersChangedEvent, &mut Window, &mut App) + 'static,
    ) -> Self {
        self.interactivity()
            .modifiers_changed
            .push(Box::new(listener));
        self
    }

    fn on_file_drop_exit(
        mut self,
        listener: impl Fn(&FileDropEvent, &mut Window, &mut App) + 'static,
    ) -> Self {
        self.interactivity()
            .on_file_drop_exit
            .push(Box::new(listener));
        self
    }

    fn occlude(mut self) -> Self {
        self.interactivity().occlude = true;
        self
    }

    fn block_mouse_except_scroll(mut self) -> Self {
        self.interactivity().block_mouse_except_scroll = true;
        self
    }

    fn focus(mut self, f: impl FnOnce(StyleRefinement) -> StyleRefinement) -> Self {
        self.interactivity().focus = Some(f(StyleRefinement::default()));
        self
    }

    fn in_focus(mut self, f: impl FnOnce(StyleRefinement) -> StyleRefinement) -> Self {
        self.interactivity().in_focus = Some(f(StyleRefinement::default()));
        self
    }

    fn focus_visible(mut self, f: impl FnOnce(StyleRefinement) -> StyleRefinement) -> Self {
        self.interactivity().focus_visible = Some(f(StyleRefinement::default()));
        self
    }
}

impl InteractiveElement for Div {
    fn interactivity(&mut self) -> &mut Interactivity {
        &mut self.interactivity
    }
}

mod lowering;

mod stateful;
pub use stateful::{Stateful, StatefulInteractiveElement};
