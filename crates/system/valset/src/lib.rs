//! The `valset` module: who validates and who resides on a network. A key enrolls as a
//! resident, by invite or through an open door; the validators' vote changes the rest.
//! Types, rules and [`Valset`] always build; the `module` feature adds the wasm exports.
mod program;
mod rules;
#[cfg(test)]
mod tests;

pub use program::Valset;

use borsh::{BorshDeserialize, BorshSerialize};
pub use store::{PageRequest, PageResponse};

pub use abi::role::validators::{
    Genesis, Grant, INVITE_NAMESPACE, Invite, Member, Motion, Op, Voted,
};

pub const MODULE: &str = "valset";

#[derive(Clone, Copy, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub enum Role {
    Validator,
    Resident,
}

#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct Membership {
    pub key: Vec<u8>,
    pub address: String,
    pub role: Role,
}

impl Membership {
    pub fn member(&self) -> Member {
        Member {
            key: self.key.clone(),
            address: self.address.clone(),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize, ::program::Ask)]
#[ask(Valset)]
pub enum Query {
    #[ask(Reply::Validators(Vec<Vec<u8>>))]
    Validators,
    #[ask(Reply::Members(Vec<Member>))]
    Members,
    #[ask(Reply::Memberships(PageResponse<Membership>))]
    Memberships { page: PageRequest },
    #[ask(Reply::Membership(Option<Membership>))]
    Membership { key: Vec<u8> },
}

#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub enum Reply {
    Validators(Vec<Vec<u8>>),
    Members(Vec<Member>),
    Memberships(PageResponse<Membership>),
    Membership(Option<Membership>),
}

/// The ask another module makes of valset, at the program genesis bound to
/// the validators role.
pub fn role(ctx: &guest::QueryCtx, key: &[u8]) -> Result<Option<Role>, guest::Error> {
    let validators = &ctx.env().roles.validators;
    match ctx.query::<Query, Reply>(validators, &Query::Membership { key: key.to_vec() })? {
        Reply::Membership(membership) => Ok(membership.map(|membership| membership.role)),
        other => Err(guest::unexpected_reply(validators, "Membership", &other)),
    }
}

/// An op as a person reads it: a title and its fields. The source of the
/// `ducktape.describe` module this module ships (`make wasm-describes`).
pub fn describe(op: &Op) -> describe::Description {
    use describe::{Value, field};
    let (title, fields) = match op {
        Op::Enroll { address, invite } => (
            format!("Enroll · {address}"),
            vec![
                field("address", Value::text(address)),
                field(
                    "invite",
                    invite.as_ref().map_or(Value::text("none"), |invite| {
                        Value::Key(invite.issuer.clone())
                    }),
                ),
            ],
        ),
        Op::Leave => ("Leave".into(), vec![]),
        Op::Vote(motion) => {
            let (verb, fields) = match motion {
                Motion::Promote { key } => ("promote", vec![field("key", Value::Key(key.clone()))]),
                Motion::Demote { key } => ("demote", vec![field("key", Value::Key(key.clone()))]),
                Motion::Remove { key } => ("remove", vec![field("key", Value::Key(key.clone()))]),
                Motion::Door { open: true } => ("open the door", vec![]),
                Motion::Door { open: false } => ("close the door", vec![]),
                Motion::Registry { op } => ("registry", vec![field("op", Value::bytes(op))]),
            };
            (format!("Vote · {verb}"), fields)
        }
    };
    describe::Description { title, fields }
}

describe::export!(Op, describe);

/// Old op bytes are described with the current code (`describe`): the op
/// enum only grows at its end. Append a new variant here; never reorder.
#[test]
fn op_variants_only_append() {
    assert_eq!(describe::variants::<Op>(), ["Enroll", "Leave", "Vote"]);
}
