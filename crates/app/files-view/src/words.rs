//! Every sentence, label and status word both designs show, once: the
//! desktop's pages (`ui/`) and the phone's screens (`ui/phone/`) word
//! things from here and never type a sentence the other also types.
use files::{Change, DiffKind, Node};
use view::design::{grouped, plural};

use crate::state::{Note, PREVIEW, name_of};
use crate::write::{Naming, Write};

pub const TITLE: &str = "Files";
pub const PLACES: &str = "Places";
pub const HOME: &str = "Home";
pub const SHARED: &str = "Shared";
pub const EVERYTHING: &str = "Everything";
pub const HISTORY: &str = "History";
pub const PINS: &str = "Pins";
pub const CHANGES: &str = "Changes";
pub const LATEST: &str = "Latest snapshots";
pub const CHAIN: &str = "the chain";

pub const SHARED_CAPTION: &str = "Any account writes here";
pub const EVERYTHING_CAPTION: &str = "The whole tree";
pub const NO_NAME: &str = "No home: your account has no name, so you write under /shared only";
pub const SIGNED_OUT: &str = "Sign in to have a home";
pub const FINDING_HOME: &str = "Finding your home…";

pub const READING_FOLDER: &str = "Reading the folder…";
pub const READING_FILE: &str = "Reading the file…";
pub const READING_HISTORY: &str = "Reading the history…";
pub const READING_CHANGES: &str = "Reading what changed…";

pub const EMPTY_FOLDER: &str = "Nothing here yet";
pub const EMPTY_WRITABLE: &str = "Make a folder or a text file here.";
pub const EMPTY_READ_ONLY: &str = "Its writers have put nothing here.";
pub const NO_SNAPSHOTS: &str = "No snapshot yet";
pub const NO_SNAPSHOTS_DETAIL: &str = "The first commit to files makes one.";
pub const NO_PINS: &str = "No snapshot is pinned.";
pub const OLDER: &str = "Older snapshots are not shown.";
pub const NOT_LISTED: &str = "This snapshot is older than the history shows.";
pub const OLDER_SNAPSHOT: &str = "an older snapshot";
pub const FIRST_SNAPSHOT: &str = "The first snapshot: everything in it is new.";
pub const NOTHING_CHANGED: &str = "It changed nothing.";
pub const GONE: &str = "Nothing is at this path any more.";

pub const NAME_IT: &str = "Name it.";
pub const ONE_NAME: &str = "A name has no /: rename it afterwards to move it.";
pub const SAME_PATH: &str = "It is at that path already.";
pub const IN_DOUBT: &str = "No answer came back: it may have gone through.";

pub const NAME: &str = "Name";
pub const NEW_PATH: &str = "New path";
pub const TEXT: &str = "Text";
pub const MESSAGE: &str = "Message";
pub const CANCEL: &str = "Cancel";
pub const COMMIT: &str = "Commit";
pub const COMMITTING: &str = "Committing…";
pub const ACTIONS: &str = "Actions";

pub const SIZE: &str = "Size";
pub const KIND: &str = "Kind";
pub const TARGET: &str = "Points to";
pub const SNAPSHOT: &str = "Snapshot";
pub const AUTHOR: &str = "By";
pub const TIME: &str = "Time";
pub const BLOCK: &str = "Block";

/// An account by its name, else its number.
pub fn account(number: u64, name: Option<&str>) -> String {
    match name {
        Some(name) => name.to_owned(),
        None => format!("account {number}"),
    }
}

/// A count of bytes, in the unit that reads best.
pub fn size(bytes: u64) -> String {
    const KB: u64 = 1024;
    const MB: u64 = KB * 1024;
    match bytes {
        bytes if bytes < KB => plural(bytes, "byte", "bytes"),
        bytes if bytes < MB => format!("{:.1} KB", bytes as f64 / KB as f64),
        bytes => format!("{:.1} MB", bytes as f64 / MB as f64),
    }
}

/// What a folder's row says under its name: how many entries, how big,
/// or where a link points.
pub fn entry_caption(node: &Node) -> String {
    match node {
        Node::Dir { entries, .. } => plural(*entries, "item", "items"),
        Node::File { size: bytes, .. } => size(*bytes),
        Node::Symlink { target } => format!("→ {target}"),
    }
}

/// The glyph a row of this kind leads with (a Lucide name).
pub fn glyph(node: &Node) -> &'static str {
    match node {
        Node::Dir { .. } => "folder",
        Node::File { .. } => "file-text",
        Node::Symlink { .. } => "link",
    }
}

/// What a file holds, in a word: text, a link, or bytes that are not text.
pub fn kind(node: Option<&Node>, text: bool) -> &'static str {
    match (node, text) {
        (Some(Node::Symlink { .. }), _) => "Link",
        (Some(Node::Dir { .. }), _) => "Folder",
        (_, true) => "Text",
        (_, false) => "Not text",
    }
}

/// Why only part of a file shows.
pub fn cut(size: u64) -> String {
    format!(
        "Showing the first {} of {}; a file this large is not edited here.",
        self::size(PREVIEW),
        self::size(size)
    )
}

/// What a file that is not text shows in place of its bytes.
pub fn not_text(size: u64) -> String {
    format!("{} that are not text.", self::size(size))
}

/// The snapshot's place in the history.
pub fn snapshot(seq: u64) -> String {
    format!("Snapshot {}", grouped(seq))
}

/// What a write is called on its button or sheet row.
pub fn verb(write: &Write) -> &'static str {
    match write {
        Write::NewFolder { .. } => "New folder",
        Write::NewFile { .. } => "New text file",
        Write::Edit { .. } => "Edit text",
        Write::Rename { .. } => "Rename or move",
        Write::Remove { .. } => "Remove",
    }
}

/// The glyph a write's sheet row leads with.
pub fn verb_glyph(write: &Write) -> &'static str {
    match write {
        Write::NewFolder { .. } => "folder-plus",
        Write::NewFile { .. } => "file-plus",
        Write::Edit { .. } => "pencil",
        Write::Rename { .. } => "folder-input",
        Write::Remove { .. } => "trash",
    }
}

/// What a write's form is headed by: the write and what it is made on.
pub fn form_title(write: &Write) -> String {
    match write {
        Write::NewFolder { folder } | Write::NewFile { folder } => {
            format!("{} in {}", verb(write), name_of(folder))
        }
        Write::Edit { path, .. } | Write::Rename { path } | Write::Remove { path } => {
            format!("{} {}", verb(write), name_of(path))
        }
    }
}

/// What a form's first field is labelled.
pub fn naming(naming: Naming) -> &'static str {
    match naming {
        Naming::Name => NAME,
        Naming::Path => NEW_PATH,
    }
}

/// What a removal's form says it does.
pub fn removes(path: &str, folder: bool) -> String {
    match folder {
        true => format!("{path} and everything in it go from the next snapshot on."),
        false => format!("{path} goes from the next snapshot on."),
    }
}

/// A commit's message when none is typed: what its change does.
pub fn message(change: &Change) -> String {
    match change {
        Change::Mkdir { path } => format!("Make {}", name_of(path)),
        Change::Put { path, .. } => format!("Write {}", name_of(path)),
        Change::Move { from, to } => format!("Move {from} to {to}"),
        Change::Remove { path } => format!("Remove {}", name_of(path)),
        Change::Symlink { path, .. } => format!("Link {}", name_of(path)),
        Change::SetOwnership { path, .. } => format!("Set who writes {}", name_of(path)),
    }
}

/// What a snapshot changed at a path, in a word.
pub fn diff_kind(kind: DiffKind) -> &'static str {
    match kind {
        DiffKind::Added => "added",
        DiffKind::Removed => "removed",
        DiffKind::Modified => "changed",
    }
}

/// A snapshot's message, or what it is without one.
pub fn message_or_none(message: &str) -> &str {
    match message.trim().is_empty() {
        true => "No message",
        false => message,
    }
}

/// The line over the page.
pub fn note(note: &Note) -> String {
    match note {
        Note::Committed(message) => format!("Committed: {message}"),
        Note::Refused(why) => why.clone(),
    }
}
