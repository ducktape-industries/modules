//! The `files` program: duckfs, the network's versioned filesystem. Its wire types live
//! here; `program.rs` dispatches them, `ops.rs`/`commit.rs` write, `queries.rs` reads.
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

pub use ::program::{BlobId, Principal};
pub use objects::{CHUNK, FILE, FileObj, SNAPSHOT, SnapshotObj, TREE, Tree};
pub use program::Files;
pub use store::{PageRequest, PageResponse};

pub const PROGRAM: &str = "files";

pub const CHUNK_SIZE: u64 = 1 << 20;
pub const MAX_NAME_BYTES: usize = 255;
pub const MAX_PATH_BYTES: usize = 4096;
pub const MAX_DEPTH: usize = 128;
pub const MAX_DIR_ENTRIES: usize = 65_536;
pub const MAX_CHANGES: usize = 4096;
pub const MAX_INLINE_BYTES: usize = 4 << 20;
pub const MAX_CHUNKS_PER_FILE: usize = 65_536;
pub const MAX_MESSAGE_BYTES: usize = 4096;
pub const MAX_META_ENTRIES: usize = 16;
pub const MAX_META_KEY_BYTES: usize = 64;
pub const MAX_META_VALUE_BYTES: usize = 256;
pub const MAX_SYMLINK_TARGET_BYTES: usize = 4096;
pub const MAX_PIN_NAME_BYTES: usize = 128;
pub const MAX_READ_BYTES: u64 = 1 << 20;
pub const MAX_GREP_SCAN_BYTES: u64 = 8 << 20;
pub const MAX_GREP_LINE_BYTES: usize = 256;
pub const MAX_GREP_HITS_PER_FILE: usize = 4096;
pub const MAX_GREP_PATTERN_BYTES: usize = 256;
pub const MAX_DIFF_ENTRIES: usize = 4096;
pub const MAX_HELD_IDS: usize = 256;

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
}

impl Change {
    pub fn paths(&self) -> Vec<&str> {
        match self {
            Change::Put { path, .. }
            | Change::Mkdir { path }
            | Change::Remove { path }
            | Change::Symlink { path, .. } => vec![path],
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

describe::export!(Op, describe);

#[test]
fn op_variants_only_append() {
    assert_eq!(
        describe::variants::<Op>(),
        ["Stage", "Commit", "Pin", "Unpin"]
    );
}
