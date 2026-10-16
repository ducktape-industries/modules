//! Every sentence and status word the members screens show, once: the desk
//! and the phone both read them from here. Expo types them inline in
//! `app/(tabs)/members/index.tsx` and `[number].tsx`; each cites its line.
use view::design;

/// The screen's title (`index.tsx:85`).
pub(crate) const MEMBERS: &str = "Members";
/// The filter field's name for assistive technology.
pub(crate) const FILTER_LABEL: &str = "Filter members";
/// The filter's placeholder (`index.tsx:92`).
pub(crate) const FILTER_HINT: &str = "Filter by name or number";
/// The roster's first read (`index.tsx:32`).
pub(crate) const READING_ROSTER: &str = "Reading the roster…";
/// A network with nobody on it (`index.tsx:33`).
pub(crate) const NO_ACCOUNTS: &str = "No accounts";
pub(crate) const NO_ACCOUNTS_WHY: &str =
    "The identity program of this network holds no accounts yet.";
/// Nobody the filter and the chip let through (`index.tsx:36-37`).
pub(crate) const NOTHING_MATCHES: &str = "Nothing matches";
/// After the reader's own name (`index.tsx:58`, `[number].tsx:86`).
pub(crate) const YOU: &str = "you";
/// An agent's standing while it acts (`[number].tsx:134`).
pub(crate) const ACTIVE: &str = "active";
/// A program account (`index.tsx:69`, `[number].tsx:66`).
pub(crate) const PROGRAM: &str = "Program";
/// The member's two ways out (`[number].tsx:106-107`).
pub(crate) const OPEN_DM: &str = "Open DM";
pub(crate) const EXPLORER: &str = "Explorer";
/// The member's sections (`[number].tsx:110, 132, 152`).
pub(crate) const DEVICES: &str = "Devices";
pub(crate) const MANAGES: &str = "Manages";
pub(crate) const RECENT_ACTIVITY: &str = "Recent activity";
/// An account with no keys (`[number].tsx:111`).
pub(crate) const NO_DEVICES: &str = "No devices";
/// The activity read on its way, and an empty one (`[number].tsx:157-159`).
pub(crate) const READING_ACTIVITY: &str = "Reading its activity…";
pub(crate) const NO_ACTIVITY: &str = "No activity recorded.";
/// Under an agent: where its manager changes it (`[number].tsx:178`).
pub(crate) const MANAGED_IN_SETTINGS: &str =
    "Suspend, revoke, rename and keys live in Account → Agents, for the manager.";

/// Nothing matches, and why: the chip alone (`No agents on this network
/// yet.`), else the filter (`index.tsx:36`).
pub(crate) fn no_match(needle: &str, only: Option<&str>) -> String {
    match (needle, only) {
        ("", Some(group)) => format!("No {} on this network yet.", group.to_lowercase()),
        (needle, _) => format!("No account reads like “{needle}”."),
    }
}

/// `account 7` (`[number].tsx:77`).
pub(crate) fn account(number: u64) -> String {
    format!("account {number}")
}

/// How many keys an account holds: `1 device`, `5 devices`
/// (`[number].tsx:92`).
pub(crate) fn devices(count: usize) -> String {
    design::plural(count as u64, "device", "devices")
}

/// A managed agent's caption: `account 9 · Agent` (`[number].tsx:140`).
pub(crate) fn agent(number: u64) -> String {
    format!("account {number} · Agent")
}

/// A key with no label: `Device 2`, counted from 1 (`[number].tsx:116`).
pub(crate) fn device(index: usize) -> String {
    format!("Device {}", index + 1)
}

/// The day a key joined: `added 15 Oct 2026` (`[number].tsx:124`).
pub(crate) fn added(at: u64) -> String {
    format!("added {}", design::day(at))
}
