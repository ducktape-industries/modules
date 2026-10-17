//! The `access` program: who may write each key another program names. A program answers the
//! kernel what an op needs and who owns a key; this program keeps each key's write list, and the
//! kernel asks it, before every op runs, whether the op's actor may. Types, rules and [`Access`]
//! always build; `program` adds the wasm exports.
mod program;
mod rules;
#[cfg(test)]
mod tests;

use ::program::{Need, Ownership, Party, ProgramId, Resource, Writers};
use borsh::{BorshDeserialize, BorshSchema, BorshSerialize};
pub use program::Access;
pub use store::{PageRequest, PageResponse};

pub const PROGRAM: &str = "access";

#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize, BorshSchema)]
pub enum Op {
    Check {
        program: ProgramId,
        actor: Vec<Party>,
        needs: Vec<Need>,
    },
    Set {
        program: ProgramId,
        key: String,
        list: Option<Ownership>,
    },
    Move {
        program: ProgramId,
        from: String,
        to: String,
    },
}

#[derive(
    Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize, BorshSchema, ::program::Ask,
)]
#[ask(Access)]
pub enum Query {
    #[ask(Reply::List(Option<Ownership>))]
    List { program: ProgramId, key: String },
    #[ask(Reply::Resource(Resource))]
    Resource { program: ProgramId, key: String },
    #[ask(Reply::Writers(Writers))]
    Writers { program: ProgramId, key: String },
    #[ask(Reply::Lists(PageResponse<Listed>))]
    Lists {
        program: ProgramId,
        page: PageRequest,
    },
}

#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize, BorshSchema)]
pub struct Listed {
    pub key: String,
    pub list: Ownership,
}

#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize, BorshSchema)]
pub enum Reply {
    List(Option<Ownership>),
    Resource(Resource),
    Writers(Writers),
    Lists(PageResponse<Listed>),
}

pub fn describe(op: &Op) -> describe::Description {
    use describe::{Value, field};
    let (title, fields) = match op {
        Op::Check { program, needs, .. } => (
            format!("Check · {program}"),
            vec![field("needs", Value::count(needs.len() as u64))],
        ),
        Op::Set { program, key, list } => (
            format!("Write list · {program} {key}"),
            vec![
                field("program", Value::Program(program.clone())),
                field("key", Value::text(key)),
                field("list", listed(list.as_ref())),
            ],
        ),
        Op::Move { program, from, to } => (
            format!("Move lists · {program} {from}"),
            vec![
                field("program", Value::Program(program.clone())),
                field("from", Value::text(from)),
                field("to", Value::text(to)),
            ],
        ),
    };
    describe::Description { title, fields }
}

fn listed(list: Option<&Ownership>) -> describe::Value {
    use describe::Value;
    match list {
        None => Value::text("cleared"),
        Some(Ownership::Owner) => Value::text("its owner"),
        Some(Ownership::Anyone) => Value::text("anyone"),
        Some(Ownership::Listed(parties)) => Value::List(
            std::iter::once(Value::text("its owner"))
                .chain(parties.iter().map(|party| match party {
                    Party::Account(number) => Value::Account(*number),
                    Party::Program(program) => Value::Program(program.clone()),
                }))
                .collect(),
        ),
    }
}

describe::export!(Op, describe);

#[test]
fn op_variants_only_append() {
    assert_eq!(describe::variants::<Op>(), ["Check", "Set", "Move"]);
}
