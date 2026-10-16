//! What both phone screens draw: a page's scroller, a press on a motion,
//! a vote's outcome strip and the confirm sheet a press that enacts
//! raises (Expo `src/network/voting.tsx` `Outcomes`, `usePress`, and
//! `src/kit/Confirm.tsx`), the node's last answer, and the Expo kit's
//! pieces the SDK has no helper for yet: the notice strip and a refused
//! read on it (`src/kit/Notice.tsx`), the small button
//! (`src/kit/Button.tsx`), the quiet line and the empty state
//! (`src/kit/States.tsx`). Reads the vote's outcome and what waits for a
//! yes; writes the press, the yes, Not now and Dismiss.
use valset::Motion;
use view::design::{Kind, space};
use view::prelude::*;
use view::{Div, FontWeight, Stateful};

use super::LADDER;
use crate::row::Status;
use crate::{Nodes, words};

// SDK gap: the sheet's inset (`design::SHEET_INSET`), swap when feat/phone-kit lands
/// How far a sheet's content sits in from its sides (Expo
/// `src/kit/Sheet.tsx:149,165` and `Confirm.tsx:65`: 20).
const SHEET_INSET: Pixels = px(20.);
// SDK gap: the small button (`Button::small`; with `small_button`), swap when feat/phone-kit lands
/// The kit's small button's height (Expo `src/design/theme.ts:76`
/// `size.small`, `Button.tsx:60`).
const SMALL_BUTTON: Pixels = px(36.);
// SDK gap: the notice and its action (with `notice`, `action`, `alert`);
// the SDK builds it later
/// A notice's action: its secondary line (18) with 6 over and under
/// (Expo `src/kit/Notice.tsx:58`).
const NOTICE_ACTION: Pixels = px(30.);
// SDK gap: the empty state (`design::empty_state`; with `empty`), swap when feat/phone-kit lands
/// The empty state's icon (Expo `src/design/theme.ts:87` `icon.empty`),
/// its padding (`States.tsx:26`: 40 over and under, 32 at the sides) and
/// how wide its body runs (`States.tsx:31`).
const EMPTY_ICON: Pixels = px(28.);
const EMPTY_Y: Pixels = px(40.);
const EMPTY_X: Pixels = px(32.);
const EMPTY_BODY_W: Pixels = px(280.);

// SDK gap: the page's scroller; the SDK builds it later
/// What a page scrolls, 16 clear at its end (Expo `index.tsx:182`
/// `paddingBottom: gap.block`).
pub(super) fn scroller(id: &'static str, body: Div) -> Stateful<Div> {
    div()
        .id(id)
        .flex_1()
        .min_h(px(0.))
        .overflow_y_scroll()
        .child(body.flex().flex_col().pb(space::BLOCK))
}

/// A press on `motion`: voted now, or once the reader says yes.
pub(super) fn press(
    cx: &Context<Nodes>,
    motion: Motion,
) -> impl Fn(&ClickEvent, &mut Window, &mut App) + 'static {
    cx.listener(move |view: &mut Nodes, _: &ClickEvent, _, cx| view.press(motion.clone(), cx))
}

/// What the last vote did on a success strip, or why it stopped on a
/// danger one, until dismissed.
pub(super) fn outcome(
    view: &Nodes,
    cx: &mut Context<Nodes>,
    theme: &Theme,
) -> Option<Stateful<Div>> {
    let outcome = view.voting.outcome.as_ref()?;
    let dismiss = cx.listener(|view: &mut Nodes, _: &ClickEvent, _, cx| view.dismiss_outcome(cx));
    let action = action("nodes-outcome-dismiss", words::DISMISS, dismiss);
    Some(match outcome {
        Ok(said) => notice("nodes-outcome", theme.success_soft, theme.success, said).child(action),
        Err(said) => alert(notice(
            "nodes-outcome",
            theme.danger_soft,
            theme.danger,
            said,
        ))
        .child(action),
    })
}

/// How long ago the node last answered, once a node that answered before
/// is silent.
pub(super) fn last_answer(view: &Nodes, id: &'static str, theme: &Theme) -> Option<Stateful<Div>> {
    let silent = view.status.ready().is_some() && !view.answering();
    silent.then(|| quiet(id, words::last_answer(view.ticks - view.answered), theme))
}

/// A press that enacts, asked as the kit's confirm card in a sheet: the
/// motion in words, that it cannot be taken back, Not now and Vote and
/// enact, and who signs.
pub(super) fn confirm(view: &Nodes, cx: &mut Context<Nodes>, theme: &Theme) -> Option<AnyElement> {
    let motion = view.voting.confirming.as_ref()?;
    let title = view.voting.words(motion);
    let hold = cx.listener(|view: &mut Nodes, _: &(), _, cx| view.hold(cx));
    Some(
        design::sheet("nodes-confirm-sheet", title.clone(), &LADDER, theme)
            .on_dismiss(hold)
            .child(confirm_card(title, cx, theme))
            .into_any_element(),
    )
}

/// The confirm card (Expo `Confirm.tsx:62-92`): the title, the warning,
/// the hold and the verb sharing the row one to two, the caption.
fn confirm_card(title: String, cx: &mut Context<Nodes>, theme: &Theme) -> Stateful<Div> {
    let no = cx.listener(|view: &mut Nodes, _: &ClickEvent, _, cx| view.hold(cx));
    let yes = cx.listener(|view: &mut Nodes, _: &ClickEvent, _, cx| view.confirm(cx));
    let not_now = design::button("nodes-confirm/cancel", words::NOT_NOW, &LADDER, theme, no)
        .kind(Kind::Quiet);
    let enact = design::button(
        "nodes-confirm/submit",
        words::VOTE_AND_ENACT,
        &LADDER,
        theme,
        yes,
    )
    .kind(Kind::Primary);
    div()
        .id("nodes-confirm")
        .flex()
        .flex_col()
        .pt(space::XS)
        .pb(space::LG)
        .child(
            div()
                .px(SHEET_INSET)
                .pb(space::MD)
                .text_type(LADDER.title)
                .font_weight(FontWeight::MEDIUM)
                .child(title),
        )
        .child(
            div()
                .px(SHEET_INSET)
                .pt(space::MD)
                .text_type(LADDER.secondary)
                .text_color(theme.warning)
                .child(words::ENACTS_NOW),
        )
        .child(
            div()
                .flex()
                .gap(space::SM)
                .px(SHEET_INSET)
                .pt(space::BLOCK)
                .child(share(1.).flex().flex_col().child(not_now))
                .child(share(2.).flex().flex_col().child(enact)),
        )
        .child(
            div()
                .px(SHEET_INSET)
                .pt(space::MD)
                .text_type(LADDER.caption)
                .text_color(theme.muted)
                .child("This phone's key signs it. The node runs the program's rules; a refusal comes back in its words."),
        )
}

// SDK gap: no `flex_grow(n)` (or `relative()`) on an element, so the
// style field is written; the SDK builds it later
/// A box taking `parts` of its row's room (Expo's `flex: n`).
pub(super) fn share(parts: f32) -> Div {
    let mut part = div().flex_basis(px(0.));
    part.style().flex_grow = Some(parts);
    part
}

/// A validator's status in colour (Expo `src/network/voting.tsx` `tintOf`,
/// `index.tsx:110,134`): its tone and its soft ground; plain, muted on the
/// raised surface, while the node is silent or the status unknown.
pub(super) fn tint(status: &Status, live: bool, theme: &Theme) -> (Hsla, Hsla) {
    match (live, status) {
        (true, Status::InSync) => (theme.success, theme.success_soft),
        (true, Status::Behind(_)) => (theme.warning, theme.warning_soft),
        (true, Status::Quiet { .. }) => (theme.danger, theme.danger_soft),
        _ => (theme.muted, theme.surface_raised),
    }
}

// SDK gap: the refused strip (`design::refused` under a finger), swap when feat/phone-kit lands
/// A refused read (the kit's `Refused`): its sentence on a danger strip,
/// Retry at the right. The strip is `{name}-refused`, its press
/// `{name}-retry`.
pub(super) fn refused(
    name: &str,
    sentence: &str,
    theme: &Theme,
    retry: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
) -> Stateful<Div> {
    let strip = notice(
        ElementId::Name(format!("{name}-refused").into()),
        theme.danger_soft,
        theme.danger,
        sentence,
    );
    alert(strip).child(action(
        ElementId::Name(format!("{name}-retry").into()),
        "Retry",
        retry,
    ))
}

/// A danger notice, said as soon as it shows (the kit's `alert` role).
fn alert(strip: Stateful<Div>) -> Stateful<Div> {
    strip
        .role(Role::Alert)
        .aria_live(view::accesskit::Live::Assertive)
}

/// The kit's `Notice`: a full-width strip of secondary text in `tone` on
/// `ground`, 16 in and 10 over and under; its one action goes after.
fn notice(id: impl Into<ElementId>, ground: Hsla, tone: Hsla, sentence: &str) -> Stateful<Div> {
    div()
        .id(id)
        .flex()
        .items_center()
        .gap(space::MD)
        .py(space::MD)
        .px(space::BLOCK)
        .bg(ground)
        .text_type(LADDER.secondary)
        .text_color(tone)
        .child(div().flex_1().child(sentence.to_owned()))
}

/// A notice's action: its word in the strip's tone, medium, 8 from the
/// words, its press reaching to the ladder's hit (the kit's `hitSlop`).
fn action(
    id: impl Into<ElementId>,
    label: &'static str,
    click: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
) -> Div {
    div()
        .relative()
        .flex_none()
        .h(NOTICE_ACTION)
        .flex()
        .items_center()
        .pl(space::SM)
        .font_weight(FontWeight::MEDIUM)
        .child(label)
        .child(
            design::hit(id, LADDER.hit, NOTICE_ACTION, &LADDER)
                .role(Role::Button)
                .aria_label(label)
                .focusable()
                .on_click(click),
        )
}

/// The kit's small outline button: a hairline edge, its label in
/// secondary medium, its press reaching to the ladder's hit. One that may
/// not be pressed now (a vote in flight) goes faint on a quiet edge and
/// takes no press.
pub(super) fn small_button(
    id: ElementId,
    label: &'static str,
    pressable: bool,
    theme: &Theme,
    click: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
) -> Div {
    let (edge, tone) = match pressable {
        true => (theme.border_strong, theme.foreground),
        false => (theme.border, theme.faint),
    };
    let hit = design::hit(id, LADDER.hit, SMALL_BUTTON, &LADDER)
        .role(Role::Button)
        .aria_label(label);
    let hit = match pressable {
        true => hit.focusable().on_click(click),
        false => hit.aria_disabled(true),
    };
    div()
        .relative()
        .flex_none()
        .h(SMALL_BUTTON)
        .px(space::LG)
        .flex()
        .items_center()
        .justify_center()
        .border_1()
        .border_color(edge)
        .text_type(LADDER.secondary)
        .font_weight(FontWeight::MEDIUM)
        .text_color(tone)
        .child(label)
        .child(hit)
}

// SDK gap: the quiet line (`design::quiet(text, ladder, theme)`), swap when feat/phone-kit lands
/// The kit's `Quiet`: one muted line of secondary text, 16 in all round.
pub(super) fn quiet(
    id: &'static str,
    text: impl Into<SharedString>,
    theme: &Theme,
) -> Stateful<Div> {
    design::quiet(text, theme)
        .id(id)
        .p(space::BLOCK)
        .text_type(LADDER.secondary)
}

/// The kit's `EmptyState`: a faint icon, what is missing in medium, what
/// would fill it muted, centred.
pub(super) fn empty(
    id: &'static str,
    icon: &str,
    title: &'static str,
    body: &'static str,
    theme: &Theme,
) -> Stateful<Div> {
    div()
        .id(id)
        .flex()
        .flex_col()
        .items_center()
        .justify_center()
        .gap(space::SM)
        .py(EMPTY_Y)
        .px(EMPTY_X)
        .child(design::icon(icon, EMPTY_ICON).text_color(theme.faint))
        .child(
            div()
                .text_type(LADDER.body)
                .font_weight(FontWeight::MEDIUM)
                .text_center()
                .child(title),
        )
        .child(
            div()
                .max_w(EMPTY_BODY_W)
                .text_type(LADDER.secondary)
                .text_color(theme.muted)
                .text_center()
                .child(body),
        )
}
