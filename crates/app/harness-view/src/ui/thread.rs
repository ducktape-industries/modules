//! One thread, read only: its state, the input held behind its loop, its context and usage,
//! and the transcript, one row per frame, each tool call opened to show its input and how
//! far it got. Its manager closes it from here.
use harness::{Body, CallRow, CallState, Frame, Said, Stop};
use view::design::Ladder;
use view::design::{fonts, space, text};
use view::prelude::*;
use view::{Div, FontWeight};

use super::{
    asking, back, caption, context_words, meter, no_node, panel, place_label, state_badge,
    usage_words, waiting,
};
use crate::read::{self, Roster, Transcript};
use crate::{Agents, Arm, Screen, State, links, thread_state};

pub(super) fn render(
    view: &Agents,
    id: u64,
    cx: &mut Context<Agents>,
    theme: &Theme,
    ladder: &Ladder,
) -> AnyElement {
    let shown = view.transcript.ready().and_then(Option::as_ref);
    let back = match shown {
        Some(shown) => {
            let agent = shown.thread.agent;
            let name = view
                .roster
                .ready()
                .map_or_else(|| format!("account {agent}"), |roster| roster.name(agent));
            back(
                "thread-back",
                format!("← {name}").into(),
                Screen::Agent(agent),
                cx,
                theme,
                ladder,
            )
            .into_any_element()
        }
        None => back(
            "thread-back",
            "← Agents".into(),
            Screen::List,
            cx,
            theme,
            ladder,
        )
        .into_any_element(),
    };
    let body = match (
        waiting("thread", &view.transcript, cx, theme, ladder),
        shown,
    ) {
        (Some(waiting), _) => waiting,
        (None, None) => design::empty_state(
            "thread-gone",
            "message-square",
            "No such thread",
            format!("Harness holds no thread {id}."),
            ladder,
            theme,
        )
        .into_any_element(),
        (None, Some(shown)) => thread(view, shown, cx, theme, ladder),
    };
    div()
        .flex()
        .flex_col()
        .gap(space::BLOCK)
        .child(div().flex().child(back))
        .child(body)
        .into_any_element()
}

fn thread(
    view: &Agents,
    shown: &Transcript,
    cx: &mut Context<Agents>,
    theme: &Theme,
    ladder: &Ladder,
) -> AnyElement {
    let roster = view.roster.ready();
    let row = &shown.thread;
    let id = row.id;
    let manages = view.me == Some(row.manager);
    let relaying = roster.map_or(1, |roster| roster.relaying);
    let state = match (shown.serving, relaying) {
        (0, _) | (_, 0) => State::NoNode,
        _ => thread_state(row),
    };
    let place = place_label(row, roster);
    let opened = links::place_link(&view.chain, &row.place);
    let arm = cx.listener(move |view: &mut Agents, _: &ClickEvent, _, cx| {
        view.arming = Some(Arm::Close(id));
        cx.notify();
    });
    let head = div()
        .flex()
        .flex_wrap()
        .items_center()
        .gap(space::LG)
        .child(
            div()
                .flex_1()
                .flex()
                .flex_col()
                .gap(space::HAIR)
                .child(design::heading("thread-place", place, 1, theme))
                .child(caption(
                    format!(
                        "thread {id} · opened at block {} · {} · max output {}",
                        design::grouped(row.opened),
                        row.model,
                        design::grouped(row.max_tokens.into()),
                    ),
                    theme,
                )),
        )
        .children(opened.map(|(program, link)| {
            let label = match program == chat::PROGRAM {
                true => "Open in chat".to_owned(),
                false => format!("Open in {program}"),
            };
            design::button(
                "thread-open",
                label,
                ladder,
                theme,
                move |_, _, cx: &mut App| cx.host().open_link(&link),
            )
        }))
        .when(manages && !row.closed, |head| {
            head.child(design::button(
                "thread-close",
                "Close thread…",
                ladder,
                theme,
                arm,
            ))
        });
    let summaries = shown
        .frames
        .iter()
        .filter(|frame| matches!(frame.body, Body::Compaction { .. }))
        .count();
    let turns_since = shown
        .frames
        .iter()
        .rev()
        .take_while(|frame| !matches!(frame.body, Body::Post { .. }))
        .filter(|frame| matches!(frame.body, Body::Turn { .. }))
        .count();
    let status = div()
        .flex()
        .flex_col()
        .gap(space::XS)
        .child(
            div()
                .flex()
                .flex_wrap()
                .items_center()
                .gap(space::SM)
                .when(!row.closed, |line| {
                    line.child(state_badge("thread-state", state, theme, ladder))
                })
                .child(
                    div()
                        .id("thread-asking")
                        .font_weight(FontWeight::MEDIUM)
                        .child(match row.closed {
                            true => "Closed".to_owned(),
                            false => asking(row.asking),
                        }),
                )
                .child(caption(
                    format!(
                        "{} · {} waiting · {} open calls",
                        design::plural(row.frames, "frame", "frames"),
                        row.held,
                        row.open_calls
                    ),
                    theme,
                )),
        )
        .child(
            div()
                .flex()
                .items_center()
                .gap(space::SM)
                .children(meter(row.context, shown.window, theme))
                .child(caption(
                    format!(
                        "Context {} · {}",
                        context_words(row.context, shown.window),
                        design::plural(summaries as u64, "summary", "summaries")
                    ),
                    theme,
                )),
        )
        .child(
            div()
                .id("thread-usage")
                .text_size(text::SECONDARY)
                .child(format!(
                    "This thread · {}",
                    usage_words(&read::usage(&shown.frames))
                )),
        )
        .when(turns_since > 0, |status| {
            status.child(caption(
                format!(
                    "{} since the last message from a person",
                    design::plural(turns_since as u64, "turn", "turns")
                ),
                theme,
            ))
        });
    div()
        .flex()
        .flex_col()
        .gap(space::BLOCK)
        .child(head)
        .children(closing(view, shown, cx, theme, ladder))
        .child(status)
        .children(no_node(
            "thread-no-node",
            &row.service,
            shown.serving,
            relaying,
            theme,
        ))
        .children(stuck(shown, theme))
        .children(held(shown, roster, theme))
        .child(transcript(view, shown, roster, cx, theme, ladder))
        .into_any_element()
}

/// Close's second press, and what it costs.
fn closing(
    view: &Agents,
    shown: &Transcript,
    cx: &mut Context<Agents>,
    theme: &Theme,
    ladder: &Ladder,
) -> Option<impl IntoElement + use<>> {
    let id = shown.thread.id;
    if view.arming != Some(Arm::Close(id)) {
        return None;
    }
    let confirm =
        cx.listener(move |view: &mut Agents, _: &ClickEvent, _, cx| view.act(Arm::Close(id), cx));
    let cancel = cx.listener(|view: &mut Agents, _: &ClickEvent, _, cx| {
        view.arming = None;
        cx.notify();
    });
    let mut cost =
        "Close this thread? The next mention opens a new thread with fresh context.".to_owned();
    if shown.thread.held > 0 {
        cost.push_str(&format!(
            " The {} waiting here do not carry over.",
            design::plural(shown.thread.held, "message", "messages")
        ));
    }
    Some(
        panel("thread-closing", theme.danger_soft, theme)
            .child(cost)
            .child(
                div()
                    .flex()
                    .gap(space::SM)
                    .child(design::button(
                        "thread-close-cancel",
                        "Cancel",
                        ladder,
                        theme,
                        cancel,
                    ))
                    .child(
                        design::button(
                            "thread-close-confirm",
                            "Close thread",
                            ladder,
                            theme,
                            confirm,
                        )
                        .kind(design::Kind::Primary)
                        .enabled(view.acting.is_none()),
                    ),
            ),
    )
}

/// A loop that still asks while input waits behind it: nothing recovers it today.
fn stuck(shown: &Transcript, theme: &Theme) -> Option<impl IntoElement + use<>> {
    let row = &shown.thread;
    if thread_state(row) != State::Stuck {
        return None;
    }
    Some(
        panel("thread-stuck", theme.warning_soft, theme)
            .child(
                div()
                    .font_weight(FontWeight::MEDIUM)
                    .child("This thread may be stuck"),
            )
            .child(format!(
                "It is still “{}” with {} waiting behind it. A harness upgrade or a frame that faults leaves a thread like this, and nothing recovers it today. Its manager can close it.",
                asking(row.asking),
                design::plural(row.held, "message", "messages"),
            ))
            .child(caption(
                format!(
                    "Last frame at {}, {} · block {}",
                    design::day(row.last_time),
                    design::clock(row.last_time),
                    design::grouped(row.last_height)
                ),
                theme,
            )),
    )
}

/// The input that arrived while the loop ran: it follows when the turn ends.
fn held(
    shown: &Transcript,
    roster: Option<&Roster>,
    theme: &Theme,
) -> Option<impl IntoElement + use<>> {
    if shown.held.is_empty() {
        return None;
    }
    let lines = shown.held.iter().enumerate().map(|(index, body)| {
        let who = match body {
            Body::Post {
                author: harness::Principal::Account(number),
                ..
            } => roster.map_or_else(|| format!("account {number}"), |r| r.name(*number)),
            Body::Notice { .. } => "Notice".to_owned(),
            _ => "harness".to_owned(),
        };
        div()
            .id(SharedString::from(format!("held-{index}")))
            .flex()
            .gap(space::SM)
            .child(div().font_weight(FontWeight::MEDIUM).child(who))
            .child(first_line(&body.text()))
    });
    Some(
        panel("thread-held", theme.surface, theme)
            .child(caption(
                format!(
                    "{} waiting · they follow when the turn ends",
                    shown.held.len()
                ),
                theme,
            ))
            .children(lines),
    )
}

fn first_line(text: &str) -> String {
    text.lines().next().unwrap_or_default().to_owned()
}

fn transcript(
    view: &Agents,
    shown: &Transcript,
    roster: Option<&Roster>,
    cx: &mut Context<Agents>,
    theme: &Theme,
    ladder: &Ladder,
) -> impl IntoElement + use<> {
    let agent = roster.map_or_else(
        || format!("account {}", shown.thread.agent),
        |roster| roster.name(shown.thread.agent),
    );
    div()
        .id("thread-transcript")
        .flex()
        .flex_col()
        .gap(space::LG)
        .children(
            shown
                .frames
                .iter()
                .map(|frame| frame_row(view, frame, shown, &agent, cx, theme, ladder)),
        )
}

fn frame_row(
    view: &Agents,
    frame: &Frame,
    shown: &Transcript,
    agent: &str,
    cx: &mut Context<Agents>,
    theme: &Theme,
    ladder: &Ladder,
) -> AnyElement {
    let roster = view.roster.ready();
    let seq = frame.seq;
    let meta = format!(
        "#{seq} · {} · block {}",
        design::clock(frame.time),
        design::grouped(frame.height)
    );
    let withheld = shown
        .withheld
        .iter()
        .find(|withheld| withheld.seq == seq)
        .map(|withheld| {
            design::badge(
                SharedString::from(format!("frame-{seq}-withheld")),
                format!(
                    "Read in parts · {}",
                    design::plural(withheld.size, "byte", "bytes")
                ),
                ladder,
                theme.agent,
                theme.agent_soft,
            )
        });
    let row = |who: String, what: String| {
        div()
            .id(SharedString::from(format!("frame-{seq}")))
            .flex()
            .flex_col()
            .gap(space::XS)
            .child(
                div()
                    .flex()
                    .flex_wrap()
                    .items_baseline()
                    .gap(space::SM)
                    .child(div().font_weight(FontWeight::MEDIUM).child(who))
                    .child(caption(what, theme))
                    .child(caption(meta.clone(), theme))
                    .children(withheld),
            )
    };
    match &frame.body {
        Body::Post { author, text } => {
            let who = match author {
                harness::Principal::Account(number) => {
                    roster.map_or_else(|| format!("account {number}"), |r| r.name(*number))
                }
                harness::Principal::Root => "the chain".to_owned(),
            };
            row(who, "wrote".into())
                .child(text.clone())
                .into_any_element()
        }
        Body::Turn {
            said, stop, usage, ..
        } => {
            let mut turn = row(agent.to_owned(), stop_words(*stop).into());
            for (index, said) in said.iter().enumerate() {
                let key = format!("thinking-{seq}-{index}");
                turn = turn.child(said_row(view, key, said, shown, cx, theme, ladder));
            }
            turn.child(caption(usage_words(usage), theme))
                .into_any_element()
        }
        Body::Result {
            call,
            tool,
            text,
            failed,
        } => {
            let what = match failed {
                true => format!("{tool} failed · {call}"),
                false => format!("{tool} result · {call}"),
            };
            row("Result".into(), what)
                .child(opened_text(
                    view,
                    format!("result-{seq}"),
                    text,
                    cx,
                    theme,
                    ladder,
                ))
                .into_any_element()
        }
        Body::Refused {
            call,
            tool,
            problem,
        } => row("Refused".into(), format!("{tool} · {call}"))
            .child(div().text_color(theme.danger).child(problem.clone()))
            .into_any_element(),
        Body::Notice { text } => row("Notice".into(), "harness gave up or noted".into())
            .p(space::LG)
            .bg(theme.danger_soft)
            .child(text.clone())
            .into_any_element(),
        Body::Compaction { .. } => row(
            "Summary".into(),
            "the model sees everything above only as this summary".into(),
        )
        .py(space::SM)
        .border_t_1()
        .border_b_1()
        .border_color(theme.border_strong)
        .child(opened_text(
            view,
            format!("summary-{seq}"),
            &frame.body.text(),
            cx,
            theme,
            ladder,
        ))
        .into_any_element(),
    }
}

fn stop_words(stop: Stop) -> &'static str {
    match stop {
        Stop::EndTurn => "answered",
        Stop::ToolUse => "asked for tool calls",
        Stop::MaxTokens => "hit max tokens",
        Stop::StopSequence => "hit a stop sequence",
        Stop::Pause => "paused",
        Stop::Refusal => "refused",
    }
}

fn said_row(
    view: &Agents,
    key: String,
    said: &Said,
    shown: &Transcript,
    cx: &mut Context<Agents>,
    theme: &Theme,
    ladder: &Ladder,
) -> AnyElement {
    match said {
        Said::Text(text) => div().child(text.clone()).into_any_element(),
        Said::Thinking { text, .. } => {
            opened_text(view, key, text, cx, theme, ladder).into_any_element()
        }
        Said::RedactedThinking(_) => caption("Thinking, redacted", theme).into_any_element(),
        Said::ToolUse { id, name, input } => {
            let call = shown.calls.iter().find(|call| &call.id == id);
            call_row(view, (id, name, input), call, cx, theme, ladder).into_any_element()
        }
    }
}

/// A tool call: its name, its input on one line and how far it got; opened, the whole
/// input and the blocks it ran at.
fn call_row(
    view: &Agents,
    (id, tool, input): (&str, &str, &str),
    call: Option<&CallRow>,
    cx: &mut Context<Agents>,
    theme: &Theme,
    ladder: &Ladder,
) -> impl IntoElement + use<> {
    let key = format!("call-{id}");
    let open = view.open.contains(&key);
    let (state, foreground, background) = match call.map(|call| &call.state) {
        None => ("Not placed", theme.muted, theme.surface_raised),
        Some(CallState::Relaying) => ("Relaying", theme.muted, theme.surface_raised),
        Some(CallState::Writing) => ("Writing", theme.warning, theme.warning_soft),
        Some(CallState::Done { failed: false, .. }) => ("Done", theme.success, theme.success_soft),
        Some(CallState::Done { failed: true, .. }) => ("Failed", theme.danger, theme.danger_soft),
    };
    let toggle = toggler(key.clone(), cx);
    let header = div()
        .id(SharedString::from(key.clone()))
        .flex()
        .items_center()
        .gap(space::SM)
        .px(space::SM)
        .py(space::XS)
        .cursor_pointer()
        .role(Role::Button)
        .aria_expanded(open)
        .focusable()
        .on_click(toggle)
        .child(
            div()
                .text_color(theme.faint)
                .child(if open { "▾" } else { "▸" }),
        )
        .child(design::mono(tool.to_owned()).font_weight(FontWeight::MEDIUM))
        .child(
            design::mono(first_line(input))
                .flex_1()
                .min_w(px(0.))
                .overflow_hidden()
                .text_color(theme.muted),
        )
        .child(design::badge(
            SharedString::from(format!("{key}-state")),
            state,
            ladder,
            foreground,
            background,
        ));
    let ran = call.map(|call| match call.state {
        CallState::Done { height, .. } => format!(
            "made at block {} · done at block {}",
            design::grouped(call.made),
            design::grouped(height)
        ),
        CallState::Relaying | CallState::Writing => {
            format!("made at block {}", design::grouped(call.made))
        }
    });
    div()
        .flex()
        .flex_col()
        .border_1()
        .border_color(theme.border)
        .child(header)
        .when(open, |shown| {
            shown.child(
                div()
                    .flex()
                    .flex_col()
                    .gap(space::XS)
                    .p(space::SM)
                    .border_t_1()
                    .border_color(theme.border)
                    .child(caption("Input", theme))
                    .child(wrapped_mono(input, theme))
                    .children(ran.map(|ran| caption(ran, theme))),
            )
        })
}

/// Long text behind a press: its size closed, the whole of it open.
fn opened_text(
    view: &Agents,
    key: String,
    text: &str,
    cx: &mut Context<Agents>,
    theme: &Theme,
    ladder: &Ladder,
) -> Div {
    let open = view.open.contains(&key);
    let size = format!(
        "{} · {}",
        design::plural(text.len() as u64, "byte", "bytes"),
        design::plural(text.lines().count() as u64, "line", "lines")
    );
    let label = match open {
        true => format!("Hide · {size}"),
        false => format!("Show · {size}"),
    };
    let press = design::button(
        SharedString::from(key.clone()),
        label,
        ladder,
        theme,
        toggler(key, cx),
    )
    .kind(design::Kind::Quiet);
    div()
        .flex()
        .flex_col()
        .gap(space::XS)
        .child(div().flex().child(press))
        .when(open, |shown| shown.child(wrapped_mono(text, theme)))
}

fn wrapped_mono(text: &str, theme: &Theme) -> Div {
    div()
        .p(space::SM)
        .bg(theme.surface)
        .font_family(fonts::FAMILY_MONO)
        .text_size(text::MONO)
        .child(text.to_owned())
}

fn toggler(
    key: String,
    cx: &mut Context<Agents>,
) -> impl Fn(&ClickEvent, &mut view::Window, &mut App) + 'static {
    cx.listener(move |view: &mut Agents, _: &ClickEvent, _, cx| {
        if !view.open.remove(&key) {
            view.open.insert(key.clone());
        }
        cx.notify();
    })
}
