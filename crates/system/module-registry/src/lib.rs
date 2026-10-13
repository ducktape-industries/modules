//! The `module-registry` module: the roster of modules a network runs, and
//! the scheduled changes to it, made by the validators' vote
//! (`Env::authority`) or by a maintainer: an account the vote granted a
//! [`Scope`], whose change lands at least [`NOTICE`] blocks after it is
//! made, so the vote can cancel it first.
//! It fills the kernel's registry role: each block the kernel asks
//! `Query::At(height)` and runs what it answers. A client asks
//! `Query::Roster`: what runs and is listed at the query's own height. An
//! update is a `Change::Set` of published code under a running id: the
//! kernel swaps the code and keeps the module's state and account; `init`
//! does not run again.
//!
//! The types, rules and [`Modules`] module are always built; a view links
//! them with `module` off. The `module` feature adds its wasm exports.
mod program;
mod rules;
#[cfg(test)]
mod tests;

pub use program::Modules;
pub use rules::grantable;
pub use store::{PageRequest, PageResponse};

use abi::role::identity::{Kind, Profile};
use borsh::{BorshDeserialize, BorshSerialize};
use guest::{AccountNumber, ModuleId, Roles};

pub use abi::role::registry::{Entry, Genesis, View};

pub const MODULE: &str = "module-registry";

pub const CODE_KIND: &str = "program";

/// How many blocks a maintainer's change waits at least before it lands:
/// about a minute at 1 s blocks, for the validators to see it and cancel
/// it by vote. A constant: changing it is a registry upgrade, which only the
/// vote schedules.
pub const NOTICE: u64 = 60;

/// The most changes one account's grants keep waiting at once; the vote's
/// do not count. It bounds what a maintainer's change scans.
pub const MAX_PENDING: usize = 8;

/// The names a grant covers: one id, which may run nothing yet (how a team
/// adds a new module), or every id but those bound to a role the kernel calls.
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub enum Scope {
    Module(ModuleId),
    All,
}

impl Scope {
    pub fn includes(&self, program: &str) -> bool {
        match self {
            Scope::Module(id) => id == program,
            Scope::All => true,
        }
    }
}

impl std::fmt::Display for Scope {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Scope::Module(id) => f.write_str(id),
            Scope::All => f.write_str("every module"),
        }
    }
}

/// An account made a maintainer of `scope` by the validators' vote: its
/// signed frames schedule and cancel changes to the names in it.
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct Grant {
    pub scope: Scope,
    pub account: AccountNumber,
}

/// A change waiting in the schedule, and the account whose grant made it;
/// `None`: the validators' vote made it.
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct Pending {
    pub height: u64,
    pub change: Change,
    pub by: Option<AccountNumber>,
}

/// Whether `program` fills a role the kernel calls by its genesis binding:
/// only the vote changes it.
pub fn role_bound(roles: &Roles, program: &str) -> bool {
    [&roles.registry, &roles.validators, &roles.identity]
        .iter()
        .any(|role| role.as_str() == program)
}

/// Whether `grants` make `account` a maintainer of `program`: it holds
/// `Module(program)` or `All`, and `program` fills no role. What a view
/// asks before it offers a press; the rule reads the same off its table.
pub fn covers(grants: &[Grant], roles: &Roles, account: AccountNumber, program: &str) -> bool {
    !role_bound(roles, program)
        && grants
            .iter()
            .any(|grant| grant.account == account && grant.scope.includes(program))
}

/// Why the vote may not make `grant`, `None` when it may: a module id in
/// its one spelling and bound to no role, given to an account identity
/// profiles (`profile`) that is no program's. The rule and valset's first
/// vote both ask it, so a bad grant is refused before it is voted on.
pub fn grant_refused(grant: &Grant, roles: &Roles, profile: Option<&Profile>) -> Option<String> {
    if let Scope::Module(id) = &grant.scope {
        if !::program::is_name(id) {
            return Some(format!("{id:?} is not a program id: 1..=64 of [a-z0-9_-]"));
        }
        if role_bound(roles, id) {
            return Some(format!(
                "{id} fills a role the kernel calls: only the validators' vote changes it"
            ));
        }
    }
    let account = grant.account;
    match profile.map(|profile| &profile.kind) {
        None => Some(format!("there is no account {account}")),
        Some(Kind::Module(module)) => Some(format!(
            "account {account} is module {module}'s: a grant goes to a person or an agent"
        )),
        Some(Kind::Person | Kind::Managed { .. }) => None,
    }
}

#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub enum Change {
    Set(Entry),
    Remove(ModuleId),
    /// A view with no module behind it, listed under its name.
    SetView(View),
    RemoveView(ModuleId),
    SetHook(ModuleId),
    RemoveHook(ModuleId),
}

impl Change {
    /// What the change does, in words: `Set`, `Remove`, `Set view`, `Remove view`.
    pub fn verb(&self) -> &'static str {
        match self {
            Change::Set(_) => "Set",
            Change::Remove(_) => "Remove",
            Change::SetView(_) => "Set view",
            Change::RemoveView(_) => "Remove view",
            Change::SetHook(_) => "Set hook",
            Change::RemoveHook(_) => "Remove hook",
        }
    }

    /// The code a set lands; a removal has none.
    pub fn code(&self) -> Option<abi::BlobId> {
        match self {
            Change::Set(entry) => Some(entry.code),
            Change::SetView(view) => Some(view.view),
            Change::Remove(_)
            | Change::RemoveView(_)
            | Change::SetHook(_)
            | Change::RemoveHook(_) => None,
        }
    }

    pub fn program(&self) -> &str {
        match self {
            Change::Set(entry) => &entry.program,
            Change::SetView(view) => &view.name,
            Change::Remove(program)
            | Change::RemoveView(program)
            | Change::SetHook(program)
            | Change::RemoveHook(program) => program,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct Scheduled {
    pub height: u64,
    pub change: Change,
}

#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub enum Op {
    Publish {
        code: abi::BlobId,
    },
    Schedule(Scheduled),
    Cancel {
        height: u64,
        program: ModuleId,
    },
    /// The vote only.
    Grant(Grant),
    /// The vote, or the holder stepping down from its own grant.
    Revoke(Grant),
}

#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize, ::program::Ask)]
#[ask(Modules)]
pub enum Query {
    #[ask(Reply::Programs(Vec<Entry>))]
    At(u64),
    #[ask(Reply::Scheduled(PageResponse<Pending>))]
    Scheduled { page: PageRequest },
    #[ask(Reply::Program { height: u64, entry: Option<Entry> })]
    Program(ModuleId),
    /// The view-only entries at a height, by name.
    #[ask(Reply::Views(Vec<View>))]
    Views(u64),
    /// The programs genesis bound to the roles the kernel calls, as the
    /// kernel hands them to this module (`Env::roles`).
    #[ask(Reply::Roles(abi::Roles))]
    Roles,
    #[ask(Reply::Hooks(Vec<ModuleId>))]
    Hooks(u64),
    /// Every grant, by scope then account. Tag 6: tag 5 is the role's `Hooks`.
    #[ask(Reply::Grants(PageResponse<Grant>))]
    Grants { page: PageRequest },
    /// What runs and what is listed now: the programs and the view-only
    /// entries at the query's own height, by name, and that height. A change
    /// lands at its height whether or not an op has run here since.
    #[ask(Reply::Roster { height: u64, programs: Vec<Entry>, views: Vec<View> })]
    Roster,
}

#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub enum Reply {
    Programs(Vec<Entry>),
    Scheduled(PageResponse<Pending>),
    Program {
        height: u64,
        entry: Option<Entry>,
    },
    Views(Vec<View>),
    Roles(abi::Roles),
    Hooks(Vec<ModuleId>),
    Grants(PageResponse<Grant>),
    Roster {
        height: u64,
        programs: Vec<Entry>,
        views: Vec<View>,
    },
}

/// An op as a person reads it: a title and its fields. The source of the
/// `ducktape.describe` module this module ships (`make wasm-describes`).
pub fn describe(op: &Op) -> describe::Description {
    use describe::{Value, field};
    let height = |height: &u64| field("height", Value::Text(height.to_string()));
    let (title, fields) = match op {
        Op::Publish { code } => (
            "Publish".into(),
            vec![field("code", Value::Hash(code.digest().to_vec()))],
        ),
        Op::Schedule(Scheduled { height: at, change }) => {
            let mut fields = vec![
                field("change", Value::text(change.verb())),
                field("program", Value::Module(change.program().into())),
                height(at),
            ];
            fields.extend(
                change
                    .code()
                    .map(|code| field("code", Value::Hash(code.digest().to_vec()))),
            );
            if let Change::Set(entry) = change {
                fields.push(field("params", Value::bytes(&entry.params)));
            }
            (format!("Schedule · {}", change.program()), fields)
        }
        Op::Cancel {
            height: at,
            program,
        } => (
            format!("Cancel · {program}"),
            vec![field("program", Value::Module(program.clone())), height(at)],
        ),
        Op::Grant(grant) => (format!("Grant · {}", grant.scope), granted(grant)),
        Op::Revoke(grant) => (format!("Revoke · {}", grant.scope), granted(grant)),
    };
    describe::Description { title, fields }
}

fn granted(grant: &Grant) -> Vec<describe::Field> {
    use describe::{Value, field};
    let scope = match &grant.scope {
        Scope::Module(id) => Value::Module(id.clone()),
        Scope::All => Value::text("every module"),
    };
    vec![
        field("account", Value::Account(grant.account)),
        field("scope", scope),
    ]
}

describe::export!(Op, describe);

/// Old op bytes are described with the current code (`describe`): the op
/// enum only grows at its end. Append a new variant here; never reorder.
#[test]
fn op_variants_only_append() {
    assert_eq!(
        describe::variants::<Op>(),
        ["Publish", "Schedule", "Cancel", "Grant", "Revoke"]
    );
}
