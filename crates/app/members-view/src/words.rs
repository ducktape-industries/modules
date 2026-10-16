//! Every sentence and status word the members screens show, once: the desk
//! and the phone both read them from here, and neither types one. Expo
//! types them inline in `app/(tabs)/members/index.tsx` and `[number].tsx`;
//! each cites its line. Where the two trees say a thing differently, each
//! word is named for where it shows.
use view::design;

/// The screen's title (`index.tsx:85`).
pub(crate) const MEMBERS: &str = "Members";
/// Opening and closing the filter (`index.tsx:113`, `:106`).
pub(crate) const FILTER: &str = "Filter";
pub(crate) const CLOSE_FILTER: &str = "Close filter";
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
/// The chip that lets everyone through (`index.tsx:120`).
pub(crate) const ALL: &str = "All";
/// After the reader's own name (`index.tsx:58`, `[number].tsx:86`).
pub(crate) const YOU: &str = "you";
/// An agent's standing while it acts (`[number].tsx:134`).
pub(crate) const ACTIVE: &str = "active";
/// A person's account, on both trees: identity's own word
/// (`identity::view::kind`). Expo's "Person" (`index.tsx:69`,
/// `[number].tsx:63`) is its copy of that word from before programs #394
/// renamed it, not a word of its own.
pub(crate) const USER: &str = "User";
/// An agent account (`[number].tsx:69`, `:140`).
pub(crate) const AGENT: &str = "Agent";
/// Before an agent's manager (`index.tsx:63`, `[number].tsx:69`).
pub(crate) const MANAGED_BY: &str = "managed by";
/// A program account (`index.tsx:69`, `[number].tsx:66`).
pub(crate) const PROGRAM: &str = "Program";
/// A key's valset seat (`[number].tsx:96`).
pub(crate) const VALIDATOR: &str = "Validator";
pub(crate) const RESIDENT: &str = "Resident";
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
/// A member page whose account the roster does not hold
/// (`[number].tsx:33`).
pub(crate) const NO_ACCOUNT_WHY: &str = "Identity holds no account by this number.";
/// The desk's own: the list's drag handle, the detail before a choice,
/// a floating detail's Close, and the activity section's order.
pub(crate) const RESIZE_LIST: &str = "Resize the member list";
pub(crate) const CHOOSE_MEMBER: &str = "Choose a member";
pub(crate) const CLOSE: &str = "Close";
pub(crate) const NEWEST_FIRST: &str = "newest first";
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

/// How many accounts the network holds: `1 account`, `15 accounts`
/// (`index.tsx:86`).
pub(crate) fn accounts(count: usize) -> String {
    design::plural(count as u64, "account", "accounts")
}

/// The missing-account page's title: `Account 7` (`[number].tsx:29`).
pub(crate) fn account_title(number: u64) -> String {
    format!("Account {number}")
}

/// The missing-account page's empty state: `No account #7`
/// (`[number].tsx:33`).
pub(crate) fn no_account(number: u64) -> String {
    format!("No account #{number}")
}

/// A frame's block, grouped: `block 12,345` (`[number].tsx:168`).
pub(crate) fn block(place: &str) -> String {
    format!("block {place}")
}

/// The desk's activity link, for assistive technology: `Open <frame> in
/// Explorer`.
pub(crate) fn open_in_explorer(frame: &str) -> String {
    format!("Open {frame} in Explorer")
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
    format!("{} · {AGENT}", account(number))
}

/// A key with no label: `Device 2`, counted from 1 (`[number].tsx:116`).
pub(crate) fn device(index: usize) -> String {
    format!("Device {}", index + 1)
}

/// The day a key joined: `added 15 Oct 2026` (`[number].tsx:124`).
pub(crate) fn added(at: u64) -> String {
    format!("added {}", design::day(at))
}
