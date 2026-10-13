//! The `attribution` module: which accounts each module's objects relate to (a mention,
//! an author, an owner, an assignee), every change to those relations, and the modules
//! told of each change as it happens.
mod program;
mod rules;
mod state;
#[cfg(test)]
mod tests;

pub use guest::{AccountNumber, Cause, ModuleId};
pub use program::Attribution;
pub use store::{PageRequest, PageResponse};

use borsh::{BorshDeserialize, BorshSerialize};

pub const MODULE: &str = "attribution";

pub const MAX_NAME_BYTES: usize = 256;
pub const MAX_RELATIONS: usize = 256;
pub const MAX_DETAIL_BYTES: usize = 4 << 10;
pub const MAX_BATCH: usize = 64;
pub const MAX_SUBSCRIBERS: usize = 32;
pub const MAX_DELIVER_BYTES: usize = 256;

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, BorshSerialize, BorshDeserialize)]
pub struct Object {
    pub kind: String,
    pub id: String,
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, BorshSerialize, BorshDeserialize)]
pub struct Source {
    pub program: ModuleId,
    pub object: Object,
}

#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub enum Actor {
    Account(AccountNumber),
    Key(Vec<u8>),
    Program(ModuleId),
    System,
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, BorshSerialize, BorshDeserialize)]
pub enum Reason {
    Mention,
    Authorship,
    Ownership,
    Assignment,
    Credit,
    Result,
    Report,
    Defined(String),
}

#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct Relation {
    pub recipient: AccountNumber,
    pub reason: Reason,
    pub detail: Vec<u8>,
}

#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct Transfer {
    pub reason: Reason,
    pub from: AccountNumber,
    pub to: AccountNumber,
}

/// Every relation `object` holds as of `revision`, which only grows: relations it
/// held before and not now are withdrawn.
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct Update {
    pub object: Object,
    pub revision: u64,
    pub actor: Actor,
    pub relations: Vec<Relation>,
    pub transfers: Vec<Transfer>,
}

#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub enum Kind {
    Added,
    Withdrawn,
    TransferredIn { from: AccountNumber },
    TransferredOut { to: AccountNumber },
}

#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct Change {
    pub seq: u64,
    pub source: Source,
    pub revision: u64,
    pub recipient: AccountNumber,
    pub reason: Reason,
    pub kind: Kind,
    pub detail: Vec<u8>,
    pub actor: Actor,
    pub cause: Cause,
    pub height: u64,
}

#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct Relations {
    pub source: Source,
    pub revision: u64,
    pub relations: Vec<Relation>,
}

/// `Attribute` and `AttributeBatch` come from the module whose objects they name;
/// `Subscribe` from the module to be told, as `deliver` followed by the borsh `Change`.
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub enum Op {
    Attribute(Update),
    AttributeBatch { updates: Vec<Update> },
    Subscribe { deliver: Vec<u8> },
}

#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize, ::program::Ask)]
#[ask(Attribution)]
pub enum Query {
    #[ask(Reply::Relations(Option<Relations>))]
    #[reads(state::OBJECTS)]
    Relations { source: Source },
    #[ask(Reply::Changes(PageResponse<Change>))]
    #[reads(state::CHANGES)]
    Changes { page: PageRequest },
    #[ask(Reply::Changes(PageResponse<Change>))]
    #[reads(state::TO, state::CHANGES)]
    ChangesTo {
        recipient: AccountNumber,
        page: PageRequest,
    },
    #[ask(Reply::Changes(PageResponse<Change>))]
    #[reads(state::OF, state::CHANGES)]
    ChangesOf { source: Source, page: PageRequest },
    #[ask(Reply::Subscribers(Vec<ModuleId>))]
    #[reads(state::SUBSCRIBERS)]
    Subscribers,
}

#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub enum Reply {
    Relations(Option<Relations>),
    Changes(PageResponse<Change>),
    Subscribers(Vec<ModuleId>),
}

impl Change {
    pub fn payload(deliver: &[u8], change: &Change) -> Vec<u8> {
        let mut payload = deliver.to_vec();
        payload.extend(abi::encode(change));
        payload
    }
}

pub fn describe(op: &Op) -> describe::Description {
    use describe::{Value, field};
    let object = |update: &Update| {
        field(
            "object",
            Value::text(format!("{}/{}", update.object.kind, update.object.id)),
        )
    };
    let recipients = |update: &Update| {
        field(
            "recipients",
            Value::List(
                update
                    .relations
                    .iter()
                    .map(|relation| Value::Account(relation.recipient))
                    .collect(),
            ),
        )
    };
    let (title, fields) = match op {
        Op::Attribute(update) => (
            format!("Attribute revision {}", update.revision),
            vec![object(update), recipients(update)],
        ),
        Op::AttributeBatch { updates } => (
            format!("Attribute {} objects", updates.len()),
            updates.iter().map(object).collect(),
        ),
        Op::Subscribe { .. } => ("Subscribe".to_owned(), Vec::new()),
    };
    describe::Description { title, fields }
}

describe::export!(Op, describe);

#[test]
fn op_variants_only_append() {
    assert_eq!(
        describe::variants::<Op>(),
        ["Attribute", "AttributeBatch", "Subscribe"]
    );
}
