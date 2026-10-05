//! The state Settings keeps: what it read, and each form as typed.
use ducktape_view_guest::{Loadable, TextField};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

use crate::api::{Invite, Session};
use crate::queries::Seat;

/// The invite lifetimes offered, in days.
pub(crate) const TTL: [u64; 3] = [1, 7, 30];

#[derive(Default, Serialize, Deserialize)]
pub struct Settings {
    pub(crate) session: Session,
    /// the pane the left menu has open
    pub(crate) section: Section,
    pub(crate) account: Loadable<Option<Seat>>,
    pub(crate) invite: Loadable<Invite>,
    /// which of [`TTL`] the next invite lasts
    pub(crate) ttl: usize,
    /// the last copy of the invite: done, or the refusal it met
    pub(crate) copied: Loadable<()>,
    pub(crate) create_account: Form,
    pub(crate) create_agent: Form,
    pub(crate) agent_key: Form,
    /// each agent's rename, by its number
    pub(crate) rename_agent: BTreeMap<u64, Form>,
    /// the one suspend, resume or revoke in flight
    pub(crate) agent_standing: Form,
}

/// The panes of the left menu. Agents is listed only for an account that
/// manages agents.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) enum Section {
    #[default]
    Account,
    Agents,
    Invites,
}

impl Section {
    pub(crate) fn label(self) -> &'static str {
        match self {
            Self::Account => "Account",
            Self::Agents => "Agents",
            Self::Invites => "Invites",
        }
    }
    pub(crate) fn slug(self) -> &'static str {
        match self {
            Self::Account => "account",
            Self::Agents => "agents",
            Self::Invites => "invites",
        }
    }
}

/// A one-field form: what was typed, whether its submit is in flight, and
/// what stopped it.
#[derive(Clone, Default, Serialize, Deserialize)]
pub(crate) struct Form {
    pub(crate) text: TextField,
    pub(crate) busy: bool,
    pub(crate) problem: Option<Problem>,
}

/// Why a form's submit did not land. The form that shows it words it.
#[derive(Clone, Serialize, Deserialize)]
pub(crate) enum Problem {
    /// nothing was typed
    Empty,
    /// the text is not an `AddKey` for one of the reader's agents
    NotAKeyRequest,
    /// the program refused it, in its own words
    Refused(String),
}
