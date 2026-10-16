//! What both designs draw alike, once: an agent's face, and the three
//! answers an agent can take. Each design passes its own ladder; neither
//! design's tree reaches into the other's.
use view::design::{Face, Ladder};
use view::prelude::*;
use view::{Div, Stateful};

use crate::Settings;
use crate::queries::Agent;
use crate::state::Choice;
use crate::words;

/// An agent's initial, in the agent tint.
pub(crate) fn agent_face(agent: &Agent, face: impl Into<Face>, theme: &Theme) -> Div {
    design::avatar(&agent.name, face, theme)
        .bg(theme.agent_soft)
        .text_color(theme.agent)
}

/// Whom an agent answers, `shown` picked: one press sends Only me or
/// Anyone; Specific opens the people it lists.
pub(crate) fn audience_choices(
    agent: &Agent,
    shown: Choice,
    cx: &mut Context<Settings>,
    ladder: &Ladder,
    theme: &Theme,
) -> Stateful<Div> {
    let number = agent.number;
    design::segmented(
        format!("settings/agents/{number}/answers"),
        words::who_answers(&agent.name),
        ladder,
        theme,
        Choice::ALL.iter().position(|choice| *choice == shown),
        Choice::ALL.map(|choice| {
            (
                format!("settings/agents/{number}/answers/{}", choice.slug()).into(),
                choice.label().into(),
            )
        }),
        cx.processor(move |v: &mut Settings, index: usize, _, cx| {
            v.pick_audience(number, Choice::ALL[index], cx)
        }),
    )
}
