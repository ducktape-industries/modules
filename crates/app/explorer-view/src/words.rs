//! The explorer's sentences, once: every sentence, problem and capability
//! line the desktop's pages and the phone's screens both show (the phone's
//! are Expo's: `src/network/words.ts` and the explorer screens, mobile dev
//! 266633c). Neither design types a sentence the other also types; a
//! heading or a button's label stays where it is drawn.
use view::methods::Capability;

use crate::Note;
use crate::decode::{ago, grouped, plural, scheme, short};

pub(crate) const READING_CHAIN: &str = "Reading the chain…";
pub(crate) const READING_BLOCK: &str = "Reading the block…";
pub(crate) const READING_TX: &str = "Reading the transaction…";
pub(crate) const READING_ACCOUNTS: &str = "Reading accounts…";
pub(crate) const READING_ACCOUNT: &str = "Reading the account…";
pub(crate) const READING_ACTIVITY: &str = "Reading its activity…";
pub(crate) const READING_ROLE: &str = "Reading the programs role…";
pub(crate) const READING_CODE: &str = "Reading the code…";

/// The search field's name, and what it takes.
pub(crate) const SEARCH: &str = "Search the chain";
pub(crate) const SEARCH_HINT: &str = "Search by height, hash, account or program";

pub(crate) const ALL_BLOCKS: &str = "All blocks →";
pub(crate) const ALL_TRANSACTIONS: &str = "All transactions →";

pub(crate) const NO_BLOCK_HERE: &str = "This node keeps no finalized block at this height.";
pub(crate) const NO_BLOCK_TRANSACTIONS: &str = "No transactions in this block.";
pub(crate) const NO_TRANSACTION: &str = "Transaction not found";
pub(crate) const NO_ACCOUNT_HERE: &str = "Identity holds no account by this number.";
pub(crate) const NO_ACTIVITY: &str = "No activity recorded.";
pub(crate) const NO_PROGRAM_HERE: &str = "The programs role runs and lists nothing by this name.";
pub(crate) const RUNS_NOTHING: &str = "The programs role of this network runs nothing yet.";
pub(crate) const NOTHING_SCHEDULED: &str = "Nothing is scheduled against the programs role.";
pub(crate) const NOTHING_SCHEDULED_FOR_IT: &str = "Nothing is scheduled for it.";
pub(crate) const NO_MAINTAINER: &str =
    "No account maintains a program: only the validators' vote changes the programs role.";
/// Who changes a role's program, and a program no account maintains.
pub(crate) const VALIDATORS_VOTE: &str = "validators' vote";
pub(crate) const VOTE_ONLY: &str = "no account: the validators' vote only";
/// The accounts whose grant covers every program, named together.
pub(crate) const EVERY_PROGRAM: &str = "every-program maintainers";
/// A change the validators' vote makes.
pub(crate) const BY_VOTE: &str = "by vote";
/// What a Copy put on the clipboard, as its note names it.
pub(crate) const CODE_ID: &str = "the code id";
pub(crate) const LINK: &str = "the link";
/// A grant form before its account is typed.
pub(crate) const TYPE_ACCOUNT: &str = "Type the account's number.";

pub(crate) fn no_block(height: u64) -> String {
    format!("No block {}", grouped(height))
}

pub(crate) fn no_transaction_at(height: u64, position: u32) -> String {
    format!(
        "This node keeps no transaction at position {position} of block {}.",
        grouped(height)
    )
}

pub(crate) fn no_account(number: u64) -> String {
    format!("No account #{number}")
}

/// A grant form's account, typed but held by no one.
pub(crate) fn no_account_numbered(number: u64) -> String {
    format!("No account {number} here.")
}

pub(crate) fn nothing_called(name: &str) -> String {
    format!("Nothing called {name}")
}

/// The link from a transaction to the block it is in.
pub(crate) fn in_block(height: u64) -> String {
    format!("In block {}", grouped(height))
}

/// A device's name where its key carries no label: `Device 2`.
pub(crate) fn device(index: usize) -> String {
    format!("Device {}", index + 1)
}

pub(crate) fn transactions_to(program: &str) -> String {
    format!("Transactions · {program}")
}

/// The node's block time: `Block every 1.0 s`.
pub(crate) fn block_every(block_time_ms: u64) -> String {
    format!("Block every {:.1} s", block_time_ms as f64 / 1000.)
}

/// How far the next epoch is: `Next in 7 blocks`.
pub(crate) fn next_in(blocks: u64) -> String {
    format!("Next in {}", plural(blocks, "block", "blocks"))
}

/// How far back the window reaches: `the last 1,000 blocks`.
pub(crate) fn last_blocks(blocks: u64) -> String {
    format!("the last {}", plural(blocks, "block", "blocks"))
}

/// The window's transactions: `2 transactions in the last 13 blocks`.
pub(crate) fn transactions_in(count: u64, blocks: u64) -> String {
    format!(
        "{} in {}",
        plural(count, "transaction", "transactions"),
        last_blocks(blocks)
    )
}

pub(crate) fn no_transactions_in(blocks: u64) -> String {
    format!("No transactions in {}.", last_blocks(blocks))
}

/// A program's way to its transactions, `sent` of them in the window's
/// `blocks`; with no block yet, no count of nothing.
pub(crate) fn to_transactions(sent: u64, blocks: u64) -> String {
    match blocks {
        0 => "Transactions →".into(),
        _ => format!("{} →", transactions_in(sent, blocks)),
    }
}

/// A block's proposer by its place in the validator set, `validator 1`;
/// a key the set no longer holds, `validator`.
pub(crate) fn validator(place: Option<usize>) -> String {
    match place {
        Some(place) => format!("validator {}", place + 1),
        None => "validator".into(),
    }
}

/// A validator's key: validators sign with ed25519.
pub(crate) fn validator_key(key: &[u8]) -> String {
    format!("ed25519 {}", short(key))
}

/// Which of `account`'s keys signed, its `at`th: `#8 device 1 · ed25519
/// 623b55d3…efd0`.
pub(crate) fn key_line(account: &identity::Account, at: usize, signer: &[u8]) -> String {
    let key = &account.keys()[at];
    let label = key
        .label
        .clone()
        .unwrap_or_else(|| format!("device {}", at + 1));
    format!(
        "#{} {label} · {} {}",
        account.number,
        scheme(key.scheme),
        short(signer)
    )
}

/// When a device last signed, by the time of its newest frame.
pub(crate) fn last_used(now: u64, last: Option<u64>) -> String {
    match last {
        Some(time) => format!("last used {} ago", ago(now, time)),
        None => "not used lately".into(),
    }
}

pub(crate) fn unindexed(from: u64) -> String {
    format!(
        "History before block {} is not indexed on this node.",
        grouped(from)
    )
}

/// The line under the page strip, in words.
pub(crate) fn note(note: &Note) -> String {
    match note {
        Note::NotFound(query) => format!("Nothing here is called “{query}”."),
        Note::NoSuchHash { listed_from } => {
            let since = listed_from.filter(|from| *from > 0);
            match since {
                Some(from) => format!(
                    "No block or transaction since block {} has this hash. {}",
                    grouped(from),
                    unindexed(from)
                ),
                None => "No block or transaction has this hash.".into(),
            }
        }
        Note::Unlinked(route) => format!("This link names nothing the Explorer shows: {route}"),
        Note::Copied(what) => format!("Copied {what}."),
        Note::Sent(words) | Note::Refused(words) => words.clone(),
    }
}

/// What a capability lets a view do, in a line a person reads: the phone's
/// "uses camera" for a view. Every capability is worded here, so a new one
/// is worded before it ships.
pub(crate) fn allows(capability: Capability) -> &'static str {
    match capability {
        Capability::Chain => "reads the chain: blocks, transactions and the node's status",
        Capability::Program => "asks programs questions and follows what they change",
        Capability::Op => "sends operations to the chain in your name",
        Capability::Invite => "mints invites with the node's key",
        Capability::Link => "opens links: another view, or the browser",
        Capability::Blob => "reads blobs the node holds",
        Capability::Host => "knows the session: who you are, which chain, when it is on screen",
        Capability::Clock => "wakes on a timer",
        Capability::Clipboard => "reads and writes the clipboard",
        Capability::Notify => "posts notifications",
        Capability::Store => "keeps its own data on this device",
        Capability::Web => "shows web pages, and takes the web links the app opens",
    }
}
