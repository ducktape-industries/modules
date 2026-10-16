//! The wires harness speaks to a model, one module each, reached through one `match` on
//! [`Wire`] per step: the turn request's parts, the reply read back as a turn, the model's
//! context window, and a thread's summary. What they read back is harness's own: a [`Reply`]
//! of [`Said`] blocks, a [`Summary`] the wire alone reads again.
pub(crate) mod anthropic;

use abi::role::egress::http::{Head, Response};

use crate::{Frame, Role, Said, Stop, Usage, Wire};

pub(crate) struct Header<'a> {
    pub name: &'a str,
    pub max_tokens: u32,
    pub system: &'a str,
}

pub(crate) struct Reply {
    pub model: String,
    pub said: Vec<Said>,
    pub stop: Stop,
    pub usage: Usage,
}

pub(crate) enum Answered {
    Reply(Reply),
    Overflow,
    Failed(String),
}

pub(crate) struct Summary {
    pub block: String,
    pub tokens: u64,
}

pub(crate) enum Summarized {
    Summary(Summary),
    Overflow,
    Failed(String),
}

pub(crate) fn header(wire: Wire, header: &Header) -> String {
    match wire {
        Wire::AnthropicMessages => anthropic::header(header),
    }
}

pub(crate) fn message(wire: Wire, role: Role, frames: &[Frame], first: bool, last: bool) -> String {
    match wire {
        Wire::AnthropicMessages => anthropic::message(role, frames, first, last),
    }
}

pub(crate) fn close(wire: Wire) -> &'static str {
    match wire {
        Wire::AnthropicMessages => anthropic::CLOSE,
    }
}

pub(crate) fn summarize(wire: Wire) -> &'static str {
    match wire {
        Wire::AnthropicMessages => anthropic::SUMMARIZE,
    }
}

pub(crate) fn summary(wire: Wire, written: Wire, block: &str, text: &str) -> String {
    let own = written == wire;
    match (wire, own) {
        (Wire::AnthropicMessages, true) => anthropic::summary_message(block),
        (Wire::AnthropicMessages, false) => anthropic::summary_text(text),
    }
}

pub(crate) fn summary_words(written: Wire, block: &str) -> String {
    match written {
        Wire::AnthropicMessages => anthropic::summary_words(block),
    }
}

pub(crate) fn turn_head(wire: Wire, summarized: bool) -> Head {
    match wire {
        Wire::AnthropicMessages => anthropic::head(summarized),
    }
}

pub(crate) fn summary_head(wire: Wire) -> Head {
    match wire {
        Wire::AnthropicMessages => anthropic::head(true),
    }
}

pub(crate) fn model_head(wire: Wire, name: &str) -> Head {
    match wire {
        Wire::AnthropicMessages => anthropic::model(name),
    }
}

pub(crate) fn answered(wire: Wire, response: &Response) -> Answered {
    match wire {
        Wire::AnthropicMessages => anthropic::answered(response),
    }
}

pub(crate) fn summarized(wire: Wire, response: &Response) -> Summarized {
    match wire {
        Wire::AnthropicMessages => anthropic::summarized(response),
    }
}

pub(crate) fn window(wire: Wire, response: &Response) -> Result<Option<u64>, String> {
    match wire {
        Wire::AnthropicMessages => anthropic::window(response),
    }
}
