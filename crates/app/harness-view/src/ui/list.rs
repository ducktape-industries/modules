//! The agents: the reader's, then the rest of the network's, each with its state.
use view::design::Ladder;
use view::design::{size, space, text};
use view::prelude::*;
use view::{FontWeight, Stateful};

use super::{agent_state, asking, caption, place_label, state_badge, waiting};
use crate::read::{Card, Roster, Unhired};
use crate::{Agents, Screen, State, thread_state};

pub(super) fn render(
    view: &Agents,
    cx: &mut Context<Agents>,
    theme: &Theme,
    ladder: &Ladder,
) -> AnyElement {
    let create = cx.listener(|view: &mut Agents, _: &ClickEvent, _, cx| view.start_create(cx));
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
                .child(design::heading("agents-title", "Agents", 1, theme))
                .child(caption(
                    "The agents harness runs. A mention or a direct message opens a thread for its channel.",
                    theme,
                )),
        )
        .children(view.roster.ready().map(|roster| {
            caption(
                design::plural(roster.relaying as u64, "relay node", "relay nodes"),
                theme,
            )
        }))
        .children(view.me.map(|_| {
            design::button("agents-create", "Create an agent…", ladder, theme, create)
                .kind(design::Kind::Primary)
        }));
    let body = match waiting("agents", &view.roster, cx, theme, ladder) {
        Some(shown) => shown,
        None => rows(view, cx, theme, ladder),
    };
    div()
        .flex()
        .flex_col()
        .gap(space::BLOCK)
        .child(head)
        .child(body)
        .into_any_element()
}

fn rows(view: &Agents, cx: &mut Context<Agents>, theme: &Theme, ladder: &Ladder) -> AnyElement {
    let Some(roster) = view.roster.ready() else {
        return div().into_any_element();
    };
    if roster.agents.is_empty() && roster.unhired.is_empty() {
        return design::empty_state(
            "agents-empty",
            "No agents yet",
            "Harness runs no agent on this network. Create one to answer in chat.",
            theme,
        )
        .into_any_element();
    }
    let (mine, others): (Vec<&Card>, Vec<&Card>) = roster
        .agents
        .iter()
        .partition(|card| Some(card.row.manager) == view.me);
    let mut list = div().flex().flex_col();
    if !mine.is_empty() || !roster.unhired.is_empty() {
        list = list
            .child(group("My agents", mine.len() + roster.unhired.len(), theme))
            .children(
                mine.iter()
                    .map(|card| row(card, roster, view.me, cx, theme, ladder)),
            )
            .children(
                roster
                    .unhired
                    .iter()
                    .map(|unhired| unhired_row(unhired, cx, theme, ladder)),
            );
    }
    if !others.is_empty() {
        list = list
            .child(group("Other agents", others.len(), theme))
            .children(
                others
                    .iter()
                    .map(|card| row(card, roster, view.me, cx, theme, ladder)),
            );
    }
    list.into_any_element()
}

fn group(label: &'static str, count: usize, theme: &Theme) -> impl IntoElement + use<> {
    div()
        .flex()
        .justify_between()
        .py(space::XS)
        .border_b_1()
        .border_color(theme.border)
        .text_size(text::CAPTION)
        .text_color(theme.muted)
        .font_weight(FontWeight::MEDIUM)
        .child(label)
        .child(count.to_string())
}

/// A row that opens what it names.
fn pressable(id: String, theme: &Theme) -> Stateful<view::Div> {
    let theme = *theme;
    div()
        .id(id)
        .flex()
        .flex_wrap()
        .items_center()
        .gap(space::LG)
        .py(space::SM)
        .border_b_1()
        .border_color(theme.border)
        .cursor_pointer()
        .hover(move |style| style.bg(theme.surface))
        .role(Role::Button)
        .focusable()
}

fn who(name: String, below: String, theme: &Theme) -> impl IntoElement + use<> {
    div()
        .flex()
        .items_center()
        .gap(space::SM)
        .w(px(180.))
        .child(
            design::avatar(&name, size::AVATAR_LG, theme)
                .bg(theme.agent_soft)
                .text_color(theme.agent),
        )
        .child(
            div()
                .flex()
                .flex_col()
                .child(div().font_weight(FontWeight::MEDIUM).child(name))
                .child(caption(below, theme)),
        )
}

fn row(
    card: &Card,
    roster: &Roster,
    me: Option<u64>,
    cx: &mut Context<Agents>,
    theme: &Theme,
    ladder: &Ladder,
) -> impl IntoElement + use<> {
    let agent = card.row.agent;
    let state = agent_state(card, roster);
    let open: Vec<_> = card
        .threads
        .iter()
        .filter(|thread| !thread.closed)
        .collect();
    let busiest = open
        .iter()
        .copied()
        .find(|thread| thread_state(thread) == State::Stuck)
        .or_else(|| {
            open.iter()
                .copied()
                .find(|thread| thread_state(thread) == State::Working)
        });
    let summary = match (state, busiest) {
        (State::NoNode, _) if card.serving == 0 => {
            format!("No member node serves {}", card.row.service)
        }
        (State::NoNode, _) => "No member node serves relay".to_owned(),
        (_, Some(thread)) => format!(
            "{} · {}",
            place_label(thread, Some(roster)),
            asking(thread.asking)
        ),
        (_, None) => design::plural(open.len() as u64, "open thread", "open threads"),
    };
    let held: u64 = open.iter().map(|thread| thread.held).sum();
    let manager = match Some(card.row.manager) == me {
        true => format!("{} (you)", roster.name(card.row.manager)),
        false => roster.name(card.row.manager),
    };
    let below = match card.note {
        Some(note) => format!("account {agent} · {note}"),
        None => format!("account {agent}"),
    };
    let closed = card.threads.len() - open.len();
    pressable(format!("agent-{agent}"), theme)
        .on_click(
            cx.listener(move |view: &mut Agents, _: &ClickEvent, _, cx| {
                view.go(Screen::Agent(agent), cx)
            }),
        )
        .child(who(card.row.name.clone(), below, theme))
        .child(div().w(px(140.)).child(manager))
        .child(design::mono(card.row.model.clone()).w(px(160.)))
        .child(
            div()
                .flex_1()
                .flex()
                .items_center()
                .gap(space::XS)
                .child(state_badge(
                    format!("agent-{agent}-state"),
                    state,
                    theme,
                    ladder,
                ))
                .child(caption(summary, theme)),
        )
        .child(caption(
            format!("{} open · {closed} closed", open.len()),
            theme,
        ))
        .child(caption(format!("{held} waiting"), theme))
}

/// An agent of the reader's handed to harness but not hired: Continue picks up the form.
fn unhired_row(
    unhired: &Unhired,
    cx: &mut Context<Agents>,
    theme: &Theme,
    ladder: &Ladder,
) -> impl IntoElement + use<> {
    let (agent, name) = (unhired.number, unhired.name.clone());
    let resume = name.clone();
    pressable(format!("unhired-{agent}"), theme)
        .on_click(
            cx.listener(move |view: &mut Agents, _: &ClickEvent, _, cx| {
                view.resume(agent, &resume, cx)
            }),
        )
        .child(who(name, format!("account {agent}"), theme))
        .child(design::badge(
            format!("unhired-{agent}-state"),
            "Not hired",
            ladder,
            theme.muted,
            theme.surface_raised,
        ))
        .child(caption(
            "Handed to harness, not hired yet. Continue to hire it.",
            theme,
        ))
}
