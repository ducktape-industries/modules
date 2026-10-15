//! One commit: every change checked, every touched path compared between the base
//! and the head, then the head's tree edited, written back and named by a new snapshot.
use std::collections::{BTreeMap, BTreeSet};

use ::program::{BlobId, Error, ExecCtx, Principal, QueryCtx, invalid, stale};

use crate::objects::{
    FileObj, SnapshotObj, check_chunks, digest, put_bytes, put_file, put_snapshot,
};
use crate::paths::{join, writable_path};
use crate::state::{COUNT, HEAD, HISTORY, SNAPSHOTS, minted, newest_first};
use crate::tree::{TreeEdit, node_at};
use crate::{Change, Content, Node, SnapshotRow};

enum Step {
    Put {
        segments: Vec<String>,
        exec: bool,
        meta: BTreeMap<String, String>,
        content: Content,
    },
    Mkdir(Vec<String>),
    Remove(Vec<String>),
    Rename {
        from: Vec<String>,
        to: Vec<String>,
    },
    Symlink {
        segments: Vec<String>,
        target: String,
    },
}

struct Plan {
    steps: Vec<Step>,
    touched: Vec<Vec<String>>,
}

pub(crate) fn commit(
    ctx: &ExecCtx,
    author: Principal,
    base: Option<BlobId>,
    message: String,
    changes: Vec<Change>,
) -> Result<(), Error> {
    if changes.is_empty() {
        return Err(invalid("a commit carries at least one change"));
    }
    let base_root = base
        .as_ref()
        .map(|id| minted(ctx, id))
        .transpose()?
        .map(|row| row.root);
    let head = HEAD.get(ctx)?;
    let head_root = head.as_ref().map(|row| row.root);
    let plan = plan(ctx, &author, changes)?;
    unchanged_since_base(ctx, base_root, head_root, &plan.touched)?;
    let mut edit = TreeEdit::over(head_root);
    for step in plan.steps {
        apply(ctx, &mut edit, step)?;
    }
    let (root, _) = edit.build(ctx)?;
    let snapshot = SnapshotObj {
        root,
        parent: head.map(|row| row.id),
        author,
        height: ctx.env().height,
        time: ctx.env().time,
        message,
    };
    let id = put_snapshot(ctx, &snapshot)?;
    record(ctx, id, snapshot)?;
    ctx.set_return_data(abi::encode(&id));
    Ok(())
}

fn plan(ctx: &QueryCtx, author: &Principal, changes: Vec<Change>) -> Result<Plan, Error> {
    let mut touched = Vec::new();
    let mut seen = BTreeSet::new();
    let mut touch = |segments: &Vec<String>| {
        let fresh = seen.insert(segments.clone());
        if !fresh {
            return Err(invalid(format!(
                "{} is touched twice in one commit",
                join(segments)
            )));
        }
        touched.push(segments.clone());
        Ok(())
    };
    let mut steps = Vec::with_capacity(changes.len());
    for change in changes {
        let step = match change {
            Change::Put {
                path,
                exec,
                meta,
                content,
            } => {
                let segments = writable_path(author, &path)?;
                touch(&segments)?;
                check_meta(&meta)?;
                if let Content::Chunks { size, chunks } = &content {
                    check_chunks(ctx, *size, chunks)?;
                }
                Step::Put {
                    segments,
                    exec,
                    meta,
                    content,
                }
            }
            Change::Mkdir { path } => {
                let segments = writable_path(author, &path)?;
                touch(&segments)?;
                Step::Mkdir(segments)
            }
            Change::Remove { path } => {
                let segments = writable_path(author, &path)?;
                touch(&segments)?;
                Step::Remove(segments)
            }
            Change::Move { from, to } => {
                let from = writable_path(author, &from)?;
                let to = writable_path(author, &to)?;
                touch(&from)?;
                touch(&to)?;
                Step::Rename { from, to }
            }
            Change::Symlink { path, target } => {
                let segments = writable_path(author, &path)?;
                touch(&segments)?;
                if target.is_empty() {
                    return Err(invalid("a symlink's target is not empty"));
                }
                Step::Symlink { segments, target }
            }
        };
        steps.push(step);
    }
    Ok(Plan { steps, touched })
}

fn check_meta(meta: &BTreeMap<String, String>) -> Result<(), Error> {
    if meta.keys().any(String::is_empty) {
        return Err(invalid("a metadata key is not empty"));
    }
    Ok(())
}

fn unchanged_since_base(
    ctx: &QueryCtx,
    base: Option<BlobId>,
    head: Option<BlobId>,
    touched: &[Vec<String>],
) -> Result<(), Error> {
    if base == head {
        return Ok(());
    }
    let node = |root: Option<BlobId>, segments: &[String]| match root {
        Some(root) => node_at(ctx, &root, segments),
        None => Ok(None),
    };
    for segments in touched {
        if node(base, segments)? != node(head, segments)? {
            return Err(stale(format!(
                "{} changed since the base snapshot: read it again and retry",
                join(segments)
            )));
        }
    }
    Ok(())
}

fn apply(ctx: &ExecCtx, edit: &mut TreeEdit, step: Step) -> Result<(), Error> {
    match step {
        Step::Put {
            segments,
            exec,
            meta,
            content,
        } => {
            let (size, chunks) = match content {
                Content::Inline(bytes) => (bytes.len() as u64, put_bytes(ctx, &bytes)?),
                Content::Chunks { size, chunks } => (size, chunks),
            };
            let file = put_file(ctx, &FileObj { size, chunks, meta })?;
            edit.put(ctx, &segments, Node::File { file, size, exec })
        }
        Step::Mkdir(segments) => edit.mkdir(ctx, &segments),
        Step::Remove(segments) => edit.remove(ctx, &segments),
        Step::Rename { from, to } => edit.rename(ctx, &from, &to),
        Step::Symlink { segments, target } => edit.put(ctx, &segments, Node::Symlink { target }),
    }
}

fn record(ctx: &ExecCtx, id: BlobId, snapshot: SnapshotObj) -> Result<(), Error> {
    let seq = COUNT.get(ctx)?.unwrap_or(0) + 1;
    let row = SnapshotRow {
        id,
        seq,
        parent: snapshot.parent,
        root: snapshot.root,
        author: snapshot.author,
        height: snapshot.height,
        time: snapshot.time,
        message: snapshot.message,
    };
    COUNT.put(ctx, &seq);
    HISTORY.put(ctx, &newest_first(seq), &row);
    SNAPSHOTS.put(ctx, &digest(&id)?, &seq);
    HEAD.put(ctx, &row);
    Ok(())
}
