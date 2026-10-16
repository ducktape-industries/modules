//! The `files` program: duckfs, the network's versioned filesystem. Its wire types live
//! here; `program.rs` dispatches them, `ops.rs`/`commit.rs` write, `queries.rs` reads, and
//! `paths.rs` decides who writes a path, by its write list or the home and `/shared/` rule.
mod commit;
mod cursor;
mod objects;
mod ops;
pub mod paths;
mod program;
mod queries;
mod state;
#[cfg(test)]
mod tests;
mod tree;

use std::collections::BTreeMap;

use borsh::{BorshDeserialize, BorshSerialize};

pub use ::program::{AccountNumber, BlobId, Ownership, Principal};
pub use objects::{Access, CHUNK, FILE, FileObj, SNAPSHOT, SnapshotObj, TREE, Tree};
pub use program::Files;
pub use store::{PageRequest, PageResponse};

pub const PROGRAM: &str = "files";

pub const CHUNK_SIZE: u64 = 1 << 20;

#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub enum Op {
    Stage {
        bytes: Vec<u8>,
    },
    Commit {
        base: Option<BlobId>,
        message: String,
        changes: Vec<Change>,
    },
    Pin {
        name: String,
        snapshot: BlobId,
    },
    Unpin {
        name: String,
    },
}

#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub enum Change {
    Put {
        path: String,
        exec: bool,
        meta: BTreeMap<String, String>,
        content: Content,
    },
    Mkdir {
        path: String,
    },
    Remove {
        path: String,
    },
    Move {
        from: String,
        to: String,
    },
    Symlink {
        path: String,
        target: String,
    },
    SetOwnership {
        path: String,
        write: Option<Ownership>,
    },
}

impl Change {
    pub fn paths(&self) -> Vec<&str> {
        match self {
            Change::Put { path, .. }
            | Change::Mkdir { path }
            | Change::Remove { path }
            | Change::Symlink { path, .. }
            | Change::SetOwnership { path, .. } => vec![path],
            Change::Move { from, to } => vec![from, to],
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub enum Content {
    Inline(Vec<u8>),
    Chunks { size: u64, chunks: Vec<BlobId> },
}

#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub enum Node {
    File { file: BlobId, size: u64, exec: bool },
    Dir { tree: BlobId, entries: u64 },
    Symlink { target: String },
}

impl Node {
    pub fn kind(&self) -> &'static str {
        match self {
            Node::File { .. } => "file",
            Node::Dir { .. } => "dir",
            Node::Symlink { .. } => "symlink",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct EntryInfo {
    pub path: String,
    pub node: Node,
    pub meta: BTreeMap<String, String>,
}

#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct SnapshotRow {
    pub id: BlobId,
    pub seq: u64,
    pub parent: Option<BlobId>,
    pub root: BlobId,
    pub author: Principal,
    pub height: u64,
    pub time: u64,
    pub message: String,
}

#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct PinRow {
    pub name: String,
    pub snapshot: BlobId,
    pub owner: Principal,
    pub height: u64,
}

#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct Contents {
    pub snapshot: BlobId,
    pub size: u64,
    pub offset: u64,
    pub bytes: Vec<u8>,
    pub eof: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct Hit {
    pub path: String,
    pub line: u64,
    pub text: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub enum DiffKind {
    Added,
    Removed,
    Modified,
}

#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct DiffEntry {
    pub path: String,
    pub kind: DiffKind,
}

#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub enum Writers {
    Listed {
        path: String,
        creator: Option<AccountNumber>,
        write: Ownership,
    },
    Home(String),
    Shared,
    Chain,
}

#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct PathOwnership {
    pub snapshot: Option<BlobId>,
    pub creator: Option<AccountNumber>,
    pub write: Option<Ownership>,
    pub writers: Writers,
}

#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize, ::program::Ask)]
#[ask(Files)]
pub enum Query {
    #[ask(Reply::Head(Option<SnapshotRow>))]
    #[reads(state::HEAD)]
    Head,
    #[ask(Reply::Snapshot(Option<SnapshotRow>))]
    #[reads(state::HISTORY, state::SNAPSHOTS)]
    Snapshot { id: BlobId },
    #[ask(Reply::History(PageResponse<SnapshotRow>))]
    #[reads(state::HISTORY)]
    History { page: PageRequest },
    #[ask(Reply::Stat { snapshot: Option<BlobId>, entry: Option<EntryInfo> })]
    #[reads(state::HEAD, state::HISTORY, state::SNAPSHOTS)]
    Stat { path: String, at: Option<BlobId> },
    #[ask(Reply::Listing { snapshot: Option<BlobId>, entries: PageResponse<EntryInfo> })]
    #[reads(state::HEAD, state::HISTORY, state::SNAPSHOTS)]
    List {
        path: String,
        at: Option<BlobId>,
        page: PageRequest,
    },
    #[ask(Reply::Contents(crate::Contents))]
    #[reads(state::HEAD, state::HISTORY, state::SNAPSHOTS)]
    Read {
        path: String,
        at: Option<BlobId>,
        offset: u64,
        len: u64,
    },
    #[ask(Reply::Listing { snapshot: Option<BlobId>, entries: PageResponse<EntryInfo> })]
    #[reads(state::HEAD, state::HISTORY, state::SNAPSHOTS)]
    Find {
        prefix: String,
        at: Option<BlobId>,
        page: PageRequest,
    },
    #[ask(Reply::Hits { snapshot: Option<BlobId>, hits: PageResponse<Hit> })]
    #[reads(state::HEAD, state::HISTORY, state::SNAPSHOTS)]
    Grep {
        pattern: String,
        prefix: String,
        at: Option<BlobId>,
        page: PageRequest,
    },
    #[ask(Reply::Diff(Vec<DiffEntry>))]
    #[reads(state::HISTORY, state::SNAPSHOTS)]
    Diff {
        from: BlobId,
        to: BlobId,
        prefix: String,
    },
    #[ask(Reply::Pins(PageResponse<PinRow>))]
    #[reads(state::PINS)]
    Pins { page: PageRequest },
    #[ask(Reply::Pin(Option<PinRow>))]
    #[reads(state::PINS)]
    Pin { name: String },
    #[ask(Reply::Held(Vec<bool>))]
    Held { chunks: Vec<BlobId> },
    #[ask(Reply::Ownership(crate::PathOwnership))]
    #[reads(state::HEAD, state::HISTORY, state::SNAPSHOTS)]
    Ownership { path: String, at: Option<BlobId> },
}

#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub enum Reply {
    Head(Option<SnapshotRow>),
    Snapshot(Option<SnapshotRow>),
    History(PageResponse<SnapshotRow>),
    Stat {
        snapshot: Option<BlobId>,
        entry: Option<EntryInfo>,
    },
    Listing {
        snapshot: Option<BlobId>,
        entries: PageResponse<EntryInfo>,
    },
    Contents(Contents),
    Hits {
        snapshot: Option<BlobId>,
        hits: PageResponse<Hit>,
    },
    Diff(Vec<DiffEntry>),
    Pins(PageResponse<PinRow>),
    Pin(Option<PinRow>),
    Held(Vec<bool>),
    Ownership(PathOwnership),
}

pub fn describe(op: &Op) -> describe::Description {
    use describe::{Value, field};
    let hash = |id: &BlobId| Value::Hash(id.digest().to_vec());
    let (title, fields) = match op {
        Op::Stage { bytes } => (
            "Stage a chunk".to_owned(),
            vec![field("bytes", Value::count(bytes.len() as u64))],
        ),
        Op::Commit {
            base,
            message,
            changes,
        } => {
            let paths = changes
                .iter()
                .flat_map(Change::paths)
                .map(Value::text)
                .collect();
            let mut fields = vec![
                field("message", Value::text(message)),
                field("paths", Value::List(paths)),
            ];
            fields.extend(changes.iter().filter_map(write_list));
            if let Some(base) = base {
                fields.push(field("base", hash(base)));
            }
            (format!("Commit {} changes", changes.len()), fields)
        }
        Op::Pin { name, snapshot } => (
            format!("Pin {name}"),
            vec![field("snapshot", hash(snapshot))],
        ),
        Op::Unpin { name } => (format!("Unpin {name}"), Vec::new()),
    };
    describe::Description { title, fields }
}

fn write_list(change: &Change) -> Option<describe::Field> {
    use describe::{Value, field};
    let Change::SetOwnership { path, write } = change else {
        return None;
    };
    let creator = || Value::text("its creator");
    let who = match write {
        None => Value::text("no list"),
        Some(Ownership::Owner) => creator(),
        Some(Ownership::Accounts(listed)) => Value::List(
            std::iter::once(creator())
                .chain(listed.iter().copied().map(Value::Account))
                .collect(),
        ),
        Some(Ownership::Anyone) => Value::text("any account"),
    };
    Some(field(
        "write list",
        Value::List(vec![Value::text(path), who]),
    ))
}

describe::export!(Op, describe);

#[test]
fn op_variants_only_append() {
    assert_eq!(
        describe::variants::<Op>(),
        ["Stage", "Commit", "Pin", "Unpin"]
    );
}
