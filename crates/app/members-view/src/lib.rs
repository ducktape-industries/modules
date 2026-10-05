//! Members: every account the `identity` program holds, as a list beside
//! one account read in full. The list groups people, then agents, then
//! modules; the account chosen from it shows its bio, its devices, the
//! agents it manages and what its keys signed lately.
//!
//! The screen is read-only: suspending, revoking, renaming and keys live in
//! Settings. Nothing here asks a program anything the list does not: one
//! `identity` list gives every account whole, `valset` the standing of the
//! keys, and the recent activity is a scan of the chain's recent window
//! ([`activity`]).
//!
//! The contracts are borsh and this view's state is a serde snapshot, so a
//! reply is folded to [`Row`]s as it lands: nothing the programs speak is
//! kept across a snapshot, only what the screen shows.
mod activity;
mod ui;

use ducktape_view_guest::prelude::*;
use module_registry::PageRequest;
use serde::{Deserialize, Serialize};

use identity::Identity;
use valset::Valset;

pub use activity::Recent;

#[derive(Serialize, Deserialize, Default)]
pub struct Members {
    rows: Loadable<Vec<Row>>,
    /// what the reader typed into the filter; the rows are never refetched
    /// for it, since the program has no search
    filter: TextField,
    /// the kind chip that is on; `None` is All. It and the filter narrow the
    /// list together and never touch `selected`.
    only: Option<Group>,
    /// the account the detail shows, kept while the filter hides its row
    selected: Option<u64>,
    /// the reader's own account, from the session
    #[serde(skip)]
    me: Option<u64>,
    /// the chain links are minted on, from the session
    #[serde(skip)]
    chain: String,
    /// what the selected account signed lately; read again on restore
    #[serde(skip)]
    activity: Loadable<Recent>,
    /// each account's activity as last read: an account chosen again shows
    /// it at once (the arrows run down the list and back) while `rereading`
    /// reads it anew
    #[serde(skip)]
    seen: std::collections::HashMap<u64, Recent>,
    #[serde(skip)]
    rereading: Option<Task<()>>,
    /// the roster's re-read while the rows on screen stay
    #[serde(skip)]
    rereading_rows: Option<Task<()>>,
    /// the pane's width, read from the window each render
    #[serde(skip)]
    width: f32,
    /// the list's dragged width; `None` until the first drag
    #[serde(skip)]
    list: Option<f32>,
}

/// The list's three groups, in the order they are drawn.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum Group {
    People,
    Agents,
    Modules,
}

impl Group {
    pub const ALL: [Group; 3] = [Group::People, Group::Agents, Group::Modules];

    pub fn label(self) -> &'static str {
        match self {
            Group::People => "People",
            Group::Agents => "Agents",
            Group::Modules => "Modules",
        }
    }

    fn of(kind: &identity::Kind) -> Group {
        match kind {
            identity::Kind::Person => Group::People,
            identity::Kind::Managed { .. } => Group::Agents,
            identity::Kind::Module(_) => Group::Modules,
        }
    }
}

/// One account as this screen shows it.
#[derive(Clone, PartialEq, Serialize, Deserialize)]
struct Row {
    number: u64,
    name: String,
    bio: Option<String>,
    /// what the account is; labelled as it is drawn ([`identity::view::kind`])
    #[serde(with = "ducktape_view_guest::borsh_bytes")]
    kind: identity::Kind,
    devices: Vec<Device>,
    /// the valset standing of a key this account holds, where it holds one
    standing: Option<String>,
}

impl Row {
    fn group(&self) -> Group {
        Group::of(&self.kind)
    }

    fn manager(&self) -> Option<u64> {
        match self.kind {
            identity::Kind::Managed { manager, .. } => Some(manager),
            _ => None,
        }
    }
}

/// One key an account acts with.
#[derive(Clone, PartialEq, Serialize, Deserialize)]
struct Device {
    label: Option<String>,
    key: Vec<u8>,
    /// when it joined, in milliseconds
    added_at: u64,
}

impl View for Members {
    const NAME: &'static str = "Members";
    const DESCRIPTION: &'static str =
        "Every account of this network: who it is, what it runs and what it signed lately.";
    const ICON: &'static str = "icons/users.svg";
    const CAPABILITIES: &'static [Capability] = &[
        Capability::Chain,
        Capability::Module,
        Capability::Host,
        Capability::Link,
    ];
    const TARGETS: &'static [&'static str] = &[identity::MODULE, valset::MODULE];
    const MIN_WINDOW_WIDTH: u32 = 320;

    fn attach(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        cx.follow::<HostSession>((), |view, session, cx| match session {
            Ok(session) => {
                if (view.me, &view.chain) != (session.account, &session.chain_id) {
                    view.me = session.account;
                    view.chain = session.chain_id;
                    cx.notify();
                }
            }
            Err(refusal) => cx.log_refused("the session", &refusal),
        })
        .detach();
        // the roster joins two programs: a block of either re-reads it
        cx.follow::<Changes<Identity>>((), |view, bump, cx| match bump {
            Ok(_) => view.read(cx),
            Err(refusal) => cx.log_refused("identity's live heads", &refusal),
        })
        .detach();
        cx.follow::<Changes<Valset>>((), |view, bump, cx| match bump {
            Ok(_) => view.read(cx),
            Err(refusal) => cx.log_refused("valset's live heads", &refusal),
        })
        .detach();
        self.read(cx);
        self.read_activity(cx);
    }
}

impl Render for Members {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        // the pane's width, as the host lays the frame out: the detail
        // docks or floats by it in the frame that shows it
        self.width = window.viewport_size().width.into();
        ui::render(self, cx)
    }
}

impl Members {
    /// One read of both programs — the boot, a retry, a restore, a live
    /// bump of either. Rows already on screen stay there while it runs, so a bump
    /// never blinks the list back to "Loading", and a bump that changed no
    /// row draws nothing; a refused bump is logged and leaves them. A newer
    /// read cancels the one before.
    fn read(&mut self, cx: &mut Context<Self>) {
        let task = cx.land(roster(cx.host()), |view, result, cx| {
            let rescan = view.activity.is_idle();
            match (result, view.rows.ready()) {
                (Ok(rows), Some(old)) if *old == rows => {}
                (Ok(rows), old) => {
                    let old = old.map_or(&[][..], Vec::as_slice);
                    let keys_moved = view
                        .selected
                        .is_some_and(|number| keys(old, number) != keys(&rows, number));
                    view.rows = Loadable::Ready(rows);
                    cx.notify();
                    if keys_moved {
                        return view.read_activity(cx);
                    }
                }
                (Err(refusal), Some(_)) => cx.log_refused("a refresh", &refusal),
                (Err(refusal), None) => {
                    view.rows = Loadable::Failed(refusal);
                    cx.notify();
                }
            }
            if rescan {
                view.read_activity(cx);
            }
        });
        match self.rows.ready() {
            Some(_) => self.rereading_rows = Some(task),
            None => {
                self.rows = Loadable::Loading(task);
                cx.notify();
            }
        }
    }

    /// Shows `number` in the detail and reads what it signed lately.
    pub fn select(&mut self, number: u64, cx: &mut Context<Self>) {
        if self.selected == Some(number) {
            return;
        }
        self.selected = Some(number);
        self.activity = Loadable::Idle;
        self.read_activity(cx);
        cx.notify();
    }

    /// Reads the selected account's recent activity, once its keys are
    /// known; an account with no keys signs nothing and asks nothing.
    /// Notifies when the detail's activity changes.
    fn read_activity(&mut self, cx: &mut Context<Self>) {
        self.rereading = None;
        let Some(row) = self.selected_row() else {
            return;
        };
        if row.devices.is_empty() {
            if !self.activity.is_idle() {
                self.activity = Loadable::Idle;
                cx.notify();
            }
            return;
        }
        let number = row.number;
        let keys = row
            .devices
            .iter()
            .map(|device| device.key.clone())
            .collect();
        let work = activity::recent(cx.host(), keys);
        // held in `activity` or `rereading`, so choosing someone else drops
        // it unfinished
        let task = cx.land(work, move |view, result, cx| {
            if let Ok(recent) = &result {
                view.seen.insert(number, recent.clone());
            }
            match result {
                // read anew and the same: nothing to draw
                Ok(recent) if view.activity.ready() == Some(&recent) => {}
                result => {
                    view.activity = Loadable::from(result);
                    cx.notify();
                }
            }
        });
        match self.seen.get(&number) {
            Some(recent) => {
                if self.activity.ready() != Some(recent) {
                    self.activity = Loadable::Ready(recent.clone());
                    cx.notify();
                }
                self.rereading = Some(task);
            }
            None => {
                self.activity = Loadable::Loading(task);
                cx.notify();
            }
        }
    }

    fn selected_row(&self) -> Option<&Row> {
        let number = self.selected?;
        self.rows.ready()?.iter().find(|row| row.number == number)
    }

    /// The rows the filter and the chip let through, grouped and in order.
    fn shown(&self) -> Vec<&Row> {
        let Some(rows) = self.rows.ready() else {
            return Vec::new();
        };
        let needle = self.filter.text().trim().to_lowercase();
        let mut shown: Vec<&Row> = rows
            .iter()
            .filter(|row| self.only.is_none_or(|only| row.group() == only))
            .filter(|row| {
                needle.is_empty()
                    || row.name.to_lowercase().contains(&needle)
                    || row.number.to_string().contains(&needle)
            })
            .collect();
        shown.sort_by_key(|row| (row.group(), row.number));
        shown
    }
}

/// The roster, with each account's valset standing joined on the keys it
/// holds.
///
/// Both programs answer in pages; the roster follows every `next` cursor to
/// the end, since the screen shows the whole network.
async fn roster(host: Host) -> Result<Vec<Row>, Error> {
    let page = |after| PageRequest { after, limit: None };
    let accounts = host
        .query_all(|after| identity::ask::List { page: page(after) })
        .await?;
    let members = host
        .query_all(|after| valset::ask::Memberships { page: page(after) })
        .await?;
    Ok(accounts
        .iter()
        .map(|account| row(account, &members))
        .collect())
}

/// The keys account `number` holds in `rows`, if it is there.
fn keys(rows: &[Row], number: u64) -> Option<Vec<&[u8]>> {
    let row = rows.iter().find(|row| row.number == number)?;
    Some(row.devices.iter().map(|device| &device.key[..]).collect())
}

fn row(account: &identity::Account, members: &[valset::Membership]) -> Row {
    Row {
        number: account.number,
        name: account.card.name.clone(),
        bio: account.card.bio.clone(),
        kind: account.kind(),
        devices: account
            .keys()
            .iter()
            .map(|key| Device {
                label: key.label.clone(),
                key: key.key.clone(),
                added_at: key.added_at,
            })
            .collect(),
        standing: members
            .iter()
            .find(|member| account.holds(&member.key))
            .map(|member| {
                match member.role {
                    valset::Role::Validator => "Validator",
                    valset::Role::Resident => "Resident",
                }
                .into()
            }),
    }
}

export_view!(Members);

#[cfg(test)]
mod tests;
