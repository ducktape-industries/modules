//! The `provider` program: a program's call to the world outside the chain, served by a member
//! node the chain elects, whose answer resolves the call in a later block.
mod program;
mod rules;
mod state;
#[cfg(test)]
mod tests;

pub mod http;

pub use ::program::{BlobId, ProgramId};
pub use program::Provider;
pub use store::{PageRequest, PageResponse};

use borsh::{BorshDeserialize, BorshSerialize};

pub const PROGRAM: &str = "provider";

pub const RELAY: &str = "relay";

pub const EXPIRY_BLOCKS: u64 = abi::role::awaits::EXPIRY_BLOCKS;

#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub enum Op {
    Serve { services: Vec<String> },
    Request(Request),
    Answer { id: u64, answer: Answer },
    Expire { id: u64 },
    Reassign { id: u64 },
}

#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct Request {
    pub service: String,
    pub input: Vec<u8>,
    pub parts: Vec<BlobId>,
}

#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub enum Answer {
    Output(Vec<u8>),
    Failed(String),
}

#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct RequestRow {
    pub id: u64,
    pub requester: ProgramId,
    pub service: String,
    pub input: Vec<u8>,
    pub parts: Vec<BlobId>,
    pub provider: Vec<u8>,
    pub made: u64,
    pub deadline: u64,
}

#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct ProviderRow {
    pub key: Vec<u8>,
    pub services: Vec<String>,
    pub since: u64,
}

#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize, ::program::Ask)]
#[ask(Provider)]
pub enum Query {
    #[ask(Reply::Requests(PageResponse<RequestRow>))]
    #[reads(state::ASSIGNED, state::REQUESTS)]
    Assigned { key: Vec<u8>, page: PageRequest },
    #[ask(Reply::Request(Option<RequestRow>))]
    #[reads(state::REQUESTS)]
    Request { id: u64 },
    #[ask(Reply::Requests(PageResponse<RequestRow>))]
    #[reads(state::DEADLINES, state::REQUESTS)]
    Overdue { page: PageRequest },
    #[ask(Reply::Providers(PageResponse<ProviderRow>))]
    #[reads(state::PROVIDERS)]
    Providers { page: PageRequest },
    #[ask(Reply::Serving(Vec<Vec<u8>>))]
    #[reads(state::SERVING)]
    Serving { service: String },
}

#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub enum Reply {
    Requests(PageResponse<RequestRow>),
    Request(Option<RequestRow>),
    Providers(PageResponse<ProviderRow>),
    Serving(Vec<Vec<u8>>),
}

pub fn describe(op: &Op) -> describe::Description {
    use describe::{Value, field};
    let (title, fields) = match op {
        Op::Serve { services } if services.is_empty() => ("Stop serving".to_owned(), Vec::new()),
        Op::Serve { services } => (
            "Serve".to_owned(),
            vec![field(
                "services",
                Value::List(services.iter().map(Value::text).collect()),
            )],
        ),
        Op::Request(request) => (
            format!("Request {}", request.service),
            vec![
                field("input", Value::count(request.input.len() as u64)),
                field("parts", Value::count(request.parts.len() as u64)),
            ],
        ),
        Op::Answer { id, answer } => {
            let field = match answer {
                Answer::Output(output) => field("output", Value::count(output.len() as u64)),
                Answer::Failed(reason) => field("failed", Value::text(reason)),
            };
            (format!("Answer request {id}"), vec![field])
        }
        Op::Expire { id } => (format!("Expire request {id}"), Vec::new()),
        Op::Reassign { id } => (format!("Reassign request {id}"), Vec::new()),
    };
    describe::Description { title, fields }
}

describe::export!(Op, describe);

#[test]
fn op_variants_only_append() {
    assert_eq!(
        describe::variants::<Op>(),
        ["Serve", "Request", "Answer", "Expire", "Reassign"]
    );
}
