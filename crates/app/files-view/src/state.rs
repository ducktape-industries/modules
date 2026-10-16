//! The state Files keeps: where the reader is (a stack of pages), who reads,
//! and what each page read. Rows are the program's own types, worded only
//! where they are drawn (`words.rs`, `ui/`).
use files::{
    BlobId, DiffEntry, EntryInfo, Node, PathOwnership, PinRow, Principal, SnapshotRow, Writers,
};
use serde::{Deserialize, Serialize};
use view::borsh_bytes;
use view::design::Pages;
use view::prelude::*;

use crate::write::Form;

/// Everything under `/shared/`: any account writes there.
pub const SHARED: &str = "/shared";
/// The whole tree.
pub const ROOT: &str = "/";

/// One page of the browser. Both designs draw the same stack: a phone
/// shows its top alone with Back, a desktop window the top under its bar.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Route {
    /// where to start: the reader's home, `/shared`, the whole tree, the
    /// newest snapshots
    Places,
    /// a folder's entries, by path
    Folder(String),
    /// one file, by path
    File(String),
    /// every snapshot, newest first, and the pins
    History,
    /// one snapshot, by its place in the history ([`SnapshotRow::seq`]),
    /// and what it changed
    Snapshot(u64),
}

/// Who reads: the session's account, and the name identity gives it,
/// which names its home (`/home/<name>`).
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Reader {
    pub account: Option<u64>,
    pub name: Name,
}

/// What identity answered of the reader's name.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum Name {
    /// not answered yet
    #[default]
    Asking,
    /// identity names the account no way: it writes under `/shared/` only
    Unnamed,
    Named(String),
    /// the read was refused, in the host's words; asked again when
    /// identity changes
    Refused(String),
}

/// Where the reader's home is, or why there is none to show.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Home<'a> {
    At(String),
    /// no account is signed in
    SignedOut,
    Asking,
    Unnamed,
    Refused(&'a str),
}

impl Reader {
    pub fn home(&self) -> Home<'_> {
        if self.account.is_none() {
            return Home::SignedOut;
        }
        match &self.name {
            Name::Named(name) => Home::At(format!("/home/{name}")),
            Name::Asking => Home::Asking,
            Name::Unnamed => Home::Unnamed,
            Name::Refused(why) => Home::Refused(why),
        }
    }

    /// Whether `files` lets the reader write a path whose writers are
    /// `writers` (its `Ownership` answer): the write list's accounts, the
    /// home's own account, anyone under `/shared/`, else only the chain.
    pub fn writes(&self, writers: &Writers) -> bool {
        let Some(account) = self.account else {
            return false;
        };
        match writers {
            Writers::Listed { creator, write, .. } => write.allows(*creator, account),
            Writers::Home(home) => self.name == Name::Named(home.clone()),
            Writers::Shared => true,
            Writers::Chain => false,
        }
    }
}

/// A folder as `files` listed it at one snapshot.
#[derive(Clone, PartialEq, Serialize, Deserialize)]
pub struct Folder {
    pub path: String,
    /// the snapshot it was read at; `None` before the first commit. A
    /// write made from this folder is based on it, so `files` refuses it
    /// as stale if what it touches moved since.
    #[serde(with = "borsh_bytes")]
    pub snapshot: Option<BlobId>,
    /// folders first, then files and links, each by name
    #[serde(with = "borsh_bytes")]
    pub entries: Vec<EntryInfo>,
    /// who writes the folder itself; `None` for `/`, which no change names
    #[serde(with = "borsh_bytes")]
    pub ownership: Option<PathOwnership>,
}

impl Folder {
    /// Who writes what is made, moved or removed in this folder: the write
    /// list at or above it, else the path rule `files` applies to a child
    /// with no list of its own (`paths::writers`): a child of `/shared` or
    /// below it is anyone's, a child of `/home/<name>` or below it its
    /// account's, any other the chain's.
    pub fn children(&self) -> Writers {
        if let Some(PathOwnership {
            writers: listed @ Writers::Listed { .. },
            ..
        }) = &self.ownership
        {
            return listed.clone();
        }
        let segments: Vec<&str> = self.path.split('/').filter(|s| !s.is_empty()).collect();
        match segments.as_slice() {
            ["shared", ..] => Writers::Shared,
            ["home", home, ..] => Writers::Home((*home).to_owned()),
            _ => Writers::Chain,
        }
    }
}

/// How much of a file is read to show it, and the most a file may hold
/// for its text to be edited here: an edit writes the whole file back.
pub const PREVIEW: u64 = 64 * 1024;

/// One file as `files` answered it at one snapshot.
#[derive(Clone, PartialEq, Serialize, Deserialize)]
pub struct Opened {
    pub path: String,
    #[serde(with = "borsh_bytes")]
    pub snapshot: Option<BlobId>,
    /// what is at the path (`Stat`); `None`: nothing any more
    #[serde(with = "borsh_bytes")]
    pub entry: Option<EntryInfo>,
    /// its first [`PREVIEW`] bytes (`Read`); a link's are its target
    pub bytes: Vec<u8>,
    /// `bytes` is the whole file
    pub whole: bool,
    /// who writes it
    #[serde(with = "borsh_bytes")]
    pub ownership: Option<PathOwnership>,
}

impl Opened {
    /// The bytes as text, when they are UTF-8; a read cut inside a
    /// character's bytes still is, up to that character.
    pub fn text(&self) -> Option<&str> {
        match std::str::from_utf8(&self.bytes) {
            Ok(text) => Some(text),
            Err(cut) if cut.error_len().is_none() && !self.whole => {
                std::str::from_utf8(&self.bytes[..cut.valid_up_to()]).ok()
            }
            Err(_) => None,
        }
    }

    /// The file's size, as its tree records it.
    pub fn size(&self) -> u64 {
        match self.entry.as_ref().map(|entry| &entry.node) {
            Some(Node::File { size, .. }) => *size,
            _ => self.bytes.len() as u64,
        }
    }

    /// Whether its text can be edited here: a file, read whole, as text.
    pub fn editable(&self) -> bool {
        let file = matches!(
            self.entry.as_ref().map(|entry| &entry.node),
            Some(Node::File { .. })
        );
        file && self.whole && self.text().is_some()
    }
}

/// The newest snapshots, and every pin.
#[derive(Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct History {
    /// newest first, at most [`HISTORY_PAGE`]
    #[serde(with = "borsh_bytes")]
    pub snapshots: Vec<SnapshotRow>,
    /// the history holds older snapshots than these
    pub older: bool,
    #[serde(with = "borsh_bytes")]
    pub pins: Vec<PinRow>,
}

/// How many snapshots the history reads, newest first.
pub const HISTORY_PAGE: u64 = 100;

/// What one snapshot changed against its parent (`Diff`).
#[derive(Clone, PartialEq, Serialize, Deserialize)]
pub struct Changed {
    pub seq: u64,
    #[serde(with = "borsh_bytes")]
    pub entries: Vec<DiffEntry>,
}

/// Every account's name, by number, as identity lists them: who made a
/// snapshot.
#[derive(Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Names {
    pub list: Vec<(u64, String)>,
}

impl Names {
    pub fn of(&self, account: u64) -> Option<&str> {
        self.list
            .iter()
            .find(|(number, _)| *number == account)
            .map(|(_, name)| name.as_str())
    }
}

/// The line over the page: what the last write did, or, with no form
/// open to show it in, why it did not land. Worded when drawn.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Note {
    /// the commit landed: its message
    Committed(String),
    /// a write that did not land, in the host's or the program's words
    Refused(String),
}

/// What is open over the page.
pub enum Sheet {
    /// the page's writes as a list: a phone has no room for their
    /// buttons (a desktop draws them on the page and never opens this)
    Actions,
    /// one write being filled in
    Write(Form),
}

#[derive(Serialize, Deserialize)]
pub struct Files {
    pub(crate) pages: Pages<Route>,
    pub(crate) reader: Reader,
    /// the folder on screen, or the last one read
    pub(crate) folder: Loadable<Folder>,
    pub(crate) file: Loadable<Opened>,
    pub(crate) history: Loadable<History>,
    pub(crate) changed: Loadable<Changed>,
    /// the snapshot whose changes were last asked for (`read_changed`)
    #[serde(skip)]
    pub(crate) changed_of: Option<u64>,
    pub(crate) names: Loadable<Names>,
    pub(crate) note: Option<Note>,
    /// never kept: a restored view comes back with nothing half filled
    #[serde(skip)]
    pub(crate) sheet: Option<Sheet>,
}

impl Default for Files {
    fn default() -> Self {
        Self {
            pages: Pages::root(Route::Places),
            reader: Reader::default(),
            folder: Loadable::Idle,
            file: Loadable::Idle,
            history: Loadable::Idle,
            changed: Loadable::Idle,
            changed_of: None,
            names: Loadable::Idle,
            note: None,
            sheet: None,
        }
    }
}

impl Files {
    pub(crate) fn page(&self) -> &Route {
        self.pages.top()
    }

    /// The folder at `path`, once it is read; another folder's entries
    /// never stand in for it.
    pub(crate) fn folder_at(&self, path: &str) -> Option<&Folder> {
        self.folder.ready().filter(|folder| folder.path == path)
    }

    /// The file at `path`, once it is read.
    pub(crate) fn file_at(&self, path: &str) -> Option<&Opened> {
        self.file.ready().filter(|file| file.path == path)
    }

    /// The snapshot at `seq`, among those the history read.
    pub(crate) fn snapshot(&self, seq: u64) -> Option<&SnapshotRow> {
        self.history
            .ready()?
            .snapshots
            .iter()
            .find(|row| row.seq == seq)
    }

    /// Who made a snapshot or a pin, in words: an account's name, else its
    /// number; the chain itself.
    pub(crate) fn author(&self, principal: &Principal) -> String {
        match principal {
            Principal::Account(number) => {
                let name = self.names.ready().and_then(|names| names.of(*number));
                crate::words::account(*number, name)
            }
            Principal::Root => crate::words::CHAIN.into(),
        }
    }

    /// The page's form, while one is open.
    pub(crate) fn form(&self) -> Option<&Form> {
        match &self.sheet {
            Some(Sheet::Write(form)) => Some(form),
            _ => None,
        }
    }
}

/// A path's last segment: a folder's or a file's name; `/` for the root.
pub fn name_of(path: &str) -> &str {
    match path.rsplit_once('/') {
        Some((_, "")) | None => ROOT,
        Some((_, name)) => name,
    }
}

/// `name` inside `folder`.
pub fn child_of(folder: &str, name: &str) -> String {
    match folder {
        ROOT => format!("/{name}"),
        folder => format!("{folder}/{name}"),
    }
}
