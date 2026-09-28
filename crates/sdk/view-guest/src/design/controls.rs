//! The controls every view repeats: buttons, tabs, the segmented choice,
//! the switch and the row it sits in, and the pane divider.
use super::{size, space, text};
use crate::prelude::*;
use crate::{Div, FontWeight, Stateful};

/// What a [`Button`] is among its neighbours.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Kind {
    /// a surface fill
    #[default]
    Plain,
    /// the one action a screen leads with: the primary fill
    Primary,
    /// a choice among many: no fill, muted text
    Quiet,
    /// a secondary action beside the primary one: the window's ground in a
    /// hairline box
    Outline,
}

/// A button. Disabled keeps it visible, drops the click and says so.
/// Selected is the chosen one: fg text on the window and an fg edge. A
/// button told whether it is selected is a toggle, pressed or not; one
/// never told is a plain button.
#[derive(IntoElement)]
pub struct Button<F>
where
    F: Fn(&ClickEvent, &mut Window, &mut App) + 'static,
{
    id: ElementId,
    label: SharedString,
    theme: Theme,
    enabled: bool,
    kind: Kind,
    selected: Option<bool>,
    click: F,
}

pub fn button<F>(
    id: impl Into<ElementId>,
    label: impl Into<SharedString>,
    theme: &Theme,
    click: F,
) -> Button<F>
where
    F: Fn(&ClickEvent, &mut Window, &mut App) + 'static,
{
    Button {
        id: id.into(),
        label: label.into(),
        theme: *theme,
        enabled: true,
        kind: Kind::Plain,
        selected: None,
        click,
    }
}

impl<F> Button<F>
where
    F: Fn(&ClickEvent, &mut Window, &mut App) + 'static,
{
    pub fn enabled(mut self, enabled: bool) -> Self {
        self.enabled = enabled;
        self
    }
    pub fn kind(mut self, kind: Kind) -> Self {
        self.kind = kind;
        self
    }
    pub fn selected(mut self, selected: bool) -> Self {
        self.selected = Some(selected);
        self
    }
}

impl<F> RenderOnce for Button<F>
where
    F: Fn(&ClickEvent, &mut Window, &mut App) + 'static,
{
    fn render(self, _: &mut Window, _: &mut App) -> impl IntoElement {
        let theme = self.theme;
        let selected = self.selected == Some(true);
        let mut element = div()
            .id(self.id)
            .px_2()
            .py_1()
            .text_size(text::SECONDARY)
            .role(Role::Button)
            .child(self.label);
        // The chosen one is the ink one: fg text on the window, an fg edge
        // around it; the rest stay quiet.
        element = match (self.kind, selected) {
            // a primary that cannot run greys out but keeps its place
            (Kind::Primary, _) if !self.enabled => {
                element.bg(theme.faint).text_color(theme.primary_foreground)
            }
            (Kind::Primary, _) => element
                .bg(theme.primary)
                .text_color(theme.primary_foreground),
            (_, true) => element
                .bg(theme.background)
                .text_color(theme.foreground)
                .border_1()
                .border_color(theme.foreground),
            (Kind::Quiet, false) => element
                .text_color(theme.muted)
                .border_1()
                .border_color(crate::hsla(0., 0., 0., 0.)),
            (Kind::Plain, false) => element
                .bg(theme.surface)
                .text_color(theme.foreground)
                .border_1()
                .border_color(theme.surface),
            (Kind::Outline, false) => element
                .bg(theme.background)
                .text_color(theme.foreground)
                .border_1()
                .border_color(theme.border_strong),
        };
        if let Some(pressed) = self.selected {
            element = element.aria_toggled(pressed.into());
        }
        if selected {
            element = element.font_weight(FontWeight::MEDIUM);
        }
        if !self.enabled {
            return match self.kind {
                Kind::Primary => element.aria_disabled(true),
                _ => element.text_color(theme.muted).aria_disabled(true),
            };
        }
        element = match (self.kind, selected) {
            (Kind::Quiet, false) => element.hover(move |style| style.text_color(theme.foreground)),
            (Kind::Plain, false) => element
                .hover(move |style| style.bg(theme.surface_raised))
                .active(move |style| style.bg(theme.accent_soft)),
            (Kind::Outline, false) => element.hover(move |style| style.bg(theme.surface)),
            _ => element,
        };
        element.focusable().on_click(self.click)
    }
}

/// A control drawn as a glyph alone (a cross, a plus): muted until the
/// pointer is on it. `name` is what it does, in words, since the glyph
/// says nothing to a screen reader.
pub fn icon_button(
    id: impl Into<ElementId>,
    glyph: impl IntoElement,
    name: impl Into<SharedString>,
    theme: &Theme,
    click: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
) -> Stateful<Div> {
    let theme = *theme;
    div()
        .id(id)
        .px_1()
        .text_color(theme.muted)
        .cursor_pointer()
        .hover(move |style| style.text_color(theme.foreground))
        .role(Role::Button)
        .aria_label(name)
        .focusable()
        .on_click(click)
        .child(glyph)
}

/// A tab: quiet text, the chosen one fg and underlined, no fill. A caller
/// sizes it to its bar (`h_full`, `flex_1`) and may label a glyph.
pub fn tab(
    id: impl Into<ElementId>,
    label: impl Into<SharedString>,
    selected: bool,
    theme: &Theme,
    click: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
) -> Stateful<Div> {
    let theme = *theme;
    div()
        .id(id)
        .flex()
        .items_center()
        .px_2()
        .py_1()
        .text_size(text::SECONDARY)
        .text_color(if selected {
            theme.foreground
        } else {
            theme.muted
        })
        .border_b_2()
        .border_color(if selected {
            theme.foreground
        } else {
            theme.background
        })
        .when(selected, |tab| tab.font_weight(FontWeight::MEDIUM))
        .hover(move |style| style.text_color(theme.foreground))
        .role(Role::Tab)
        .aria_selected(selected)
        .focusable()
        .on_click(click)
        .child(label.into())
}

/// A few choices side by side in one box, the picked one ink-filled: a
/// state filter, an object format, an invite's lifetime. `label` names the
/// choice; the segments are [`segment`]s; the box draws the edge they share.
pub fn segmented(
    id: impl Into<ElementId>,
    label: impl Into<SharedString>,
    theme: &Theme,
    segments: impl IntoIterator<Item = Stateful<Div>>,
) -> Stateful<Div> {
    div()
        .id(id)
        .flex()
        .flex_none()
        .items_center()
        .border_t_1()
        .border_b_1()
        .border_r_1()
        .border_color(theme.border_strong)
        .role(Role::RadioGroup)
        .aria_label(label)
        .children(segments)
}

/// One choice of a [`segmented`] box.
pub fn segment(
    id: impl Into<ElementId>,
    label: impl Into<SharedString>,
    selected: bool,
    theme: &Theme,
    click: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
) -> Stateful<Div> {
    let theme = *theme;
    div()
        .id(id)
        .h(size::ROW)
        .px(space::MD)
        .flex()
        .items_center()
        .border_l_1()
        .border_color(theme.border_strong)
        .text_size(text::SECONDARY)
        .whitespace_nowrap()
        .map(|segment| match selected {
            true => segment
                .bg(theme.primary)
                .text_color(theme.primary_foreground),
            false => segment
                .text_color(theme.muted)
                .hover(move |style| style.text_color(theme.foreground)),
        })
        .role(Role::RadioButton)
        .aria_toggled(selected.into())
        .focusable()
        .on_click(click)
        .child(label.into())
}

/// An on/off switch: a pill with its knob at the on or the off end.
pub fn switch(
    id: impl Into<ElementId>,
    label: impl Into<SharedString>,
    on: bool,
    enabled: bool,
    theme: &Theme,
    toggle: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
) -> Stateful<Div> {
    let knob = div().size(px(12.)).rounded_full().bg(theme.background);
    let element = div()
        .id(id)
        .w(px(28.))
        .h(px(16.))
        .flex_none()
        .flex()
        .items_center()
        .px(px(2.))
        .rounded_full()
        .bg(if on {
            theme.primary
        } else {
            theme.border_strong
        })
        .when(on, |pill| pill.justify_end())
        .role(Role::Switch)
        .aria_label(label.into())
        .aria_toggled(on.into())
        .child(knob);
    match enabled {
        true => element.cursor_pointer().focusable().on_click(toggle),
        false => element.opacity(0.5).aria_disabled(true),
    }
}

/// One setting: its title and a line about it on the left, its control on
/// the right, a hairline under it.
pub fn setting_row(
    id: impl Into<ElementId>,
    title: impl Into<SharedString>,
    description: impl Into<SharedString>,
    control: impl IntoElement,
    theme: &Theme,
) -> Stateful<Div> {
    div()
        .id(id)
        .w_full()
        // wrapped, it grows; a column around it must not squeeze it
        .flex_none()
        .flex()
        .flex_wrap()
        .items_center()
        .gap(space::LG)
        .py(space::MD)
        .border_b_1()
        .border_color(theme.border)
        .child(
            div()
                .flex_1()
                .min_w(px(200.))
                .flex()
                .flex_col()
                .gap(space::HAIR)
                .child(
                    div()
                        .text_size(text::BODY)
                        .font_weight(FontWeight::MEDIUM)
                        .child(title.into()),
                )
                .child(
                    div()
                        .text_size(text::SECONDARY)
                        .text_color(theme.muted)
                        .child(description.into()),
                ),
        )
        .child(
            div()
                .flex_none()
                .flex()
                .items_center()
                .gap(space::SM)
                .child(control),
        )
}

/// How far an arrow key moves a [`divider`]; shift moves it four times as far.
const STEP: f32 = 8.;

/// The line between two panes, dragged to move it: `drag` takes the
/// horizontal delta (and clamps the layout it moves). `label` names it
/// ("Resize the room list"); focused, left and right move it by [`STEP`].
pub fn divider<V: crate::View>(
    id: impl Into<ElementId>,
    label: impl Into<SharedString>,
    theme: &Theme,
    cx: &mut crate::Context<V>,
    drag: impl Fn(&mut V, f32) + 'static,
) -> crate::ResizeHandle {
    let drag = std::rc::Rc::new(drag);
    let dragged = cx.listener({
        let drag = drag.clone();
        move |view, delta: &(Pixels, Pixels), _window, cx| {
            drag(view, delta.0.into());
            cx.notify();
        }
    });
    let stepped = cx.listener(move |view, event: &KeyDownEvent, _window, cx| {
        let step = match event.keystroke.modifiers.shift {
            true => STEP * 4.,
            false => STEP,
        };
        let delta = match event.keystroke.key.as_str() {
            "left" => -step,
            "right" => step,
            _ => return,
        };
        drag(view, delta);
        cx.notify();
    });
    crate::resize_handle(id, div().w(crate::px(1.)).h_full().bg(theme.border))
        .on_drag(dragged)
        .role(Role::Splitter)
        .aria_label(label)
        .aria_orientation(gpui::Orientation::Vertical)
        .focusable()
        .tab_stop(true)
        .on_key_down(stepped)
}

#[cfg(test)]
mod tests;
