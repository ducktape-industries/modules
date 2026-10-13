//! The `harness` module: agents that consensus runs. A manager hires an agent it handed to
//! harness; a mention or a direct message reaches it through `attribution`, the model is
//! asked through `provider`, and every tool call runs in a later block, as the agent.
mod agents;
mod anthropic;
mod program;
mod queries;
mod state;
#[cfg(test)]
mod tests;
mod tools;
mod turns;

pub use abi::role::identity::AccountNumber;
pub use attribution::Change;
pub use guest::{BlobId, Principal};
pub use program::Harness;
pub use provider::Delivery;
pub use store::{PageRequest, PageResponse};

use borsh::{BorshDeserialize, BorshSerialize};

pub const MODULE: &str = "harness";
pub const MODEL_SERVICE: &str = "anthropic";
pub const PART: &str = "part";

pub const MAX_SYSTEM_BYTES: usize = 32 << 10;
pub const MAX_MODEL_BYTES: usize = 128;
pub const MAX_OUTPUT_TOKENS: u32 = 64_000;
pub const MAX_POST_BYTES: usize = 32 << 10;
pub const MAX_INPUT_BYTES: usize = 256 << 10;
pub const MAX_RESULT_BYTES: usize = 32 << 10;
pub const MAX_CALL_ID_BYTES: usize = 64;
pub const MAX_MESSAGES: u64 = 2048;
pub const MAX_ATTEMPTS: u32 = 3;
pub const MAX_TURNS_PER_POST: u32 = 32;

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
    Turned {
        thread: u64,
        delivery: Delivery,
    },
    Relayed {
        thread: u64,
        call: String,
        delivery: Delivery,
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
    Receipt {
        call: String,
        tool: String,
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
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Role {
    User,
    Assistant,
}

impl Body {
    pub fn role(&self) -> Option<Role> {
        match self {
            Body::Post { .. }
            | Body::Receipt { .. }
            | Body::Refused { .. }
            | Body::Result { .. } => Some(Role::User),
            Body::Turn { .. } => Some(Role::Assistant),
            Body::Notice { .. } => None,
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
    Asked { upto: u64 },
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
    pub failures: u32,
    pub turns_since_post: u32,
    pub open_calls: u32,
    pub held: u64,
    pub header: BlobId,
    pub sealed: u64,
    pub tail_from: u64,
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
    Relaying { attempts: u32 },
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

pub fn turned_as(thread: u64) -> Vec<u8> {
    let mut prefix = vec![3];
    prefix.extend(abi::encode(&thread));
    prefix
}

pub fn attributed_as() -> Vec<u8> {
    vec![5]
}

pub fn relayed_as(thread: u64, call: &str) -> Vec<u8> {
    let mut prefix = vec![4];
    prefix.extend(abi::encode(&thread));
    prefix.extend(abi::encode(&call.to_owned()));
    prefix
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
        Op::Turned { thread, delivery } => (
            format!("Model answer for thread {thread}"),
            vec![field("request", Value::count(delivery.id))],
        ),
        Op::Relayed {
            thread,
            call,
            delivery,
        } => (
            format!("Run tool call {call} of thread {thread}"),
            vec![field("request", Value::count(delivery.id))],
        ),
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
        [
            "Hire",
            "Dismiss",
            "Close",
            "Turned",
            "Relayed",
            "Attributed"
        ]
    );
}

#[test]
fn a_delivery_prefix_decodes_as_the_op_it_names() {
    let delivery = Delivery {
        id: 7,
        service: MODEL_SERVICE.into(),
        outcome: provider::Outcome::Expired,
    };
    let turned = provider::Delivery::payload(&turned_as(5), &delivery);
    assert_eq!(
        abi::decode::<Op>(&turned).unwrap(),
        Op::Turned {
            thread: 5,
            delivery: delivery.clone()
        }
    );
    let relayed = provider::Delivery::payload(&relayed_as(5, "toolu_1"), &delivery);
    assert_eq!(
        abi::decode::<Op>(&relayed).unwrap(),
        Op::Relayed {
            thread: 5,
            call: "toolu_1".into(),
            delivery
        }
    );
}
