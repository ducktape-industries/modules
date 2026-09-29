use super::*;

impl Interactivity {
    pub(crate) fn into_wire(
        self,
        lowering: &Lowering<'_>,
    ) -> (Option<wire::ElementIdWire>, wire::Interactivity) {
        let scope = lowering.current_path();
        // an identified element lowers inside its own scope; its siblings
        // share the one above
        let scope = &scope[..scope.len() - usize::from(self.id.is_some())];
        let path = |target| [scope, &[crate::element::wire_id(target)]].concat();
        let mut aria = self.aria;
        aria.labelled_by = self.labelled_by.into_iter().map(path).collect();
        aria.described_by = self.described_by.into_iter().map(path).collect();
        aria.controls = self.controls.into_iter().map(path).collect();
        aria.error_message = self.error_message.map(path);
        let offered: std::rc::Rc<[i32]> = aria.custom_actions.iter().map(|(id, _)| *id).collect();
        aria.actions = self
            .a11y_actions
            .into_iter()
            .map(|(action, listener)| {
                // gpui's listener is FnMut; a route is called through `&`
                let listener = std::cell::RefCell::new(listener);
                let offered = offered.clone();
                let route = lowering.route(
                    move |data: &Option<wire::ActionData>, window: &mut Window, app: &mut App| {
                        // a custom action the node does not offer is not its to answer
                        if let Some(wire::ActionData::CustomAction(id)) = data
                            && !offered.contains(id)
                        {
                            return;
                        }
                        (listener.borrow_mut())(data.as_ref(), window, app)
                    },
                );
                (action, route)
            })
            .collect();
        let id = self.id.map(crate::element::wire_id);
        let tooltip = self.tooltip.map(|tooltip| {
            let request = lowering.tooltip(tooltip.build);
            wire::Tooltip {
                request,
                content: None,
                hoverable: tooltip.hoverable,
                delay_ms: self
                    .tooltip_show_delay
                    .unwrap_or(Duration::from_millis(500))
                    .as_millis()
                    .min(u64::MAX as u128) as u64,
            }
        });
        let wire = wire::Interactivity {
            role: self.role,
            aria,
            focusable: self.focusable,
            // a focusable node is a Tab stop unless it said `tab_stop(false)`
            tab_stop: self.tab_stop.or(self.focusable.then_some(true)),
            tab_index: self.tab_index,
            tab_group: self.tab_group,
            focus: self.focus,
            in_focus: self.in_focus,
            // a focusable node shows the focus ring unless it draws its own
            focus_visible: self.focus_visible.or_else(|| {
                self.focusable
                    .then(|| crate::design::focus_ring(lowering.theme().accent))
            }),
            key_context: self.key_context,
            focus_handle: self.focus_handle.map(|handle| handle.id),
            occlude: self.occlude,
            block_mouse_except_scroll: self.block_mouse_except_scroll,
            hover_listener_mode: match self.hover_listener_mode {
                gpui::HoverListenerMode::InputModalityAware => {
                    wire::HoverListenerMode::InputModalityAware
                }
                gpui::HoverListenerMode::InputModalityIndependent => {
                    wire::HoverListenerMode::InputModalityIndependent
                }
            },
            group: self.group,
            hover: self.hover,
            active: self.active,
            group_hover: self
                .group_hover
                .map(|(group, style)| wire::GroupRefinement { group, style }),
            group_active: self
                .group_active
                .map(|(group, style)| wire::GroupRefinement { group, style }),
            on_click: self.on_click.map(|listener| lowering.click(listener)),
            on_aux_click: self.on_aux_click.map(|listener| lowering.click(listener)),
            on_mouse_down: route_buttons(self.mouse_down, lowering, |e: &gpui::MouseDownEvent| {
                e.button
            }),
            capture_mouse_down: route_plain(self.capture_mouse_down, lowering),
            on_mouse_down_out: route_plain(self.mouse_down_out, lowering),
            on_mouse_up: route_buttons(self.mouse_up, lowering, |e: &gpui::MouseUpEvent| e.button),
            capture_mouse_up: route_plain(self.capture_mouse_up, lowering),
            on_mouse_up_out: route_buttons(
                self.mouse_up_out,
                lowering,
                |e: &gpui::MouseUpEvent| e.button,
            ),
            on_mouse_pressure: route_plain(self.mouse_pressure, lowering),
            capture_mouse_pressure: route_plain(self.capture_mouse_pressure, lowering),
            on_mouse_move: route_plain(self.mouse_move, lowering),
            on_mouse_exit: route_plain(self.mouse_exit, lowering),
            on_scroll_wheel: route_plain(self.scroll_wheel, lowering),
            on_pinch: route_plain(self.pinch, lowering),
            capture_pinch: route_plain(self.capture_pinch, lowering),
            on_key_down: route_plain(self.key_down, lowering),
            capture_key_down: route_plain(self.capture_key_down, lowering),
            on_key_up: route_plain(self.key_up, lowering),
            capture_key_up: route_plain(self.capture_key_up, lowering),
            on_modifiers_changed: route_plain(self.modifiers_changed, lowering),
            on_hover: self.on_hover.map(|listener| lowering.route(listener)),
            on_file_drop_exit: route_plain(self.on_file_drop_exit, lowering),
            tooltip,
        };
        (id, wire)
    }
}

fn route_plain<E: 'static>(
    listeners: Vec<EventListener<E>>,
    lowering: &Lowering<'_>,
) -> Option<u32> {
    (!listeners.is_empty()).then(|| {
        lowering.route(move |event: &E, window, app| {
            for listener in &listeners {
                listener(event, window, app);
            }
        })
    })
}

/// One route for a list of button listeners: each runs for its button, or
/// for any button when it names none.
fn route_buttons<E: 'static>(
    listeners: Vec<ButtonBinding<E>>,
    lowering: &Lowering<'_>,
    button: fn(&E) -> MouseButton,
) -> Option<u32> {
    (!listeners.is_empty()).then(|| {
        lowering.route(move |event: &E, window, app| {
            for binding in &listeners {
                if binding.button.is_none_or(|wanted| wanted == button(event)) {
                    (binding.listener)(event, window, app);
                }
            }
        })
    })
}
