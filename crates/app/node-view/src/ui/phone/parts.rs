//! What both phone screens draw: a page's scroller, a press on a motion,
//! a vote's outcome strip and the confirm sheet a press that enacts
//! raises (Expo `src/network/voting.tsx` `Outcomes`, `usePress`, and
//! `src/kit/Confirm.tsx`), the node's last answer, the kit's small button
//! as a vote draws it, and the Expo kit's pieces the SDK has no helper for
//! yet: the notice strip (`src/kit/Notice.tsx`) and the quiet line
//! (`src/kit/States.tsx`). Reads the vote's outcome and what waits for a
//! yes; writes the press, the yes, Not now and Dismiss.
use valset::Motion;
use view::design::{Kind, SHEET_INSET, space};
use view::prelude::*;
use view::{Div, FontWeight, Stateful};

use super::LADDER;
use crate::row::Status;
use crate::{Nodes, words};

// SDK gap: the notice and its action (with `notice`, `action`, `alert`);
// the SDK builds it later
/// A notice's action: its secondary line (18) with 6 over and under, its
/// press reaching 8 past each edge (Expo `src/kit/Notice.tsx:60`,
/// `hitSlop={8}`).
const NOTICE_ACTION: Pixels = px(30.);
const NOTICE_REACH: Pixels = px(8.);
// SDK gap: a button sized by its caller (`design::button` takes the
// ladder's control height); the SDK builds it later
/// How tall the confirm card's Not now stands, as its Vote and enact
/// (Expo `src/kit/Confirm.tsx:84` `height: size.primary`, `theme.ts:75`).
const CONFIRM_ROW: Pixels = px(48.);

// SDK gap: the page's scroller; the SDK builds it later
/// What a page scrolls, 16 clear at its end (Expo `index.tsx:170` and
/// `[key].tsx:58`, `paddingBottom: gap.block`).
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

/// What the last vote did on a success strip, on the screen it was
/// pressed on; or why it stopped on a danger one, on every screen; until
/// dismissed.
pub(super) fn outcome(
    view: &Nodes,
    cx: &mut Context<Nodes>,
    theme: &Theme,
) -> Option<Stateful<Div>> {
    let outcome = view.voting.outcome.as_ref()?;
    if outcome.is_ok() && view.voting.pressed_on != view.open {
        return None;
    }
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
                .child(
                    share(1.)
                        .h(CONFIRM_ROW)
                        .flex()
                        .flex_col()
                        .justify_center()
                        .child(not_now),
                )
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
/// sentence, its press reaching 8 past each edge (the kit's `hitSlop`).
/// Not `design::hit`: that reaches to the ladder's hit from the box's
/// width, and a view can't measure its word.
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
            div()
                .id(id)
                .absolute()
                .top(-NOTICE_REACH)
                .bottom(-NOTICE_REACH)
                .left(-NOTICE_REACH)
                .right(-NOTICE_REACH)
                .role(Role::Button)
                .aria_label(label)
                .focusable()
                .on_click(click),
        )
}

/// The kit's small outline button; one that may not be pressed now (a
/// vote in flight) is disabled.
pub(super) fn small_button(
    id: ElementId,
    label: &'static str,
    pressable: bool,
    theme: &Theme,
    click: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
) -> impl IntoElement {
    design::button(id, label, &LADDER, theme, click)
        .kind(Kind::Outline)
        .small()
        .enabled(pressable)
}

// SDK gap: the quiet line at the ladder's sizes; the SDK builds it later
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
