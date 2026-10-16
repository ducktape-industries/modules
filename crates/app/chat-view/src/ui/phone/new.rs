//! New channel (Expo `app/(tabs)/chat/new.tsx`, board 54): a name and who
//! may post, Create in the title row; the program's refusal under the
//! field, verbatim. Reads and writes the create form (`Chat::create`);
//! Create sends it, and the channel it made opens in this page's place.
use view::prelude::*;
use view::{Div, Stateful};

use super::parts;
use crate::words;
use crate::{ChannelCreate, Chat};
use view::FontWeight;
use view::design::Ladder;

pub(super) fn screen(
    chat: &Chat,
    back: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
    cx: &mut Context<Chat>,
    theme: &Theme,
    ladder: &Ladder,
) -> AnyElement {
    let Some(create) = &chat.create else {
        return div().into_any_element();
    };
    design::page("chat-new-page", "New channel", Some(back), ladder, theme)
        .trailing(create_button(create, cx, theme, ladder))
        .child(
            div()
                .id("chat-new-form")
                .flex_1()
                .min_h(px(0.))
                .overflow_y_scroll()
                .flex()
                .flex_col()
                .gap(design::space::BLOCK)
                .p(design::space::BLOCK)
                .child(name_field(create, cx, theme, ladder))
                .child(policy(create, cx, theme, ladder)),
        )
        .into_any_element()
}

/// The title row's Create (the kit `HitText`, testID `create`): body
/// medium, faint while the name is empty or the create is on its way,
/// which it says (Creating…).
fn create_button(
    create: &ChannelCreate,
    cx: &mut Context<Chat>,
    theme: &Theme,
    ladder: &Ladder,
) -> Stateful<Div> {
    let enabled = !create.name.is_blank() && !create.busy;
    let label = match create.busy {
        true => "Creating…",
        false => "Create",
    };
    let button = div()
        .id("create")
        .h(ladder.hit)
        .px(design::space::SM)
        .flex()
        .flex_none()
        .items_center()
        .text_type(ladder.body)
        .font_weight(FontWeight::MEDIUM)
        .text_color(if enabled {
            theme.foreground
        } else {
            theme.faint
        })
        .role(Role::Button)
        .aria_label(label)
        .aria_disabled(!enabled)
        .child(label);
    match enabled {
        true => button
            .focusable()
            .active(|style| style.opacity(parts::PRESSED))
            .on_click(cx.listener(|chat, _: &ClickEvent, _window, cx| {
                cx.notify();
                chat.create_channel(cx)
            })),
        false => button,
    }
}

/// The name, with why the last create did not go under it; the keyboard's
/// return creates.
fn name_field(
    create: &ChannelCreate,
    cx: &mut Context<Chat>,
    theme: &Theme,
    ladder: &Ladder,
) -> Div {
    let submit = cx.listener(|chat, _: &(), _window, cx| {
        cx.notify();
        chat.create_channel(cx)
    });
    let input = Input::new("chat-new-name", &create.name, "Name")
        .autocorrect(false)
        .autocapitalize(Autocapitalize::None)
        .on_submit(submit);
    let input = design::field(input, ladder, theme);
    let problem = (!create.error.is_empty()).then_some(create.error.as_str());
    parts::field("Name", input, problem, theme, ladder)
}

// SDK gap: the kit Segmented (segments share the width, 36 tall,
// secondary words; mobile `src/kit/Choice.tsx:16-35`), swap when
// feat/phone-kit lands
/// Who may post: Anyone or Members only.
fn policy(create: &ChannelCreate, cx: &mut Context<Chat>, theme: &Theme, ladder: &Ladder) -> Div {
    let pick = cx.processor(|chat, index: usize, _window, cx| {
        if let Some(create) = &mut chat.create {
            create.members_only = index == 1;
        }
        cx.notify();
    });
    let choices = [
        (ElementId::from("chat-new-anyone"), "Anyone".into()),
        (
            ElementId::from("chat-new-members-only"),
            words::MEMBERS_ONLY.into(),
        ),
    ];
    div()
        .flex()
        .flex_col()
        .gap(design::space::XS)
        .child(
            div()
                .text_type(ladder.caption)
                .text_color(theme.muted)
                .child("Who may post"),
        )
        .child(design::segmented(
            "chat-new-policy",
            "Who may post",
            ladder,
            theme,
            Some(usize::from(create.members_only)),
            choices,
            pick,
        ))
}
