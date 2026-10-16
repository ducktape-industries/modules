//! One agent: who it is, what it was hired with, who it answers, its threads by place and
//! what its turns used. Its manager hires it again from here, or dismisses it.
use identity::Audience;
use view::FontWeight;
use view::design::Ladder;
use view::design::{size, space, text};
use view::prelude::*;

use super::{
    agent_state, asking, back, caption, context_words, fact, no_node, panel, place_label,
    state_badge, usage_words, waiting,
};
use crate::read::{self, Roster};
use crate::{Agents, Arm, Screen, State, thread_state};

pub(super) fn render(
    view: &Agents,
    agent: u64,
    cx: &mut Context<Agents>,
    theme: &Theme,
    ladder: &Ladder,
) -> AnyElement {
    let back = back(
        "agent-back",
        "← Agents".into(),
        Screen::List,
        cx,
        theme,
        ladder,
    );
    let body = waiting("agent", &view.detail, cx, theme, ladder)
        .or_else(|| waiting("agent-roster", &view.roster, cx, theme, ladder))
        .unwrap_or_else(|| detail(view, agent, cx, theme, ladder));
    div()
        .flex()
        .flex_col()
        .gap(space::BLOCK)
        .child(div().flex().child(back))
        .child(body)
        .into_any_element()
}

fn detail(
    view: &Agents,
    agent: u64,
    cx: &mut Context<Agents>,
    theme: &Theme,
    ladder: &Ladder,
) -> AnyElement {
    let (Some(detail), Some(roster)) = (view.detail.ready(), view.roster.ready()) else {
        return div().into_any_element();
    };
    let Some(row) = detail.agent.clone() else {
        return design::empty_state(
            "agent-gone",
            "Not hired",
            format!("Harness runs no agent as account {agent}: its manager dismissed it, or never hired it."),
            theme,
        )
        .into_any_element();
    };
    let manages = view.me == Some(row.manager);
    let card = roster.card(agent);
    let state = card.map_or(State::Idle, |card| agent_state(card, roster));
    let serving = card.map_or(0, |card| card.serving);
    let note = card.and_then(|card| card.note);
    let edit_row = row.clone();
    let edit =
        cx.listener(move |view: &mut Agents, _: &ClickEvent, _, cx| view.edit(&edit_row, cx));
    let arm = cx.listener(move |view: &mut Agents, _: &ClickEvent, _, cx| {
        view.arming = Some(Arm::Dismiss(agent));
        cx.notify();
    });
    let mut sub = format!(
        "account {agent} · agent · managed by {} · hired at block {}",
        roster.name(row.manager),
        design::grouped(row.hired)
    );
    if let Some(note) = note {
        sub.push_str(&format!(" · {note}"));
    }
    let head = div()
        .flex()
        .flex_wrap()
        .items_center()
        .gap(space::LG)
        .child(
            design::avatar(&row.name, size::AVATAR_LG * 1.5, theme)
                .bg(theme.agent_soft)
                .text_color(theme.agent),
        )
        .child(
            div()
                .flex_1()
                .flex()
                .flex_col()
                .gap(space::HAIR)
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap(space::SM)
                        .child(design::heading("agent-name", row.name.clone(), 1, theme))
                        .child(state_badge("agent-state", state, theme, ladder)),
                )
                .child(caption(sub, theme)),
        )
        .when(manages, |head| {
            head.child(design::button("agent-edit", "Edit…", ladder, theme, edit))
                .child(design::button(
                    "agent-dismiss",
                    "Dismiss…",
                    ladder,
                    theme,
                    arm,
                ))
        });
    let writes = detail.tools.iter().filter(|tool| tool.writes).count();
    let facts = div()
        .flex()
        .flex_wrap()
        .gap(space::XL)
        .child(fact("Model", row.model.clone(), theme))
        .child(fact(
            "Service",
            format!("{} · {serving} serving", row.service),
            theme,
        ))
        .child(fact(
            "Max output",
            format!("{} tokens", design::grouped(row.max_tokens.into())),
            theme,
        ))
        .child(fact("Answers", answers(&detail.audience, roster), theme))
        .child(fact(
            "Tools",
            format!("{} · {writes} write as {}", detail.tools.len(), row.name),
            theme,
        ));
    let total = read::sum(detail.threads.iter().map(|(_, usage, _)| usage));
    div()
        .flex()
        .flex_col()
        .gap(space::BLOCK)
        .child(head)
        .children(dismissing(view, &row.name, agent, cx, theme, ladder))
        .child(facts)
        .children(no_node(
            "agent-no-node",
            &row.service,
            serving,
            roster.relaying,
            theme,
        ))
        .child(
            div()
                .flex()
                .flex_col()
                .gap(space::XS)
                .child(design::heading("agent-system", "System prompt", 2, theme))
                .child(
                    div()
                        .p(space::LG)
                        .border_1()
                        .border_color(theme.border)
                        .bg(theme.surface)
                        .child(match row.system.is_empty() {
                            true => "No system prompt: harness's preamble alone.".to_owned(),
                            false => row.system.clone(),
                        }),
                )
                .child(caption(
                    "Edits do not reach open threads: system, model, service and max_tokens are fixed when a thread opens.",
                    theme,
                )),
        )
        .child(threads(detail, roster, cx, theme, ladder))
        .child(
            div()
                .flex()
                .flex_col()
                .gap(space::XS)
                .child(design::heading("agent-usage", "Usage", 2, theme))
                .child(div().id("agent-usage-total").child(usage_words(&total)))
                .child(caption(
                    "Every turn's usage in every thread, added up. Tokens, not money.",
                    theme,
                )),
        )
        .into_any_element()
}

fn answers(audience: &Audience, roster: &Roster) -> String {
    match audience {
        Audience::Manager => "Its manager only".into(),
        Audience::Anyone => "Anyone".into(),
        Audience::Accounts(numbers) => {
            let names: Vec<String> = numbers.iter().map(|number| roster.name(*number)).collect();
            format!("Its manager, {}", names.join(", "))
        }
    }
}

/// Dismiss's second press, and what it costs.
fn dismissing(
    view: &Agents,
    name: &str,
    agent: u64,
    cx: &mut Context<Agents>,
    theme: &Theme,
    ladder: &Ladder,
) -> Option<impl IntoElement + use<>> {
    if view.arming != Some(Arm::Dismiss(agent)) {
        return None;
    }
    let confirm = cx.listener(move |view: &mut Agents, _: &ClickEvent, _, cx| {
        view.act(Arm::Dismiss(agent), cx)
    });
    let cancel = cx.listener(|view: &mut Agents, _: &ClickEvent, _, cx| {
        view.arming = None;
        cx.notify();
    });
    Some(
        panel("agent-dismissing", theme.danger_soft, theme)
            .child(format!(
                "Dismiss {name}? Harness stops answering for it. Its threads stay here to read."
            ))
            .child(
                div()
                    .flex()
                    .gap(space::SM)
                    .child(design::button(
                        "agent-dismiss-cancel",
                        "Cancel",
                        ladder,
                        theme,
                        cancel,
                    ))
                    .child(
                        design::button("agent-dismiss-confirm", "Dismiss", ladder, theme, confirm)
                            .kind(design::Kind::Primary)
                            .enabled(view.acting.is_none()),
                    ),
            ),
    )
}

fn threads(
    detail: &crate::read::Detail,
    roster: &Roster,
    cx: &mut Context<Agents>,
    theme: &Theme,
    ladder: &Ladder,
) -> impl IntoElement + use<> {
    let rows = detail.threads.iter().map(|(thread, usage, window)| {
        let id = thread.id;
        let state = match thread.closed {
            true => "Closed".to_owned(),
            false => asking(thread.asking),
        };
        let context = context_words(thread.context, *window);
        let used = usage.input + usage.output + usage.cache_read + usage.cache_write;
        let theme = *theme;
        div()
            .id(SharedString::from(format!("thread-{id}")))
            .flex()
            .flex_wrap()
            .items_center()
            .gap(space::LG)
            .py(space::SM)
            .border_b_1()
            .border_color(theme.border)
            .when(thread.closed, |row| row.text_color(theme.muted))
            .cursor_pointer()
            .hover(move |style| style.bg(theme.surface))
            .role(Role::Button)
            .focusable()
            .on_click(
                cx.listener(move |view: &mut Agents, _: &ClickEvent, _, cx| {
                    view.go(Screen::Thread(id), cx)
                }),
            )
            .child(
                div()
                    .w(px(160.))
                    .font_weight(FontWeight::MEDIUM)
                    .child(place_label(thread, Some(roster))),
            )
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(space::XS)
                    .w(px(220.))
                    .when(!thread.closed, |cell| {
                        cell.child(state_badge(
                            format!("thread-{id}-state"),
                            thread_state(thread),
                            &theme,
                            ladder,
                        ))
                    })
                    .child(caption(state, &theme)),
            )
            .child(caption(
                design::plural(thread.frames, "frame", "frames"),
                &theme,
            ))
            .child(caption(format!("context {context}"), &theme))
            .child(caption(
                format!("{} tokens used", design::grouped(used)),
                &theme,
            ))
            .child(caption(
                format!("opened at block {}", design::grouped(thread.opened)),
                &theme,
            ))
    });
    div()
        .flex()
        .flex_col()
        .gap(space::XS)
        .child(design::heading("agent-threads", "Threads", 2, theme))
        .child(caption(
            "One thread per place, so each channel has a context of its own.",
            theme,
        ))
        .children(rows)
        .when(detail.threads.is_empty(), |list| {
            list.child(design::quiet(
                "No thread yet: a mention or a direct message opens one.",
                theme,
            ))
        })
        .text_size(text::BODY)
}
