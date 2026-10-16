//! What the phone's chat screens share (Expo `src/chat/parts.tsx`, and the
//! kit pieces the SDK does not draw yet: `Notice`, `Refused`, `Quiet`, a
//! labelled `Field`, a small `Button`): a person's face, the room's notice
//! and write-gate strip. Reads the open room's notice and gate; writes
//! only what their actions do (dismiss the notice, unarchive).
use view::prelude::*;
use view::{Div, Stateful};

use crate::Chat;
use crate::session::Gate;
use view::FontWeight;
use view::design::Ladder;

/// A hairline (mobile `src/design/theme.ts:83`, `size.hairline`).
pub(super) const HAIRLINE: Pixels = px(1.);
/// A row with a caption under its title (mobile `src/design/theme.ts:73`,
/// `size.rowTall`).
pub(super) const ROW_TALL: Pixels = px(56.);
/// The kit `Notice`'s icon (mobile `src/kit/Notice.tsx:49`).
const NOTICE_ICON: Pixels = px(18.);
/// A notice action's drawn height: its secondary line and `gap.xs` over
/// and under (mobile `src/kit/Notice.tsx:60`); its press reaches `hit`.
const NOTICE_ACTION_HEIGHT: Pixels = px(30.);
/// A small kit `Button` (mobile `src/design/theme.ts:76`, `size.small`)
/// and its icon (mobile `src/kit/Button.tsx:75`).
const SMALL_BUTTON: Pixels = px(36.);
const SMALL_BUTTON_ICON: Pixels = px(16.);

/// What a press does to a pressable the SDK's helpers do not draw: it dims
/// while held (mobile `src/kit/platform.tsx:36`, `platform.pressed`).
pub(super) const PRESSED: f32 = 0.6;

/// A person's round initial at `size` (the kit's `Avatar`), on the agent
/// tint for an agent.
pub(super) fn face(name: &str, agent: bool, size: design::Face, theme: &Theme) -> Div {
    design::avatar(name, size, theme).when(agent, |face| {
        face.bg(theme.agent_soft).text_color(theme.agent)
    })
}

/// The kit `Quiet`: one muted line of secondary text, 16 in all round.
pub(super) fn quiet(
    id: impl Into<ElementId>,
    text: impl Into<SharedString>,
    theme: &Theme,
    ladder: &Ladder,
) -> Stateful<Div> {
    div()
        .id(id)
        .p(design::space::BLOCK)
        .text_type(ladder.secondary)
        .text_color(theme.muted)
        .child(text.into())
}

/// What a kit `Notice` says by its ground and tone.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum NoticeKind {
    Danger,
    Warning,
    /// on the surface, a hairline under it
    Quiet,
}

/// The kit `Notice` (mobile `src/kit/Notice.tsx`): secondary words on its
/// kind's ground, 16 in and 10 over and under, an icon before them and
/// its one action at the right. A danger notice is an alert.
pub(super) fn notice(
    id: impl Into<ElementId>,
    kind: NoticeKind,
    icon: Option<&'static str>,
    words: impl Into<SharedString>,
    action: Option<AnyElement>,
    theme: &Theme,
    ladder: &Ladder,
) -> Stateful<Div> {
    let (ground, tone) = match kind {
        NoticeKind::Danger => (theme.danger_soft, theme.danger),
        NoticeKind::Warning => (theme.warning_soft, theme.warning),
        NoticeKind::Quiet => (theme.surface, theme.muted),
    };
    div()
        .id(id)
        .flex()
        .items_center()
        .gap(design::space::MD)
        .py(design::space::MD)
        .px(design::space::BLOCK)
        .bg(ground)
        .when(kind == NoticeKind::Quiet, |strip| {
            strip.border_b_1().border_color(theme.border)
        })
        .when(kind == NoticeKind::Danger, |strip| {
            strip
                .role(Role::Alert)
                .aria_live(view::accesskit::Live::Assertive)
        })
        .text_type(ladder.secondary)
        .text_color(tone)
        .children(icon.map(|icon| design::icon(icon, NOTICE_ICON).text_color(tone)))
        .child(div().flex_1().child(words.into()))
        .children(action)
}

/// A notice's action: secondary medium in the notice's tone, its press
/// reaching to `hit`.
pub(super) fn notice_action(
    id: impl Into<ElementId>,
    label: &'static str,
    ladder: &Ladder,
    click: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
) -> AnyElement {
    div()
        .relative()
        .h(NOTICE_ACTION_HEIGHT)
        .flex()
        .items_center()
        .pl(design::space::SM)
        .font_weight(FontWeight::MEDIUM)
        .child(label)
        .child(
            design::hit(id, ladder.hit, NOTICE_ACTION_HEIGHT, ladder)
                .role(Role::Button)
                .aria_label(label)
                .focusable()
                .on_click(click),
        )
        .into_any_element()
}

/// The kit `Refused`: a read the node refused, its sentence verbatim in a
/// danger notice, and Retry. The notice is `{name}-refused`, Retry
/// `{name}-retry`.
pub(super) fn refused(
    name: &str,
    sentence: impl Into<SharedString>,
    theme: &Theme,
    ladder: &Ladder,
    retry: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
) -> AnyElement {
    let retry = notice_action(format!("{name}-retry"), "Retry", ladder, retry);
    notice(
        format!("{name}-refused"),
        NoticeKind::Danger,
        None,
        sentence,
        Some(retry),
        theme,
        ladder,
    )
    .into_any_element()
}

/// The kit's small outline `Button`: 36 tall, 12 in, secondary medium, an
/// icon before its label; its press reaches `hit`. Drawn on the window's
/// ground, so it reads over a list.
pub(super) fn small_button(
    id: impl Into<ElementId>,
    icon: Option<&'static str>,
    label: &'static str,
    theme: &Theme,
    ladder: &Ladder,
    click: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
) -> Div {
    div()
        .relative()
        .h(SMALL_BUTTON)
        .flex()
        .flex_none()
        .items_center()
        .justify_center()
        .gap(design::space::SM)
        .px(design::space::LG)
        .border_1()
        .border_color(theme.border_strong)
        .bg(theme.background)
        .text_type(ladder.secondary)
        .font_weight(FontWeight::MEDIUM)
        .text_color(theme.foreground)
        .children(icon.map(|icon| design::icon(icon, SMALL_BUTTON_ICON)))
        .child(label)
        .child(
            design::hit(id, ladder.hit, SMALL_BUTTON, ladder)
                .role(Role::Button)
                .aria_label(label)
                .focusable()
                .on_click(click),
        )
}

/// Who may post in `channel`, as its caption and its details say it.
pub(super) fn policy(channel: &chat::ChannelRow) -> &'static str {
    match (channel.archived, channel.members_only()) {
        (true, _) => "Archived",
        (false, true) => "Members only",
        (false, false) => "Open",
    }
}

/// The kit `Field`: `input` (a [`design::field`]) with its label over it
/// (caption, muted) and, where the value will not do, its problem under it
/// in danger on a 2 edge.
pub(super) fn field(
    label: &'static str,
    input: Input,
    problem: Option<&str>,
    theme: &Theme,
    ladder: &Ladder,
) -> Div {
    // a problem's edge is 2 (mobile `src/kit/Field.tsx:38`); the thicker
    // edge eats into the padding, so the words stay put
    let input = match problem {
        Some(_) => input
            .border_2()
            .border_color(theme.danger)
            .px(design::space::LG - HAIRLINE),
        None => input,
    };
    let caption = |words: SharedString, tone: Hsla| {
        div()
            .text_type(ladder.caption)
            .text_color(tone)
            .child(words)
    };
    div()
        .flex()
        .flex_col()
        .child(caption(label.into(), theme.muted).mb(design::space::XS))
        .child(input)
        .children(
            problem.map(|problem| {
                caption(problem.to_owned().into(), theme.danger).mt(design::space::XS)
            }),
        )
}

/// The notice over a channel's screens (Expo `ChannelNotice`): a write the
/// program refused, in danger, until dismissed.
pub(super) fn room_notice(
    chat: &Chat,
    cx: &mut Context<Chat>,
    theme: &Theme,
    ladder: &Ladder,
) -> Option<AnyElement> {
    if chat.notice.is_empty() {
        return None;
    }
    let dismiss = cx.listener(|chat, _: &ClickEvent, _window, cx| {
        chat.notice.clear();
        cx.notify();
    });
    let dismiss = notice_action("chat-room-notice-dismiss", "Dismiss", ladder, dismiss);
    let words = chat.notice.clone();
    Some(
        notice(
            "chat-room-notice",
            NoticeKind::Danger,
            None,
            words,
            Some(dismiss),
            theme,
            ladder,
        )
        .into_any_element(),
    )
}

/// The strip in the composer's place where the reader may not write
/// (Expo `GateStrip`, the desktop's words): archived in warning, with
/// Unarchive for the room's owner; members-only with its lock; no
/// account, quiet.
pub(super) fn gate_strip(
    chat: &Chat,
    gate: Gate,
    cx: &mut Context<Chat>,
    theme: &Theme,
    ladder: &Ladder,
) -> AnyElement {
    let (kind, icon, words) = match gate {
        Gate::Archived => (
            NoticeKind::Warning,
            None,
            "This channel is archived. It keeps its history and takes no new messages.",
        ),
        Gate::NotMember => (
            NoticeKind::Quiet,
            Some("lock"),
            "This channel is members-only and your key is not on its roster. Ask a member to add your key from Channel details.",
        ),
        Gate::NoAccount => (
            NoticeKind::Quiet,
            None,
            "To send messages, create or join an account in Account. You can read this channel without an account.",
        ),
    };
    let unarchive = (gate == Gate::Archived && chat.owns_room()).then(|| {
        let unarchive = cx.listener(|chat, _: &ClickEvent, _window, cx| {
            cx.notify();
            chat.set_archived(false, cx);
        });
        notice_action("chat-room-unarchive", "Unarchive", ladder, unarchive)
    });
    notice(
        "chat-room-write-refusal",
        kind,
        icon,
        words,
        unarchive,
        theme,
        ladder,
    )
    .into_any_element()
}
