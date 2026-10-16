//! Finish your review (Expo
//! `app/(tabs)/forge/[repo]/changes/[n]/review.tsx`, board 71), a sheet
//! over the change: what the review says overall, the verdict in the
//! desktop's words, then every staged line comment and the verdict as one
//! `ReviewSubmit`; or Discard.
//!
//! Reads the review being written; writes its body and verdict, submits
//! it (`finish_review`) or drops it (`cancel_review`).
use view::design::{Kind, Ladder};
use view::prelude::*;
use view::{Div, Stateful};

use super::parts::{self, Row};
use crate::Forge;
use crate::id;
use crate::state::{ReviewSession, verdict_label};
use crate::words::{self, VERDICTS};
use forge::Verdict;
use view::design::short_hex;

pub(super) fn render(
    forge: &Forge,
    review: &ReviewSession,
    cx: &mut Context<Forge>,
    ladder: &Ladder,
    theme: &Theme,
) -> AnyElement {
    let close = cx.listener(|forge, _: &(), _, cx| forge.finishing(false, cx));
    let staged = design::plural(
        review.comments.len() as u64,
        "line comment",
        "line comments",
    );
    let head = parts::sheet_head(
        "forge-finish-title",
        words::FINISH_YOUR_REVIEW,
        ladder,
        theme,
    )
    .child(parts::caption(
        format!("{staged} staged against {}", short_hex(&review.commit)),
        ladder,
        theme,
    ));
    let body = div()
        .id(id("forge-finish-body"))
        .size_full()
        .overflow_y_scroll()
        .flex()
        .flex_col()
        .child(head)
        .child(overall(review, ladder, theme))
        .child(verdicts(review, cx, ladder, theme))
        .child(actions(forge, review, cx, ladder, theme));
    design::sheet(
        id("forge-finish-sheet"),
        words::FINISH_YOUR_REVIEW,
        ladder,
        theme,
    )
    .on_dismiss(close)
    .child(body)
    .into_any_element()
}

/// What the review says overall, and why it was refused; 16 in, 4 under.
fn overall(review: &ReviewSession, ladder: &Ladder, theme: &Theme) -> Div {
    let field = Textarea::new(id("forge-review-body"), &review.body, words::REVIEW_BODY)
        .autocorrect(true)
        .autocapitalize(Autocapitalize::Sentences)
        .placeholder(words::REVIEW_PROMPT);
    div()
        .p(design::space::BLOCK)
        .pb(design::space::XS)
        .child(parts::text_area_field(
            None,
            field,
            &review.error,
            ladder,
            theme,
        ))
}

/// The verdicts as a radio group, a row each with its mark at the right:
/// one Tab stop, ↑ ↓ pick the next; a press picks one.
fn verdicts(
    review: &ReviewSession,
    cx: &mut Context<Forge>,
    ladder: &Ladder,
    theme: &Theme,
) -> Stateful<Div> {
    let at = VERDICTS
        .iter()
        .position(|(verdict, ..)| review.verdict == Some(*verdict))
        .unwrap_or_default();
    let group = design::composite(id("forge-verdicts"), Role::RadioGroup, "Verdict")
        .active(at, VERDICTS.len())
        .wrap()
        .on_move(cx.processor(|forge, index: usize, _, cx| {
            if let Some((verdict, ..)) = VERDICTS.get(index) {
                forge.pick_verdict(*verdict, cx);
            }
        }))
        .build()
        .flex()
        .flex_col();
    group.children(
        VERDICTS
            .iter()
            .enumerate()
            .map(|(index, &(verdict, slug, said))| {
                let on = review.verdict == Some(verdict);
                let pick = cx
                    .listener(move |forge, _: &ClickEvent, _, cx| forge.pick_verdict(verdict, cx));
                let label = verdict_label(verdict);
                let row = Row::titled(id(format!("forge-verdict-{slug}")), label, ladder)
                    .caption(parts::caption(said, ladder, theme))
                    .tail(parts::radio_mark(on, theme))
                    .draw(ladder, theme)
                    .aria_toggled(on.into())
                    .on_click(pick);
                design::item(row, Role::RadioButton, index == at)
            }),
    )
}

/// Discard (one part) and Submit review (two), 12 apart, 16 in.
fn actions(
    forge: &Forge,
    review: &ReviewSession,
    cx: &mut Context<Forge>,
    ladder: &Ladder,
    theme: &Theme,
) -> Div {
    let verdict = review.verdict.unwrap_or(Verdict::Comment);
    let discard = cx.listener(|forge, _: &ClickEvent, _, cx| forge.cancel_review(cx));
    let submit = cx.listener(move |forge, _: &ClickEvent, _, cx| forge.finish_review(verdict, cx));
    div()
        .flex()
        .gap(design::space::LG)
        .p(design::space::BLOCK)
        .child(parts::share(
            1.,
            design::button(
                id("forge-discard-review-sheet"),
                "Discard",
                ladder,
                theme,
                discard,
            )
            .kind(Kind::Quiet),
        ))
        .child(parts::share(
            2.,
            design::button(
                id("forge-submit-review"),
                words::SUBMIT_REVIEW,
                ladder,
                theme,
                submit,
            )
            .kind(Kind::Primary)
            .enabled(forge.me_principal().is_some()),
        ))
}
