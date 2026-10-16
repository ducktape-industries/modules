//! The adaptive components (`design::popup`, `confirm`, `send`,
//! `opens_actions`, `tap_or_hold`): one intent, the desktop's presentation
//! under a pointer, node for node the code a view drew before, and the
//! Expo kit's under a finger.
use serde::{Deserialize, Serialize};

use crate::design::{self, POINTER, TOUCH};
use crate::prelude::*;
use crate::testing::{self, TestAppContext};
use crate::{Anchor, AnchoredPositionMode, Div, Edges, FontWeight, View, wire};

fn lowered(element: impl IntoElement) -> (wire::Node, wire::Styles) {
    testing::lower(|_: &mut Context<()>| element)
}

fn at() -> crate::Anchored {
    anchored()
        .position(gpui::point(px(40.), px(60.)))
        .position_mode(AnchoredPositionMode::Window)
        .snap_to_window_with_margin(Edges::all(design::space::SM))
        .anchor(Anchor::TopRight)
        .offset(gpui::point(px(0.), px(30.)))
}

/// Under a pointer a popup is the overlay a view hung its menu in before
/// (chat's message menu, forge's dropdowns): the frame in `anchored()`, in
/// a `modal_overlay`. Closed, it is the screen alone, either way.
#[test]
fn under_a_pointer_a_popup_is_the_frame_the_desktop_hangs() {
    let theme = Theme::light();
    let frame = || div().id("frame").w(px(220.)).border_1().child("Copy text");
    let built = lowered(
        design::popup(
            "overlay",
            "Message menu",
            div().id("screen"),
            Some(frame()),
            &POINTER,
            &theme,
        )
        .anchored(at())
        .on_dismiss(|_, _, _| {}),
    );
    let today = lowered(
        modal_overlay(
            "overlay",
            "Message menu",
            div().id("screen"),
            Some(at().child(frame())),
        )
        .on_dismiss(|_, _, _| {}),
    );
    assert_eq!(built, today);
    let closed = |ladder| {
        lowered(design::popup(
            "overlay",
            "Message menu",
            div().id("screen"),
            None::<Div>,
            ladder,
            &theme,
        ))
    };
    assert_eq!(closed(&POINTER), closed(&TOUCH));
}

/// Under a finger the same popup is the kit's sheet beside the screen,
/// named as the dialog was.
#[test]
fn under_touch_a_popup_is_the_kits_sheet() {
    let theme = Theme::light();
    let (node, _) = lowered(
        design::popup(
            "overlay",
            "Message actions",
            div().id("screen"),
            Some(div().id("rows").child("Copy text")),
            &TOUCH,
            &theme,
        )
        .anchored(at())
        .on_dismiss(|_, _, _| {}),
    );
    let children = node.children();
    assert_eq!(children[0].key(), Some("screen"));
    assert!(
        matches!(&children[1], wire::Node::Sheet { label, on_dismiss: Some(_), .. }
            if label.as_deref() == Some("Message actions")),
        "{:?}",
        children[1]
    );
    assert_eq!(children[1].key(), Some("overlay-sheet"));
}

/// The desktop's delete question as chat drew it (`ui/menu.rs` `floating`
/// and `delete`, its `Item` buttons), to hold `design::confirm` to it.
fn chat_delete_today(theme: &Theme) -> impl IntoElement {
    let button = |id: &'static str, label: &'static str, fg, bg| {
        let theme = *theme;
        div()
            .id(id)
            .w_full()
            .h(design::size::CONTROL)
            .px_2()
            .flex()
            .items_center()
            .gap_2()
            .bg(bg)
            .text_color(fg)
            .role(Role::Button)
            .aria_label(label)
            .aria_disabled(false)
            .child(div().whitespace_nowrap().child(label))
            .focusable()
            .hover(move |s| s.bg(theme.surface_raised))
            .active(move |s| s.bg(theme.accent_soft))
            .on_click(|_, _, _| {})
    };
    let content = div()
        .flex()
        .flex_col()
        .gap_1()
        .p_3()
        .child(
            div()
                .text_size(design::text::BODY)
                .child("Delete this message?"),
        )
        .child(
            div()
                .text_size(design::text::SECONDARY)
                .text_color(theme.muted)
                .child("It leaves the room for everyone."),
        )
        .child(
            div()
                .flex()
                .justify_end()
                .gap_2()
                .child(button("cancel", "Cancel", theme.foreground, theme.surface))
                .child(button("delete", "Delete", theme.danger, theme.danger_soft)),
        );
    let frame = div()
        .id("delete-frame")
        .role(Role::AlertDialog)
        .aria_label("Delete this message?");
    let frame = design::focus_shown(frame, theme, |style| style.shadow_lg())
        .border_1()
        .border_color(theme.border)
        .bg(theme.background)
        .shadow_lg()
        .occlude()
        .child(content);
    modal_overlay(
        "overlay",
        "Message menu",
        div().id("screen"),
        Some(at().child(frame)),
    )
    .on_dismiss(|_, _, _| {})
    .backdrop(hsla(0., 0., 0., 0.35))
}

fn delete_ask() -> design::Ask {
    design::ask(
        "delete-frame",
        "Delete this message?",
        "It leaves the room for everyone.",
    )
    .cancel("cancel", |_, _, _| {})
    .verb("delete", "Delete", |_, _, _| {})
}

/// Under a pointer a confirm is chat's desktop delete question, node for
/// node; closed, it is the screen alone, either way.
#[test]
fn under_a_pointer_a_confirm_is_the_desktops_panel() {
    let theme = Theme::light();
    let built = lowered(
        design::confirm(
            "overlay",
            "Message menu",
            div().id("screen"),
            Some(delete_ask()),
            &POINTER,
            &theme,
        )
        .anchored(at())
        .on_dismiss(|_, _, _| {}),
    );
    assert_eq!(built, lowered(chat_delete_today(&theme)));
    let closed = |ladder| {
        lowered(design::confirm(
            "overlay",
            "Message menu",
            div().id("screen"),
            None,
            ladder,
            &theme,
        ))
    };
    assert_eq!(closed(&POINTER), closed(&TOUCH));
}

/// Under a finger a confirm is the kit's `Dialog`: centred 24 in over the
/// scrim, a `border_strong` edge, the title 17 medium, a quiet Cancel and
/// the verb on a danger edge.
#[test]
fn under_touch_a_confirm_is_the_kits_dialog() {
    let theme = Theme::light();
    let (node, styles) = lowered(
        design::confirm(
            "overlay",
            "Message menu",
            div().id("screen"),
            Some(delete_ask()),
            &TOUCH,
            &theme,
        )
        .anchored(at())
        .on_dismiss(|_, _, _| {}),
    );
    let style = |node: &wire::Node| styles[node.style().expect("a styled node")].clone();
    let overlay = style(&node);
    assert_eq!(overlay.align_items, Some(gpui::AlignItems::Center));
    assert_eq!(overlay.padding.left, Some(design::space::XL.into()));
    assert_eq!(
        overlay.background,
        Some(hsla(0., 0., 0., 0.4).into()),
        "the light scrim"
    );
    let frame = &node.children()[1];
    assert_eq!(frame.key(), Some("delete-frame"));
    assert_eq!(style(frame).border_color, Some(theme.border_strong));
    let column = &frame.children()[0];
    let title = style(&column.children()[0]).text;
    assert_eq!(
        (title.font_size, title.font_weight),
        (Some(px(17.).into()), Some(FontWeight::MEDIUM))
    );
    let buttons = column.children()[2].children();
    assert_eq!(buttons[0].key(), Some("cancel"));
    let verb = style(&buttons[1]);
    assert_eq!(
        (verb.border_color, verb.text.color),
        (Some(theme.danger), Some(theme.danger))
    );
}

/// Send: under a pointer the outline text button forge drew beside its
/// reply; under a finger the kit's `Hit` with its glyph, the ink square
/// once there is something to send.
#[test]
fn send_is_a_text_button_under_a_pointer_and_the_kits_hit_under_touch() {
    let theme = Theme::light();
    let send = |ladder, sendable| {
        lowered(design::send(
            "send",
            "Send",
            "send-horizontal",
            sendable,
            ladder,
            &theme,
            |_, _, _| {},
        ))
    };
    for sendable in [false, true] {
        let today = lowered(
            design::button("send", "Send", &POINTER, &theme, |_, _, _| {})
                .kind(design::Kind::Outline)
                .enabled(sendable),
        );
        assert_eq!(send(&POINTER, sendable), today);
    }
    let (node, styles) = send(&TOUCH, true);
    let hit = &styles[node.style().expect("a styled hit")];
    assert_eq!(
        (hit.size.width, hit.size.height),
        (Some(px(44.).into()), Some(px(44.).into()))
    );
    assert_eq!(hit.background, Some(theme.primary.into()));
    assert!(
        matches!(&node.children()[0], wire::Node::Svg { source: wire::SvgSource::Asset(path), .. }
            if path == "icons/send-horizontal.svg"),
        "{:?}",
        node.children()[0]
    );
    let (faint, faint_styles) = send(&TOUCH, false);
    assert_eq!(
        faint_styles[faint.style().expect("a styled hit")].background,
        None
    );
}

/// A message row as a view draws it: a tap selects, a finger's hold and an
/// assistive technology's custom action open its actions.
#[derive(Default, Serialize, Deserialize)]
struct Held {
    taps: u32,
    holds: u32,
    opened: u32,
}
impl View for Held {
    const NAME: &'static str = "Held";
}
impl Render for Held {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let tap = cx.listener(|held, _: &ClickEvent, _, cx| {
            held.taps += 1;
            cx.notify();
        });
        let hold = cx.listener(|held, _: &ClickEvent, _, cx| {
            held.holds += 1;
            cx.notify();
        });
        let open = cx.listener(|held, _: &(), _, cx| {
            held.opened += 1;
            cx.notify();
        });
        let row = div()
            .id("row")
            .role(Role::Button)
            .aria_label("A message")
            .focusable()
            .on_click(design::tap_or_hold(tap, hold));
        div()
            .id("root")
            .child(design::opens_actions(row, "Message actions", open))
            .child(format!("{} {} {}", self.taps, self.holds, self.opened))
    }
}

#[test]
fn a_hold_and_the_custom_action_open_an_items_actions_and_a_tap_does_not() {
    let mut cx = TestAppContext::new();
    cx.open::<Held>();
    assert_eq!(
        cx.interactivity("row").aria.custom_actions,
        vec![(design::ACTIONS, "Message actions".to_owned())]
    );
    cx.simulate_click("A message");
    assert!(cx.has_text("1 0 0"), "{:?}", cx.texts());
    cx.simulate_long_press("A message");
    assert!(cx.has_text("1 1 0"), "{:?}", cx.texts());
    cx.simulate_a11y_action(
        "row",
        wire::Action::CustomAction,
        Some(wire::ActionData::CustomAction(design::ACTIONS)),
    );
    assert!(cx.has_text("1 1 1"), "{:?}", cx.texts());
}
