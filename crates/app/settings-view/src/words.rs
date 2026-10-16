//! The words Account shows, once for both designs, as the Expo app keeps
//! them in `src/account/words.ts`: what an account is, a key's name, whom
//! an agent answers, an agent's standing, why a form did not go, the
//! network's name, and every sentence both designs show. Pure: nothing here
//! reads the state or writes.
use identity::{Audience, Kind, Standing};
use view::methods::Session;

use crate::state::Problem;

/// A one-field form's words, past its field's name: what the field asks
/// for while empty, what it says when it was sent empty, its verb, and the
/// verb while the submit is in flight.
pub(crate) struct FormWords {
    pub(crate) prompt: &'static str,
    pub(crate) empty: &'static str,
    pub(crate) verb: &'static str,
    pub(crate) busy: &'static str,
}

pub(crate) const ACCOUNT_NAME: &str = "Account name";
pub(crate) const CREATE_ACCOUNT: FormWords = FormWords {
    prompt: "Name the new account",
    empty: "Enter an account name.",
    verb: "Create account",
    busy: "Creating…",
};

pub(crate) const AGENT_NAME: &str = "Agent name";
pub(crate) const CREATE_AGENT: FormWords = FormWords {
    prompt: "Name the new agent",
    empty: "Enter an agent name.",
    verb: "Create agent",
    busy: "Creating…",
};

pub(crate) const KEY_REQUEST: &str = "Agent key request";
pub(crate) const ADD_KEY: FormWords = FormWords {
    prompt: "Paste an agent's key request",
    // an empty paste is not a key request: `Problem::NotAKeyRequest` says so
    empty: "",
    verb: "Add key",
    busy: "Adding…",
};

/// The rename field's name: `Rename Scout`.
pub(crate) fn rename_field(agent: &str) -> String {
    format!("Rename {agent}")
}
pub(crate) const RENAME: FormWords = FormWords {
    prompt: "New name",
    empty: "Enter the agent's new name.",
    verb: "Save",
    busy: "Renaming…",
};

/// The field that adds a person an agent answers: `Add someone Scout answers`.
pub(crate) fn person_field(agent: &str) -> String {
    format!("Add someone {agent} answers")
}
pub(crate) const ADD_PERSON: FormWords = FormWords {
    prompt: "Name or account number",
    empty: "Enter a name or an account number.",
    verb: "Add",
    busy: "Adding…",
};

pub(crate) const READING: &str = "Reading your account…";
pub(crate) const NO_ACCOUNT: &str = "No account";
pub(crate) const NO_KEY: &str = "No host key is selected. Sign in with a key to create an account.";
pub(crate) const UNREGISTERED: &str = "Unregistered key";
pub(crate) const NO_ACCOUNT_YET: &str = "no account yet";
/// Under a stopped agent's name, where an account's line would be.
pub(crate) const AGENT_KEY: &str = "a key of an agent";
pub(crate) const KEYS: &str = "Keys";
pub(crate) const VALIDATOR: &str = "Validator";
pub(crate) const CREATE_ACCOUNT_TITLE: &str = "Create an account";
pub(crate) const CREATE_ACCOUNT_WHY: &str =
    "Your key isn't linked to an account yet. An account gives you a name others see.";
pub(crate) const AGENTS_HELP: &str =
    "Agents act as accounts you manage. You answer for what they do.";
pub(crate) const CREATE_AGENT_TITLE: &str = "Create an agent";
pub(crate) const ADD_KEY_TITLE: &str = "Add a key to an agent";
pub(crate) const ADD_KEY_WHY: &str = "Paste the key request the agent's device shows.";
pub(crate) const WHO_IT_ANSWERS: &str = "Who it answers";
pub(crate) const ANYONE_WARNING: &str = "Anyone on the network can tell this agent what to do.";
pub(crate) const NOBODY_LISTED: &str = "Add people by name or account number.";
/// A standing change in flight.
pub(crate) const WORKING: &str = "Working…";
/// A write of whom an agent answers, or (on the phone) any write, in flight.
pub(crate) const SAVING: &str = "Saving…";
/// A write whose answer was lost.
pub(crate) const IN_DOUBT: &str = "No answer came back: it may have gone through.";
/// The head over the ways to add an agent or a key.
pub(crate) const ADD: &str = "Add";
pub(crate) const CANCEL: &str = "Cancel";
/// The press that opens an agent's rename.
pub(crate) const RENAME_AGENT: &str = "Rename";
pub(crate) const REVOKE: &str = "Revoke";
/// The press that takes a person off whom an agent answers.
pub(crate) const REMOVE: &str = "Remove";

/// The choice of whom an agent answers, as a control names it.
pub(crate) fn who_answers(agent: &str) -> String {
    format!("Who {agent} answers")
}

/// An account by its number: `account 7`.
pub(crate) fn account(number: u64) -> String {
    format!("account {number}")
}

/// An agent's number and how many keys it holds: `account 12 · 0 keys`.
pub(crate) fn account_keys(number: u64, keys: usize) -> String {
    format!("{} · {}", account(number), keys_count(keys))
}

/// `1 key`, `5 keys`.
pub(crate) fn keys_count(keys: usize) -> String {
    view::design::plural(keys as u64, "key", "keys")
}

/// What identity says an account is, as the line under its name says it
/// (Expo `what`).
pub(crate) fn what(kind: &Kind) -> String {
    match kind {
        Kind::User => "Person".into(),
        Kind::Program { .. } => "Agent".into(),
        Kind::System(program) => format!("Program · {program}"),
    }
}

/// A key's name: its label, else its place on the account (Expo
/// `keyLabel`).
pub(crate) fn key_label(label: Option<&str>, index: usize) -> String {
    match label {
        Some(label) => label.to_owned(),
        None => format!("Device {}", index + 1),
    }
}

/// Whom an agent answers, as its row says it (Expo `answers`).
pub(crate) fn answers(audience: &Audience) -> String {
    match audience {
        Audience::Manager => "only me".into(),
        Audience::Anyone => "anyone".into(),
        Audience::Accounts(people) => view::design::plural(people.len() as u64, "person", "people"),
    }
}

/// An agent's standing, as its badge says it.
pub(crate) fn standing(standing: Standing) -> &'static str {
    match standing {
        Standing::Active => "active",
        Standing::Suspended => "suspended",
        Standing::Revoked => "revoked",
    }
}

/// What stopped a form, as the form words it; `empty` says what to type
/// (Expo `PROBLEM`).
pub(crate) fn problem_text(problem: &Problem, empty: &str) -> String {
    match problem {
        Problem::Empty => empty.to_owned(),
        Problem::NotAKeyRequest => "That isn’t a key request for one of your agents.".into(),
        Problem::NoSuchAccount => "No account has that name or number.".into(),
        Problem::SeveralAccounts => {
            "More than one account has that name. Add it by its number.".into()
        }
        Problem::Refused(message) => format!("That didn’t go through: {message}"),
    }
}

/// The network's name: the session's `<label>#<salt>` up to the `#`; none
/// while the session names no network.
pub(crate) fn network(session: &Session) -> Option<&str> {
    session
        .chain_id
        .split('#')
        .next()
        .filter(|name| !name.is_empty())
}
