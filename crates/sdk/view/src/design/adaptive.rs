//! One intent, two presentations (wasm-mobile `lanes/V/STRUCTURE.md` item
//! 3). Where a pointer and a finger differ in how a person acts, not only
//! in how a thing looks, the view says what it offers once and the
//! [`Ladder`] picks the presentation: the desktop's under a pointer, the
//! Expo kit's under a finger. Each pointer half is the code a view drew
//! before it moved here, so a view that adopts one draws the same desktop
//! frame.
//!
//! - an item's actions: [`opens_actions`] (the Aria custom action, either
//!   way) and [`tap_or_hold`] (a finger's long press);
//! - choices or a short form over the page: [`popup`], a frame hung from a
//!   point, or the kit's sheet;
//! - a yes/no before something final: [`confirm`], a small panel hung from
//!   a point, or the kit's `Dialog`;
//! - a many-line field's way to submit: [`send`], a text button, or the
//!   kit's send `Hit`.
use gpui::Refineable;

use super::{
    Kind, Ladder, TextType, button, focus_shown, icon, icon_button, sheet, size, space, text,
};
use crate::prelude::*;
use crate::{Anchored, AnyElement, Div, FontWeight, Hsla, Stateful, StyleRefinement};
use gpui::SharedString;

/// What a press runs.
type Press = Box<dyn Fn(&ClickEvent, &mut Window, &mut App)>;
/// What a dismissal runs.
type Dismiss = Box<dyn Fn(&(), &mut Window, &mut App)>;

/// The custom action an item's actions answer ([`opens_actions`]).
pub const ACTIONS: i32 = 1;

/// Gives `element` the Aria custom action `label` ("Message actions"),
/// which runs `open`: an assistive technology's way to what a pointer's
/// hover strip or a finger's long press offers (rule 5). The same under
/// either pointer: `design::opens_actions(row, "Message actions",
/// cx.listener(|view, _: &(), window, cx| ..))`.
pub fn opens_actions<E: StatefulInteractiveElement>(
    element: E,
    label: impl Into<String>,
    open: impl Fn(&(), &mut Window, &mut App) + 'static,
) -> E {
    element
        .custom_action(ACTIONS, label)
        .on_a11y_action(crate::wire::Action::CustomAction, move |_, window, app| {
            open(&(), window, app)
        })
}

/// A press that runs `tap`, or `hold` when a finger holds it: the Expo
/// app's long press, its way to an item's actions while a tap selects. A
/// pointer never holds, so under a pointer it is `tap` alone:
/// `.on_click(design::tap_or_hold(select, open))`.
pub fn tap_or_hold(
    tap: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
    hold: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
) -> impl Fn(&ClickEvent, &mut Window, &mut App) + 'static {
    move |event, window, app| match event {
        ClickEvent::Touch(touch) if touch.long_press => hold(event, window, app),
        _ => tap(event, window, app),
    }
}

/// Choices or a short form over `base` while `open` holds them. Under a
/// pointer a frame hangs from a point (the caller's [`Popup::anchored`],
/// gpui's own `anchored()` without its child) in a [`modal_overlay`]
/// named `label`: Tab stays in it and a press outside dismisses it. Under
/// a finger the kit's sheet named `label` (`{id}-sheet`) beside `base`:
/// Back, the scrim and a drag dismiss it. Closed, it is `base` either way.
/// What it holds is the caller's: the desktop frame (its edge, its shadow,
/// its size) under a pointer, the Expo body under a finger.
pub fn popup(
    id: impl Into<ElementId>,
    label: impl Into<String>,
    base: impl IntoElement,
    open: Option<impl IntoElement>,
    ladder: &Ladder,
    theme: &Theme,
) -> Popup {
    Popup {
        id: id.into(),
        label: label.into(),
        base: base.into_any_element(),
        open: open.map(IntoElement::into_any_element),
        ladder: *ladder,
        theme: *theme,
        anchored: crate::anchored(),
        deferred: false,
        style: StyleRefinement::default(),
        on_dismiss: None,
    }
}

/// A [`popup`] being built.
pub struct Popup {
    id: ElementId,
    label: String,
    base: AnyElement,
    open: Option<AnyElement>,
    ladder: Ladder,
    theme: Theme,
    anchored: Anchored,
    deferred: bool,
    style: StyleRefinement,
    on_dismiss: Option<Dismiss>,
}

impl Popup {
    /// Where a pointer's frame hangs: `anchored().position(at)
    /// .snap_to_window_with_margin(..)`, its child left to the popup.
    pub fn anchored(mut self, anchored: Anchored) -> Self {
        self.anchored = anchored;
        self
    }
    /// A pointer's frame is drawn after everything else (gpui's
    /// `deferred`): one hung from a control in the page's flow.
    pub fn deferred(mut self) -> Self {
        self.deferred = true;
        self
    }
    /// Runs when the person dismisses it: a press outside, Escape, the
    /// scrim, a drag down, Back.
    pub fn on_dismiss(mut self, listener: impl Fn(&(), &mut Window, &mut App) + 'static) -> Self {
        self.on_dismiss = Some(Box::new(listener));
        self
    }
}

/// The open overlay's style under a pointer (its backdrop, its alignment).
impl Styled for Popup {
    fn style(&mut self) -> &mut StyleRefinement {
        &mut self.style
    }
}

impl IntoElement for Popup {
    type Element = AnyElement;
    fn into_element(self) -> AnyElement {
        let Self {
            id,
            label,
            base,
            open,
            ladder,
            theme,
            anchored,
            deferred,
            style,
            on_dismiss,
        } = self;
        if ladder.touch {
            let sheet_id = ElementId::Name(format!("{id}-sheet").into());
            let open = open.map(|held| {
                let panel = sheet(sheet_id, label, &ladder, &theme);
                match on_dismiss {
                    Some(dismiss) => panel.on_dismiss(dismiss),
                    None => panel,
                }
                .child(held)
            });
            return div()
                .id(id)
                .size_full()
                .child(base)
                .children(open)
                .into_any_element();
        }
        let hung = open.map(|held| {
            let hung = anchored.child(held);
            match deferred {
                true => crate::deferred(hung).into_any_element(),
                false => hung.into_any_element(),
            }
        });
        let mut overlay = modal_overlay(id, label, base, hung);
        overlay.style().refine(&style);
        match on_dismiss {
            Some(dismiss) => overlay.on_dismiss(dismiss),
            None => overlay,
        }
        .into_any_element()
    }
}

/// What a [`confirm`] asks: its `title` ("Delete this message?"), what
/// follows in `body`, Cancel and the final `verb`. `id` names its frame,
/// an alert dialog.
pub fn ask(
    id: impl Into<ElementId>,
    title: impl Into<SharedString>,
    body: impl Into<SharedString>,
) -> Ask {
    Ask {
        id: id.into(),
        title: title.into(),
        body: body.into(),
        cancel: None,
        verb: None,
    }
}

/// An [`ask`] being built.
pub struct Ask {
    id: ElementId,
    title: SharedString,
    body: SharedString,
    cancel: Option<(ElementId, Press)>,
    verb: Option<(ElementId, SharedString, Press)>,
}

impl Ask {
    /// Cancel, the first button: a stray Enter on open keeps things as
    /// they are.
    pub fn cancel(
        mut self,
        id: impl Into<ElementId>,
        press: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
    ) -> Self {
        self.cancel = Some((id.into(), Box::new(press)));
        self
    }
    /// The final verb ("Delete"), in the danger tone.
    pub fn verb(
        mut self,
        id: impl Into<ElementId>,
        words: impl Into<SharedString>,
        press: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
    ) -> Self {
        self.verb = Some((id.into(), words.into(), Box::new(press)));
        self
    }
}

/// A yes/no over `base` before something final, while `open` asks it.
/// Under a pointer a small panel hangs from a point (the caller's
/// [`Confirm::anchored`]) over a dim: the title in body text, the body
/// secondary and muted, Cancel and the verb as text buttons. Under a
/// finger the Expo kit's `Dialog`: centred, 24 in, a `border_strong` edge,
/// the title 17 medium, the body small and muted, a quiet Cancel and the
/// verb on a danger edge, over the scrim. The overlay is named `label`;
/// a press outside dismisses it. Closed, it is `base` either way.
pub fn confirm(
    id: impl Into<ElementId>,
    label: impl Into<String>,
    base: impl IntoElement,
    open: Option<Ask>,
    ladder: &Ladder,
    theme: &Theme,
) -> Confirm {
    Confirm {
        id: id.into(),
        label: label.into(),
        base: base.into_any_element(),
        open,
        ladder: *ladder,
        theme: *theme,
        anchored: crate::anchored(),
        on_dismiss: None,
    }
}

/// A [`confirm`] being built.
pub struct Confirm {
    id: ElementId,
    label: String,
    base: AnyElement,
    open: Option<Ask>,
    ladder: Ladder,
    theme: Theme,
    anchored: Anchored,
    on_dismiss: Option<Dismiss>,
}

impl Confirm {
    /// Where a pointer's panel hangs (gpui's `anchored()`, its child left
    /// to the confirm). A finger's dialog is centred.
    pub fn anchored(mut self, anchored: Anchored) -> Self {
        self.anchored = anchored;
        self
    }
    /// Runs when the person dismisses it without answering.
    pub fn on_dismiss(mut self, listener: impl Fn(&(), &mut Window, &mut App) + 'static) -> Self {
        self.on_dismiss = Some(Box::new(listener));
        self
    }
}

impl IntoElement for Confirm {
    type Element = AnyElement;
    fn into_element(self) -> AnyElement {
        let Self {
            id,
            label,
            base,
            open,
            ladder,
            theme,
            anchored,
            on_dismiss,
        } = self;
        let asking = open.is_some();
        let modal = open.map(|ask| match ladder.touch {
            true => dialog(ask, &ladder, &theme),
            false => anchored.child(panel(ask, &theme)).into_any_element(),
        });
        let overlay = modal_overlay(id, label, base, modal);
        let overlay = match on_dismiss {
            Some(dismiss) => overlay.on_dismiss(dismiss),
            None => overlay,
        };
        match (asking, ladder.touch) {
            (true, true) => overlay
                .flex()
                .items_center()
                .justify_center()
                .px(space::XL)
                .backdrop(match theme.dark {
                    true => crate::hsla(0., 0., 0., 0.55),
                    false => crate::hsla(0., 0., 0., 0.4),
                })
                .into_any_element(),
            (true, false) => overlay
                .backdrop(crate::hsla(0., 0., 0., 0.35))
                .into_any_element(),
            (false, _) => overlay.into_any_element(),
        }
    }
}

/// A pointer's confirmation: the frame chat drew around its delete
/// question, and the question inside it.
fn panel(ask: Ask, theme: &Theme) -> Stateful<Div> {
    let Ask {
        id,
        title,
        body,
        cancel,
        verb,
    } = ask;
    let frame = div()
        .id(id)
        .role(Role::AlertDialog)
        .aria_label(title.clone());
    let buttons = div()
        .flex()
        .justify_end()
        .gap_2()
        .children(cancel.map(|(id, press)| {
            text_button(
                id,
                "Cancel".into(),
                press,
                theme,
                (theme.foreground, theme.surface),
            )
        }))
        .children(verb.map(|(id, words, press)| {
            text_button(id, words, press, theme, (theme.danger, theme.danger_soft))
        }));
    focus_shown(frame, theme, |style| style.shadow_lg())
        .border_1()
        .border_color(theme.border)
        .bg(theme.background)
        .shadow_lg()
        .occlude()
        .child(
            div()
                .flex()
                .flex_col()
                .gap_1()
                .p_3()
                .child(div().text_size(text::BODY).child(title))
                .child(
                    div()
                        .text_size(text::SECONDARY)
                        .text_color(theme.muted)
                        .child(body),
                )
                .child(buttons),
        )
}

/// A pointer's text button in a confirmation: a control's height, its
/// words in `fg` on `bg`, a raised ground under the pointer.
fn text_button(
    id: ElementId,
    label: SharedString,
    press: Press,
    theme: &Theme,
    (fg, bg): (Hsla, Hsla),
) -> Stateful<Div> {
    let theme = *theme;
    div()
        .id(id)
        .w_full()
        .h(size::CONTROL)
        .px_2()
        .flex()
        .items_center()
        .gap_2()
        .bg(bg)
        .text_color(fg)
        .role(Role::Button)
        .aria_label(label.clone())
        .aria_disabled(false)
        .child(div().whitespace_nowrap().child(label))
        .focusable()
        .hover(move |style| style.bg(theme.surface_raised))
        .active(move |style| style.bg(theme.accent_soft))
        .on_click(press)
}

/// A finger's confirmation: the Expo kit's `Dialog`.
fn dialog(ask: Ask, ladder: &Ladder, theme: &Theme) -> AnyElement {
    let Ask {
        id,
        title,
        body,
        cancel,
        verb,
    } = ask;
    let cancel =
        cancel.map(|(id, press)| button(id, "Cancel", ladder, theme, press).kind(Kind::Quiet));
    // the kit's danger button: its words and edge in the danger tone
    let verb = verb.map(|(id, words, press)| {
        div()
            .id(id)
            .h(ladder.control)
            .px(space::BLOCK)
            .flex()
            .items_center()
            .border_1()
            .border_color(theme.danger)
            .text_color(theme.danger)
            .text_type(ladder.label)
            .font_weight(FontWeight::MEDIUM)
            .role(Role::Button)
            .focusable()
            .active(|style| style.opacity(0.6))
            .on_click(press)
            .child(words)
    });
    div()
        .id(id)
        .role(Role::AlertDialog)
        .aria_label(title.clone())
        .w_full()
        .border_1()
        .border_color(theme.border_strong)
        .bg(theme.background)
        .occlude()
        .child(
            div()
                .flex()
                .flex_col()
                .pt(px(20.))
                .px(px(20.))
                .pb(space::LG)
                .child(
                    div()
                        .mb(space::SM)
                        .text_type(ladder.title)
                        .font_weight(FontWeight::MEDIUM)
                        .child(title),
                )
                .child(
                    div()
                        .text_type(ladder.small)
                        .text_color(theme.muted)
                        .child(body),
                )
                .child(
                    div()
                        .flex()
                        .justify_end()
                        .gap(space::XXS)
                        .mt(space::BLOCK)
                        .children(cancel)
                        .children(verb),
                ),
        )
        .into_any_element()
}

/// The control that submits a many-line field (a composer, a reply), named
/// `name` ("Send", "Save edit") and live while `sendable`. Under a pointer
/// an outline text button beside the field; under a finger the Expo kit's
/// `Hit` with the Lucide `glyph` (`send-horizontal`, an edit's `check`),
/// the ink square once there is something to send, faint until then. A
/// finger's Return is a newline in a many-line field (rule 6), so this is
/// its one way to send.
pub fn send(
    id: impl Into<ElementId>,
    name: impl Into<SharedString>,
    glyph: &str,
    sendable: bool,
    ladder: &Ladder,
    theme: &Theme,
    click: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
) -> AnyElement {
    match ladder.touch {
        true => icon_button(id, icon(glyph, ladder.icon), name, ladder, theme, click)
            .filled(sendable)
            .enabled(sendable)
            .into_any_element(),
        false => button(id, name, ladder, theme, click)
            .kind(Kind::Outline)
            .enabled(sendable)
            .into_any_element(),
    }
}
