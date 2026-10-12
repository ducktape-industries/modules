//! The vote on the sheet, for a reader who votes: what a press about to
//! enact asks first, what the last vote did, the motions that wait, Change
//! a module, and the scheduled changes. `render` reads the state and
//! changes nothing; presses land in `voting.rs`.
use ducktape_view_guest::design::Kind;
use ducktape_view_guest::prelude::*;
use ducktape_view_guest::{Div, Input, Stateful};

use crate::voting;
use crate::{Nodes, ui};

/// A press about to enact: what will happen, asked once.
pub(crate) fn confirm(
    view: &Nodes,
    cx: &mut Context<Nodes>,
    theme: &Theme,
) -> Option<Stateful<Div>> {
    let motion = view.voting.confirming.as_ref()?;
    let yes = cx.listener(|view: &mut Nodes, _: &ClickEvent, _, cx| view.confirm(cx));
    let no = cx.listener(|view: &mut Nodes, _: &ClickEvent, _, cx| view.hold(cx));
    Some(
        div()
            .id("nodes-confirm")
            .flex()
            .flex_wrap()
            .items_center()
            .gap_2()
            .p_3()
            .border_1()
            .border_color(theme.warning)
            .bg(theme.warning_soft)
            .child(
                div()
                    .flex_1()
                    .min_w(px(240.))
                    .flex()
                    .flex_col()
                    .gap_1()
                    .child(div().font_weight(ducktape_view_guest::FontWeight::MEDIUM).child(view.voting.words(motion)))
                    .child(
                        div()
                            .text_size(design::text::SECONDARY)
                            .child("Your vote reaches the quorum: this happens now, and a vote can't be taken back."),
                    ),
            )
            .child(design::button("nodes-confirm-yes", "Vote and enact", theme, yes).kind(Kind::Primary))
            .child(design::button("nodes-confirm-no", "Not now", theme, no).kind(Kind::Outline)),
    )
}

/// What the last vote did, or the refusal it met, until dismissed.
pub(crate) fn outcome(
    view: &Nodes,
    cx: &mut Context<Nodes>,
    theme: &Theme,
) -> Option<Stateful<Div>> {
    let outcome = view.voting.outcome.as_ref()?;
    let (text, color, background) = match outcome {
        Ok(text) => (text.clone(), theme.foreground, theme.surface),
        Err(text) => (text.clone(), theme.danger, theme.danger_soft),
    };
    let dismiss = cx.listener(|view: &mut Nodes, _: &ClickEvent, _, cx| view.dismiss_outcome(cx));
    Some(
        div()
            .id("nodes-outcome")
            .flex()
            .items_center()
            .gap_2()
            .px_2()
            .min_h(design::size::CONTROL)
            .bg(background)
            .text_color(color)
            .text_size(design::text::SECONDARY)
            .child(div().flex_1().child(text))
            .child(design::icon_button(
                "nodes-outcome-dismiss",
                "✕",
                "Dismiss",
                theme,
                dismiss,
            )),
    )
}

/// The vote's three sections under the table: nothing for a reader who
/// does not vote.
pub(crate) fn sections(view: &Nodes, cx: &mut Context<Nodes>, theme: &Theme) -> Option<Div> {
    if !view.voting.votes() {
        return None;
    }
    Some(
        div()
            .flex()
            .flex_col()
            .gap_3()
            .pt_4()
            .child(pending(view, cx, theme))
            .child(change(view, cx, theme))
            .child(scheduled(view, cx, theme)),
    )
}

/// One line of a section: its words, a quiet caption, a press.
fn line(id: impl Into<ElementId>, words: String, caption: String, theme: &Theme) -> Stateful<Div> {
    div()
        .id(id)
        .flex()
        .flex_wrap()
        .items_center()
        .gap_x_3()
        .gap_y_1()
        .px_2()
        .py_1()
        .min_h(design::size::CONTROL + design::space::SM)
        .border_b_1()
        .border_color(theme.border)
        .text_size(design::text::SECONDARY)
        .child(div().flex_1().min_w(px(200.)).child(words))
        .child(div().text_color(theme.muted).child(caption))
}

/// Each motion that waits: its words, its votes of the quorum, which of the
/// reader's validators voted, and Vote while one has not.
fn pending(view: &Nodes, cx: &mut Context<Nodes>, theme: &Theme) -> Div {
    let voting = &view.voting;
    let lines: Vec<AnyElement> = match &voting.motions {
        Loadable::Ready(motions) | Loadable::Reloading(motions, _)
            if motions.pending.is_empty() =>
        {
            vec![quiet(
                "nodes-motions-none",
                "No motion waits for votes.",
                theme,
            )]
        }
        Loadable::Ready(motions) | Loadable::Reloading(motions, _) => motions
            .pending
            .iter()
            .enumerate()
            .map(|(index, pending)| {
                let cast = voting.voices.len() - voting.uncast(&pending.motion);
                let caption = format!(
                    "{} of {} · {}",
                    pending.voters.len(),
                    motions.needed,
                    yours(cast, voting.voices.len())
                );
                let words = voting.words(&pending.motion);
                let row = line(
                    ElementId::Name(format!("nodes-motion-{index}").into()),
                    words,
                    caption,
                    theme,
                );
                let open = voting.uncast(&pending.motion) > 0;
                let press = open.then(|| {
                    let motion = pending.motion.clone();
                    let vote = cx.listener(move |view: &mut Nodes, _: &ClickEvent, _, cx| {
                        view.press(motion.clone(), cx)
                    });
                    design::button(
                        ElementId::Name(format!("nodes-motion-vote-{index}").into()),
                        "Vote",
                        theme,
                        vote,
                    )
                    .kind(Kind::Outline)
                    .enabled(voting.pressable())
                });
                row.children(press).into_any_element()
            })
            .collect(),
        Loadable::Failed(refusal) => vec![quiet(
            "nodes-motions-refused",
            refusal.message.clone(),
            theme,
        )],
        Loadable::Idle | Loadable::Loading(_) => vec![quiet(
            "nodes-motions-loading",
            "Reading the motions…",
            theme,
        )],
    };
    div()
        .flex()
        .flex_col()
        .child(ui::section("nodes-motions", "Motions".into(), theme))
        .children(lines)
}

/// Which of the reader's validators voted: "yours voted", "1 of 2 of yours".
fn yours(cast: usize, voices: usize) -> String {
    match (cast, voices) {
        (0, 1) => "yours has not voted".into(),
        (1, 1) => "yours voted".into(),
        (cast, voices) => format!("{cast} of your {voices} voted"),
    }
}

/// Change a module: a program from the roster, a published code id, the
/// height; Propose votes the change.
fn change(view: &Nodes, cx: &mut Context<Nodes>, theme: &Theme) -> Div {
    let form = &view.voting.change;
    let entries = view.voting.roster.ready().map_or(&[][..], Vec::as_slice);
    let programs = entries.iter().map(|entry| {
        let program = entry.program.clone();
        let picked = form.program.as_deref() == Some(program.as_str());
        let pick = cx.listener(move |view: &mut Nodes, _: &ClickEvent, _, cx| {
            view.pick(program.clone(), cx)
        });
        design::button(
            ElementId::Name(format!("nodes-change-program-{}", entry.program).into()),
            entry.program.clone(),
            theme,
            pick,
        )
        .selected(picked)
    });
    let current = form
        .program
        .as_ref()
        .and_then(|program| entries.iter().find(|entry| &entry.program == program))
        .map(|entry| {
            design::quiet(
                format!(
                    "Runs code {} now; its params stay.",
                    voting::code_words(&entry.code)
                ),
                theme,
            )
        });
    let field = |id: &str, field: &TextField, name: &str, hint: &str| {
        Input::new(id.to_owned(), field, name.to_owned())
            .h(design::size::CONTROL)
            .w(px(280.))
            .px_2()
            .border_1()
            .border_color(theme.border_strong)
            .bg(theme.background)
            .text_size(design::text::SECONDARY)
            .placeholder(hint.to_owned())
    };
    let propose = cx.listener(|view: &mut Nodes, _: &ClickEvent, _, cx| view.propose(cx));
    div()
        .flex()
        .flex_col()
        .gap_2()
        .child(ui::section("nodes-change", "Change a module".into(), theme))
        .child(
            div()
                .id("nodes-change-programs")
                .px_2()
                .flex()
                .flex_wrap()
                .gap_2()
                .children(programs),
        )
        .children(current.map(|line| line.px_2()))
        .child(
            div()
                .px_2()
                .flex()
                .flex_wrap()
                .items_center()
                .gap_2()
                .child(field(
                    "nodes-change-code",
                    &form.code,
                    "Code id",
                    "sha256:… of the published code",
                ))
                .child(
                    field(
                        "nodes-change-height",
                        &form.height,
                        "At block",
                        "block height",
                    )
                    .w(px(140.)),
                )
                .child(
                    design::button("nodes-change-propose", "Propose", theme, propose)
                        .kind(Kind::Primary)
                        .enabled(view.voting.pressable()),
                ),
        )
        .children(form.problem.as_ref().map(|problem| {
            div()
                .id("nodes-change-problem")
                .px_2()
                .text_size(design::text::SECONDARY)
                .text_color(theme.danger)
                .child(problem.clone())
        }))
}

/// The registry's changes still to land, each with Cancel; one at or below
/// the tip has landed and is not listed.
fn scheduled(view: &Nodes, cx: &mut Context<Nodes>, theme: &Theme) -> Div {
    let voting = &view.voting;
    let tip = view.status.ready().map_or(0, |status| status.height);
    let entries = voting.roster.ready().map_or(&[][..], Vec::as_slice);
    let lines: Vec<AnyElement> = match &voting.scheduled {
        Loadable::Ready(scheduled) | Loadable::Reloading(scheduled, _) => {
            let ahead: Vec<_> = scheduled
                .iter()
                .filter(|change| change.height > tip)
                .collect();
            if ahead.is_empty() {
                vec![quiet(
                    "nodes-scheduled-none",
                    "No change is scheduled.",
                    theme,
                )]
            } else {
                ahead
                    .into_iter()
                    .enumerate()
                    .map(|(index, change)| {
                        let motion = voting::cancel(change);
                        let caption = format!(
                            "in {}",
                            design::plural(change.height - tip, "block", "blocks")
                        );
                        let row = line(
                            ElementId::Name(format!("nodes-scheduled-{index}").into()),
                            voting::scheduled_words(change, entries),
                            caption,
                            theme,
                        );
                        let press = cx.listener(move |view: &mut Nodes, _: &ClickEvent, _, cx| {
                            view.press(motion.clone(), cx)
                        });
                        row.child(
                            design::button(
                                ElementId::Name(format!("nodes-scheduled-cancel-{index}").into()),
                                "Cancel",
                                theme,
                                press,
                            )
                            .kind(Kind::Outline)
                            .enabled(voting.pressable()),
                        )
                        .into_any_element()
                    })
                    .collect()
            }
        }
        Loadable::Failed(refusal) => vec![quiet(
            "nodes-scheduled-refused",
            refusal.message.clone(),
            theme,
        )],
        Loadable::Idle | Loadable::Loading(_) => vec![quiet(
            "nodes-scheduled-loading",
            "Reading the schedule…",
            theme,
        )],
    };
    div()
        .flex()
        .flex_col()
        .child(ui::section(
            "nodes-scheduled",
            "Scheduled changes".into(),
            theme,
        ))
        .children(lines)
}

fn quiet(id: &'static str, text: impl Into<SharedString>, theme: &Theme) -> AnyElement {
    design::quiet(text, theme)
        .id(id)
        .px_2()
        .py_1()
        .into_any_element()
}
