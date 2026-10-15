//! The `harness` program: agents that consensus runs. A manager hires an agent it handed to
//! harness; a mention or a direct message reaches it through `attribution`, the model is
//! called through `provider`, a thread is summarized before it outgrows the model's context
//! window, and every tool call runs in a later block, as the agent.
mod agents;
mod anthropic;
mod program;
mod queries;
mod state;
#[cfg(test)]
mod tests;
mod tools;
mod turns;

pub use ::program::{BlobId, Principal};
pub use abi::role::identity::AccountNumber;
pub use attribution::Change;
pub use program::Harness;
pub use store::{PageRequest, PageResponse};

use borsh::{BorshDeserialize, BorshSerialize};

pub const PROGRAM: &str = "harness";
pub const MODEL_SERVICE: &str = "anthropic";
pub const PART: &str = "part";

#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub enum Op {
    Hire {
        agent: AccountNumber,
        system: String,
        model: String,
        max_tokens: u32,
    },
    Dismiss {
        agent: AccountNumber,
    },
    Close {
        thread: u64,
    },
    Attributed(Box<Change>),
}

#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct AgentRow {
    pub agent: AccountNumber,
    pub manager: AccountNumber,
    pub name: String,
    pub system: String,
    pub model: String,
    pub max_tokens: u32,
    pub hired: u64,
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
    pub system: String,
    pub model: String,
    pub max_tokens: u32,
    pub opened: u64,
    pub closed: bool,
    pub frames: u64,
    pub last_input: u64,
    pub seen: u64,
    pub last_stop: Option<Stop>,
    pub asking: Asking,
    pub open_calls: u32,
    pub held: u64,
    pub header: BlobId,
    pub sealed: u64,
    pub tail_from: u64,
    pub summary: Option<u64>,
    pub context: Context,
    pub compact_before: Option<u64>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct Sealed {
    pub end: u64,
    pub role: Role,
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
    #[ask(Reply::Agent(Option<AgentRow>))]
    #[reads(state::AGENTS)]
    Agent { agent: AccountNumber },
    #[ask(Reply::Agents(PageResponse<AgentRow>))]
    #[reads(state::AGENTS)]
    Agents { page: PageRequest },
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
}

#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub enum Reply {
    Agent(Option<AgentRow>),
    Agents(PageResponse<AgentRow>),
    Thread(Option<ThreadRow>),
    Threads(PageResponse<ThreadRow>),
    Frames(PageResponse<Frame>),
    Held(Vec<Body>),
    Calls(PageResponse<CallRow>),
    Tools(Vec<ToolInfo>),
}

pub fn attributed_as() -> Vec<u8> {
    vec![3]
}

pub fn describe(op: &Op) -> describe::Description {
    use describe::{Value, field};
    let (title, fields) = match op {
        Op::Hire { agent, model, .. } => (
            "Hire an agent".to_owned(),
            vec![
                field("agent", Value::Account(*agent)),
                field("model", Value::text(model)),
            ],
        ),
        Op::Dismiss { agent } => (
            "Dismiss an agent".to_owned(),
            vec![field("agent", Value::Account(*agent))],
        ),
        Op::Close { thread } => (format!("Close thread {thread}"), Vec::new()),
        Op::Attributed(change) => (
            format!("Account {} is related to an object", change.recipient),
            vec![field("change", Value::count(change.seq))],
        ),
    };
    describe::Description { title, fields }
}

describe::export!(Op, describe);

#[test]
fn op_variants_only_append() {
    assert_eq!(
        describe::variants::<Op>(),
        ["Hire", "Dismiss", "Close", "Attributed"]
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
