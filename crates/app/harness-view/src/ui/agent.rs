//! One agent: who it is, its configuration file and what harness resolves from it (the
//! configuration and its model, or what is wrong), who it answers, its conversations and what
//! its turns used. Its manager edits its configuration, starts or forks a conversation, or
//! takes it back from harness.
use harness::{AgentSetup, Configured, Thinking};
use identity::{Ownership, Party};
use view::FontWeight;
use view::design::Ladder;
use view::design::{size, space, text};
use view::prelude::*;

use super::{
    agent_state, asking, back, began_words, caption, context_words, conversation_label, fact,
    no_node, panel, snapshot_words, state_badge, thinking_picker, thinking_words, usage_words,
    waiting, window_words,
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
    let Some(setup) = detail.setup.clone() else {
        return design::empty_state(
            "agent-gone",
            "bot-off",
            "Not run by harness",
            format!(
                "Account {agent} has no script that calls harness: its manager took it back, or never handed it over."
            ),
            ladder,
            theme,
        )
        .into_any_element();
    };
    let name = roster.name(agent);
    let manages = view.me == Some(setup.manager);
    let card = roster.card(agent);
    let state = card.map_or(State::Idle, |card| agent_state(card, roster));
    let note = card.and_then(|card| card.note);
    let edit_setup = setup.clone();
    let edit_name = name.clone();
    let edit = cx.listener(move |view: &mut Agents, _: &ClickEvent, _, cx| {
        view.edit(&edit_setup, &edit_name, cx)
    });
    let arm = cx.listener(move |view: &mut Agents, _: &ClickEvent, _, cx| {
        view.arming = Some(Arm::Remove(agent));
        cx.notify();
    });
    let mut sub = format!(
        "account {agent} · agent · managed by {}",
        roster.name(setup.manager)
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
            design::avatar(&name, size::AVATAR_LG * 1.5, theme)
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
                        .child(design::heading("agent-name", name.clone(), 1, theme))
                        .child(state_badge("agent-state", state, theme, ladder)),
                )
                .child(caption(sub, theme)),
        )
        .when(manages, |head| {
            head.child(design::button("agent-edit", "Edit…", ladder, theme, edit))
                .child(design::button(
                    "agent-remove",
                    "Take back…",
                    ladder,
                    theme,
                    arm,
                ))
        });
    let total = read::sum(detail.threads.iter().map(|(_, usage)| usage));
    let window = setup.configured.model().map(|model| model.window.tokens());
    div()
        .flex()
        .flex_col()
        .gap(space::BLOCK)
        .child(head)
        .children(removing(view, &name, agent, cx, theme, ladder))
        .child(configuration(&setup, detail, roster, &name, theme))
        .children(
            manages
                .then(|| thinking(view, &setup, &name, cx, theme, ladder))
                .flatten(),
        )
        .children(setup.configured.model().and_then(|model| {
            no_node(
                "agent-no-node",
                &model.service,
                card.and_then(|card| card.serving).unwrap_or_default(),
                roster.relaying,
                theme,
            )
        }))
        .child(system_prompt(&setup, theme))
        .child(threads(
            view,
            detail,
            roster,
            window.flatten(),
            cx,
            theme,
            ladder,
        ))
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

fn configuration(
    setup: &AgentSetup,
    detail: &read::Detail,
    roster: &Roster,
    name: &str,
    theme: &Theme,
) -> impl IntoElement + use<> {
    let writes = detail.tools.iter().filter(|tool| tool.writes).count();
    let serving = roster
        .card(setup.agent)
        .and_then(|card| card.serving)
        .unwrap_or_default();
    let mut facts = div()
        .flex()
        .flex_wrap()
        .gap(space::XL)
        .child(fact("Configuration", setup.path.clone(), theme))
        .children(detail.config_writers.as_ref().map(|writers| {
            fact(
                "Who may change it",
                config_writers(writers, &setup.path, roster),
                theme,
            )
        }));
    if let Some((snapshot, config)) = setup.configured.config() {
        let wait = config.wait.map_or_else(
            || "egress's default".to_owned(),
            |wait| design::plural(wait, "block", "blocks"),
        );
        facts = facts
            .child(fact("Read at", snapshot_words(&snapshot), theme))
            .child(fact("Model", config.model.clone(), theme))
            .child(fact(
                "Max output",
                format!("{} tokens", design::grouped(config.max_tokens.into())),
                theme,
            ))
            .child(fact("Wait", wait, theme))
            .child(fact("Thinking", thinking_words(config.thinking), theme));
    }
    if let Some(model) = setup.configured.model() {
        facts = facts
            .child(fact("Wire", model.wire.label(), theme))
            .child(fact(
                "Service",
                format!("{} · {serving} serving", model.service),
                theme,
            ))
            .child(fact("Vendor's name", model.name.clone(), theme))
            .child(fact("Window", window_words(model.window), theme));
    }
    facts = facts
        .child(fact("Answers", answers(&detail.ownership, roster), theme))
        .child(fact(
            "Tools",
            format!("{} · {writes} write as {name}", detail.tools.len()),
            theme,
        ));
    let problem = setup.configured.problem(&setup.path).map(|problem| {
        panel("agent-problem", theme.danger_soft, theme)
            .child(div().font_weight(FontWeight::MEDIUM).child(problem))
            .child(caption(
                "Harness says so in a thread once for each message that reaches the agent, and asks no model until the file is fixed.",
                theme,
            ))
    });
    div()
        .flex()
        .flex_col()
        .gap(space::SM)
        .child(design::heading(
            "agent-configuration",
            "Configuration",
            2,
            theme,
        ))
        .children(problem)
        .child(facts)
        .child(caption(
            "Harness reads the file at each model call, so an edit holds from the next call; each turn names the snapshot it read.",
            theme,
        ))
}

fn thinking(
    view: &Agents,
    setup: &AgentSetup,
    name: &str,
    cx: &mut Context<Agents>,
    theme: &Theme,
    ladder: &Ladder,
) -> Option<impl IntoElement + use<>> {
    let (_, config) = setup.configured.config()?;
    let rethinking = view.draft.rethink && view.draft.created == Some(setup.agent);
    let sending = rethinking && view.draft.sending.is_some();
    let picked = match sending {
        true => view.draft.thinking,
        false => config.thinking,
    };
    let problem = view.draft.problem.clone().filter(|_| rethinking);
    let (setup, name) = (setup.clone(), name.to_owned());
    let pick = cx.processor(
        move |view: &mut Agents, thinking: Option<Thinking>, _, cx| {
            view.rethink(&setup, &name, thinking, cx)
        },
    );
    Some(
        div()
            .flex()
            .flex_col()
            .gap(space::XS)
            .child(design::heading("agent-thinking", "Thinking", 2, theme))
            .child(div().flex().child(thinking_picker(
                "agent-thinking-level",
                picked,
                ladder,
                theme,
                pick,
            )))
            .children(problem.map(|problem| {
                panel("agent-thinking-problem", theme.danger_soft, theme).child(problem)
            }))
            .child(caption(
                "A press writes the configuration file again with this level; the next model call reads it.",
                theme,
            )),
    )
}

fn system_prompt(setup: &AgentSetup, theme: &Theme) -> impl IntoElement + use<> {
    let text = match &setup.configured {
        Configured::Ready { config, .. } | Configured::UnknownModel { config, .. } => {
            match config.system.is_empty() {
                true => {
                    "No system prompt: harness sends its ducktape resource manual alone.".to_owned()
                }
                false => config.system.clone(),
            }
        }
        Configured::Unreadable { .. } | Configured::Malformed { .. } => {
            "No system prompt is read until the file is fixed.".to_owned()
        }
    };
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
                .child(text),
        )
}

fn answers(ownership: &Ownership, roster: &Roster) -> String {
    match ownership {
        Ownership::Owner => "Its manager only".into(),
        Ownership::Anyone => "Anyone".into(),
        Ownership::Listed(parties) => format!("Its manager, {}", named_parties(parties, roster)),
    }
}

fn named_parties(parties: &[Party], roster: &Roster) -> String {
    let names: Vec<String> = parties
        .iter()
        .map(|party| match party {
            Party::Account(number) => roster.name(*number),
            Party::Program(program) => format!("the {program} program"),
        })
        .collect();
    names.join(", ")
}

fn config_writers(writers: &files::Writers, config: &str, roster: &Roster) -> String {
    let named = |number: Option<u64>| match number {
        Some(number) => roster.name(number),
        None => "no account".into(),
    };
    match writers {
        files::Writers::Listed {
            path,
            creator,
            write,
        } => {
            let who = match write {
                Ownership::Owner => format!("Only {}", named(*creator)),
                Ownership::Listed(parties) => {
                    format!("{}, {}", named(*creator), named_parties(parties, roster))
                }
                Ownership::Anyone => "Anyone".into(),
            };
            let own_list = path == config;
            match own_list {
                true => who,
                false => format!("{who}, by the list on {path}"),
            }
        }
        files::Writers::Anyone => "Anyone".into(),
    }
}

/// Take back's second press, and what it costs.
fn removing(
    view: &Agents,
    name: &str,
    agent: u64,
    cx: &mut Context<Agents>,
    theme: &Theme,
    ladder: &Ladder,
) -> Option<impl IntoElement + use<>> {
    if view.arming != Some(Arm::Remove(agent)) {
        return None;
    }
    let confirm = cx
        .listener(move |view: &mut Agents, _: &ClickEvent, _, cx| view.act(Arm::Remove(agent), cx));
    let cancel = cx.listener(|view: &mut Agents, _: &ClickEvent, _, cx| {
        view.arming = None;
        cx.notify();
    });
    Some(
        panel("agent-removing", theme.danger_soft, theme)
            .child(format!(
                "Take {name} back from harness? Its script is cleared, so harness stops answering for it. Its threads and its configuration file stay."
            ))
            .child(
                div()
                    .flex()
                    .gap(space::SM)
                    .child(design::button(
                        "agent-remove-cancel",
                        "Cancel",
                        ladder,
                        theme,
                        cancel,
                    ))
                    .child(
                        design::button("agent-remove-confirm", "Take back", ladder, theme, confirm)
                            .kind(design::Kind::Primary)
                            .enabled(view.acting.is_none()),
                    ),
            ),
    )
}

fn threads(
    view: &Agents,
    detail: &crate::read::Detail,
    roster: &Roster,
    window: Option<u64>,
    cx: &mut Context<Agents>,
    theme: &Theme,
    ladder: &Ladder,
) -> impl IntoElement + use<> {
    let current = detail.setup.as_ref().and_then(|setup| setup.current);
    let rows = detail.threads.iter().map(|(thread, usage)| {
        let id = thread.id;
        let is_current = current == Some(id);
        let began = began_words(thread, Some(roster));
        let state = match thread.closed {
            true => "Closed".to_owned(),
            false => asking(thread.asking),
        };
        let context = context_words(thread.context, window);
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
                    .w(px(220.))
                    .flex()
                    .flex_col()
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(space::XS)
                            .font_weight(FontWeight::MEDIUM)
                            .child(conversation_label(thread))
                            .when(is_current, |label| {
                                label.child(design::badge(
                                    SharedString::from(format!("thread-{id}-current")),
                                    "Current",
                                    ladder,
                                    theme.agent,
                                    theme.agent_soft,
                                ))
                            }),
                    )
                    .children(began.map(|began| caption(began, &theme))),
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
    let manages = detail
        .setup
        .as_ref()
        .is_some_and(|setup| view.me == Some(setup.manager));
    let agent = detail.setup.as_ref().map(|setup| setup.agent);
    let actions = agent.filter(|_| manages).map(|agent| {
        let renew = cx.listener(move |view: &mut Agents, _: &ClickEvent, _, cx| {
            view.converse(harness::Op::NewThread { agent }, cx)
        });
        let fork = cx.listener(move |view: &mut Agents, _: &ClickEvent, _, cx| {
            view.converse(harness::Op::ForkThread { agent }, cx)
        });
        let idle = view.acting.is_none();
        div()
            .flex()
            .gap(space::SM)
            .child(
                design::button("agent-new-thread", "New conversation", ladder, theme, renew)
                    .enabled(idle),
            )
            .child(
                design::button("agent-fork-thread", "Fork", ladder, theme, fork)
                    .enabled(idle && current.is_some()),
            )
    });
    div()
        .flex()
        .flex_col()
        .gap(space::XS)
        .child(
            div()
                .flex()
                .flex_wrap()
                .items_center()
                .justify_between()
                .gap(space::SM)
                .child(design::heading("agent-threads", "Conversations", 2, theme))
                .children(actions),
        )
        .child(caption(
            "One conversation at a time: every person and every place continue the current one, and each reply goes back where its message came from. New conversation starts from nothing; Fork starts from the current one's context. A conversation no longer current closes once it is idle, and stays readable.",
            theme,
        ))
        .children(rows)
        .when(detail.threads.is_empty(), |list| {
            list.child(design::quiet(
                "No conversation yet: a mention or a direct message opens one.",
                theme,
            ))
        })
        .text_size(text::BODY)
}
