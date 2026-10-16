//! The `harness` program: agents that consensus runs. An agent is a program account whose
//! script calls harness; its script's `config` is the path of its configuration in `files`
//! (`config.rs`), which harness reads at each model call. A mention or a direct message about
//! the agent reaches harness through `attribution`; the model is a record harness keeps
//! (`SetModel`), reached through the egress role in the wire it speaks (`wire/`). Every tool
//! call runs in a later block, as the agent.
mod agents;
pub mod config;
mod models;
mod program;
mod queries;
mod state;
#[cfg(test)]
mod tests;
mod tools;
mod turns;
mod wire;

pub use ::program::{BlobId, Principal};
pub use abi::role::identity::{AccountNumber, Standing};
pub use attribution::Change;
pub use config::{Config, Malformed};
pub use program::Harness;
pub use store::{PageRequest, PageResponse};

use borsh::{BorshDeserialize, BorshSerialize};

pub const PROGRAM: &str = "harness";
pub const PART: &str = "part";

#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub enum Op {
    Close {
        thread: u64,
    },
    Attributed(Box<Change>),
    SetModel {
        id: String,
        wire: Wire,
        service: String,
        name: String,
    },
    RemoveModel {
        id: String,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub enum Wire {
    AnthropicMessages,
}

impl Wire {
    pub const ALL: &'static [Wire] = &[Wire::AnthropicMessages];

    pub fn label(self) -> &'static str {
        match self {
            Wire::AnthropicMessages => "Anthropic Messages",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub enum Window {
    Unasked,
    Unreported,
    Tokens(u64),
}

impl Window {
    pub fn tokens(self) -> Option<u64> {
        match self {
            Window::Tokens(tokens) => Some(tokens),
            Window::Unasked | Window::Unreported => None,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct ModelRow {
    pub id: String,
    pub owner: AccountNumber,
    pub wire: Wire,
    pub service: String,
    pub name: String,
    pub window: Window,
}

#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct AgentSetup {
    pub agent: AccountNumber,
    pub manager: AccountNumber,
    pub standing: Standing,
    pub path: String,
    pub configured: Configured,
}

#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub enum Configured {
    Unreadable {
        reason: String,
    },
    Malformed {
        snapshot: BlobId,
        malformed: Malformed,
    },
    UnknownModel {
        snapshot: BlobId,
        config: Config,
    },
    Ready {
        snapshot: BlobId,
        config: Config,
        model: ModelRow,
    },
}

impl Configured {
    pub fn problem(&self, path: &str) -> Option<String> {
        match self {
            Configured::Unreadable { reason } => Some(format!(
                "the agent's configuration {path} could not be read: {reason}"
            )),
            Configured::Malformed { malformed, .. } => Some(format!(
                "the agent's configuration {path} is malformed: {malformed}"
            )),
            Configured::UnknownModel { config, .. } => Some(format!(
                "the agent's configuration {path} names model {}, which harness has no record of",
                config.model
            )),
            Configured::Ready { .. } => None,
        }
    }

    pub fn config(&self) -> Option<(BlobId, &Config)> {
        match self {
            Configured::UnknownModel { snapshot, config }
            | Configured::Ready {
                snapshot, config, ..
            } => Some((*snapshot, config)),
            Configured::Unreadable { .. } | Configured::Malformed { .. } => None,
        }
    }

    pub fn model(&self) -> Option<&ModelRow> {
        match self {
            Configured::Ready { model, .. } => Some(model),
            Configured::Unreadable { .. }
            | Configured::Malformed { .. }
            | Configured::UnknownModel { .. } => None,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct Frame {
    pub seq: u64,
    pub height: u64,
    pub time: u64,
    pub body: Body,
}

#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub enum Body {
    Post {
        author: Principal,
        text: String,
    },
    Turn {
        model: String,
        config: BlobId,
        said: Vec<Said>,
        stop: Stop,
        usage: Usage,
    },
    Refused {
        call: String,
        tool: String,
        problem: String,
    },
    Result {
        call: String,
        tool: String,
        text: String,
        failed: bool,
    },
    Notice {
        text: String,
    },
    Compaction {
        wire: Wire,
        block: String,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub enum Role {
    User,
    Assistant,
}

impl Body {
    pub fn role(&self) -> Option<Role> {
        match self {
            Body::Post { .. } | Body::Refused { .. } | Body::Result { .. } => Some(Role::User),
            Body::Turn { .. } => Some(Role::Assistant),
            Body::Notice { .. } | Body::Compaction { .. } => None,
        }
    }

    pub fn text(&self) -> String {
        match self {
            Body::Post { text, .. } | Body::Result { text, .. } | Body::Notice { text } => {
                text.clone()
            }
            Body::Refused { problem, .. } => problem.clone(),
            Body::Turn { said, .. } => said
                .iter()
                .filter_map(Said::text)
                .collect::<Vec<_>>()
                .join("\n"),
            Body::Compaction { wire, block } => wire::summary_words(*wire, block),
        }
    }
}

impl Said {
    fn text(&self) -> Option<String> {
        match self {
            Said::Text(text) | Said::Thinking { text, .. } => Some(text.clone()),
            Said::ToolUse { name, input, .. } => Some(format!("{name} {input}")),
            Said::RedactedThinking(_) => None,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub enum Said {
    Text(String),
    Thinking {
        text: String,
        signature: String,
    },
    RedactedThinking(String),
    ToolUse {
        id: String,
        name: String,
        input: String,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub enum Stop {
    EndTurn,
    ToolUse,
    MaxTokens,
    StopSequence,
    Pause,
    Refusal,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct Usage {
    pub input: u64,
    pub output: u64,
    pub cache_read: u64,
    pub cache_write: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub enum Asking {
    Idle,
    Learning,
    Compacting { through: u64 },
    Asked { upto: u64 },
    Running,
}

#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct ThreadRow {
    pub id: u64,
    pub agent: AccountNumber,
    pub manager: AccountNumber,
    pub place: String,
    pub title: String,
    pub opened: u64,
    pub closed: bool,
    pub frames: u64,
    pub last_input: u64,
    pub seen: u64,
    pub last_stop: Option<Stop>,
    pub asking: Asking,
    pub open_calls: u32,
    pub held: u64,
    pub sealed: u64,
    pub base: u64,
    pub tail_from: u64,
    pub summary: Option<u64>,
    pub context: Context,
    pub compact_before: Option<u64>,
    pub last_height: u64,
    pub last_time: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct Sealed {
    pub end: u64,
    pub role: Role,
    pub wire: Wire,
    pub part: BlobId,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub enum Context {
    Measured(u64),
    Overflowed,
}

impl ThreadRow {
    pub fn due(&self) -> bool {
        let unseen_input = self.last_input > self.seen;
        let paused = self.last_stop == Some(Stop::Pause);
        unseen_input || paused
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct WithheldFrame {
    pub seq: u64,
    pub size: u64,
}

#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub enum CallState {
    Relaying,
    Writing,
    Done { height: u64, failed: bool },
}

#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct CallRow {
    pub thread: u64,
    pub id: String,
    pub tool: String,
    pub input: String,
    pub made: u64,
    pub state: CallState,
}

#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct ToolInfo {
    pub name: String,
    pub description: String,
    pub schema: String,
    pub writes: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize, ::program::Ask)]
#[ask(Harness)]
pub enum Query {
    #[ask(Reply::Agent(Option<AgentSetup>))]
    Agent { agent: AccountNumber },
    #[ask(Reply::Thread(Option<ThreadRow>))]
    #[reads(state::THREADS)]
    Thread { thread: u64 },
    #[ask(Reply::Threads(PageResponse<ThreadRow>))]
    #[reads(state::BY_AGENT, state::THREADS)]
    Threads {
        agent: AccountNumber,
        page: PageRequest,
    },
    #[ask(Reply::Frames(PageResponse<Frame>))]
    #[reads(state::FRAMES)]
    Frames { thread: u64, page: PageRequest },
    #[ask(Reply::Held(Vec<Body>))]
    #[reads(state::HELD)]
    Held { thread: u64 },
    #[ask(Reply::Calls(PageResponse<CallRow>))]
    #[reads(state::CALLS)]
    Calls { thread: u64, page: PageRequest },
    #[ask(Reply::Tools(Vec<ToolInfo>))]
    Tools,
    #[ask(Reply::Thread(Option<ThreadRow>))]
    #[reads(state::PLACES, state::THREADS)]
    ThreadAt { agent: AccountNumber, place: String },
    #[ask(Reply::Model(Option<ModelRow>))]
    #[reads(state::MODELS)]
    Model { id: String },
    #[ask(Reply::Models(PageResponse<ModelRow>))]
    #[reads(state::MODELS)]
    Models { page: PageRequest },
    #[ask(Reply::Withheld(Vec<WithheldFrame>))]
    #[reads(state::WITHHELD, state::FRAMES)]
    Withheld { thread: u64 },
}

#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub enum Reply {
    Agent(Option<AgentSetup>),
    Thread(Option<ThreadRow>),
    Threads(PageResponse<ThreadRow>),
    Frames(PageResponse<Frame>),
    Held(Vec<Body>),
    Calls(PageResponse<CallRow>),
    Tools(Vec<ToolInfo>),
    Model(Option<ModelRow>),
    Models(PageResponse<ModelRow>),
    Withheld(Vec<WithheldFrame>),
}

pub fn attributed_as() -> Vec<u8> {
    vec![1]
}

pub fn describe(op: &Op) -> describe::Description {
    use describe::{Value, field};
    let (title, fields) = match op {
        Op::Close { thread } => (format!("Close thread {thread}"), Vec::new()),
        Op::Attributed(change) => (
            format!("Account {} is related to an object", change.recipient),
            vec![field("change", Value::count(change.seq))],
        ),
        Op::SetModel {
            id,
            wire,
            service,
            name,
        } => (
            format!("Set model · {id}"),
            vec![
                field("model", Value::text(id)),
                field("wire", Value::text(wire.label())),
                field("service", Value::text(service)),
                field("vendor's name", Value::text(name)),
            ],
        ),
        Op::RemoveModel { id } => (
            format!("Remove model · {id}"),
            vec![field("model", Value::text(id))],
        ),
    };
    describe::Description { title, fields }
}

describe::export!(Op, describe);

#[test]
fn op_variants_only_append() {
    assert_eq!(
        describe::variants::<Op>(),
        ["Close", "Attributed", "SetModel", "RemoveModel"]
    );
}

#[test]
fn query_variants_only_append() {
    assert_eq!(
        describe::variants::<Query>(),
        [
            "Agent", "Thread", "Threads", "Frames", "Held", "Calls", "Tools", "ThreadAt", "Model",
            "Models", "Withheld"
        ]
    );
}

#[test]
fn the_delivery_prefix_decodes_as_attributed() {
    let change = Change {
        seq: 7,
        source: attribution::Source {
            program: "chat".into(),
            object: attribution::Object {
                kind: "message".into(),
                id: "m1".into(),
            },
        },
        revision: 1,
        recipient: 4,
        reason: attribution::Reason::Mention,
        kind: attribution::Kind::Added,
        detail: Vec::new(),
        actor: attribution::Actor::Account(2),
        cause: attribution::Cause::Direct,
        height: 1,
    };
    let delivered = Change::payload(&attributed_as(), &change);
    assert_eq!(
        abi::decode::<Op>(&delivered).unwrap(),
        Op::Attributed(Box::new(change))
    );
}
