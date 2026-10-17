//! One commit: every change checked, every touched path compared between the base
//! and the head, then the head's tree edited change by change, each write checked against
//! the tree as the changes before it left it, written back and named by a new snapshot.
use std::collections::{BTreeMap, BTreeSet};

use ::program::{
    AccountNumber, BlobId, Error, ExecCtx, Ownership, Principal, QueryCtx, invalid, not_found,
    stale, unexpected_reply,
};

use crate::objects::{Access, FileObj, SnapshotObj, check_chunks, digest, put_file, put_snapshot};
use crate::paths::{Act, change_path, join, writable, writer};
use crate::state::{COUNT, HEAD, HISTORY, SNAPSHOTS, minted, newest_first};
use crate::tree::{TreeEdit, node_at};
use crate::{Change, Content, Node, SnapshotRow, carried};

enum Step {
    Put {
        segments: Vec<String>,
        exec: bool,
        meta: BTreeMap<String, String>,
        size: u64,
        chunks: Vec<BlobId>,
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
    SetOwnership {
        segments: Vec<String>,
        write: Option<Ownership>,
    },
}

impl Step {
    fn paths(&self) -> Vec<&[String]> {
        match self {
            Step::Put { segments, .. }
            | Step::Mkdir(segments)
            | Step::Remove(segments)
            | Step::Symlink { segments, .. }
            | Step::SetOwnership { segments, .. } => vec![segments],
            Step::Rename { from, to } => vec![from, to],
        }
    }

    fn act(&self) -> Act {
        match self {
            Step::Put { .. }
            | Step::Mkdir(_)
            | Step::Remove(_)
            | Step::Rename { .. }
            | Step::Symlink { .. } => Act::Write,
            Step::SetOwnership { .. } => Act::SetOwnership,
        }
    }

    fn touched(&self) -> Vec<&[String]> {
        match self.act() {
            Act::Write => self.paths(),
            Act::SetOwnership => Vec::new(),
        }
    }
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
    let steps = plan(ctx, changes)?;
    let writer = writer(ctx, &author)?;
    let mut edit = TreeEdit::over(head_root);
    for step in steps {
        for segments in step.paths() {
            let along = edit.access_along(ctx, segments)?;
            writable(&writer, segments, &along, step.act())?;
        }
        unchanged_since_base(ctx, base_root, head_root, &step.touched())?;
        apply(ctx, writer.number(), &mut edit, step)?;
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

fn plan(ctx: &QueryCtx, changes: Vec<Change>) -> Result<Vec<Step>, Error> {
    let mut seen = BTreeSet::new();
    let mut touch = |segments: &Vec<String>| {
        let fresh = seen.insert(segments.clone());
        if !fresh {
            return Err(invalid(format!(
                "{} is touched twice in one commit",
                join(segments)
            )));
        }
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
                let segments = change_path(&path)?;
                touch(&segments)?;
                check_meta(&meta)?;
                let Content::Chunks { size, chunks } = content else {
                    return Err(carried());
                };
                check_chunks(ctx, size, &chunks)?;
                Step::Put {
                    segments,
                    exec,
                    meta,
                    size,
                    chunks,
                }
            }
            Change::Mkdir { path } => {
                let segments = change_path(&path)?;
                touch(&segments)?;
                Step::Mkdir(segments)
            }
            Change::Remove { path } => {
                let segments = change_path(&path)?;
                touch(&segments)?;
                Step::Remove(segments)
            }
            Change::Move { from, to } => {
                let from = change_path(&from)?;
                let to = change_path(&to)?;
                touch(&from)?;
                touch(&to)?;
                Step::Rename { from, to }
            }
            Change::Symlink { path, target } => {
                let segments = change_path(&path)?;
                touch(&segments)?;
                if target.is_empty() {
                    return Err(invalid("a symlink's target is not empty"));
                }
                Step::Symlink { segments, target }
            }
            Change::SetOwnership { path, write } => {
                let segments = change_path(&path)?;
                if let Some(write) = &write {
                    known(ctx, write.listed())?;
                }
                Step::SetOwnership { segments, write }
            }
        };
        steps.push(step);
    }
    Ok(steps)
}

fn known(ctx: &QueryCtx, listed: &[AccountNumber]) -> Result<(), Error> {
    use abi::role::identity::{Query, Reply};
    let identity = ctx.env().role(abi::role::IDENTITY)?.clone();
    for number in listed {
        match ctx.query::<Query, Reply>(&identity, &Query::Profile(*number))? {
            Reply::Profile(Some(_)) => {}
            Reply::Profile(None) => return Err(not_found(format!("account {number}"))),
            other => return Err(unexpected_reply(&identity, "Profile", &other)),
        }
    }
    Ok(())
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
    touched: &[&[String]],
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

fn apply(
    ctx: &ExecCtx,
    by: Option<AccountNumber>,
    edit: &mut TreeEdit,
    step: Step,
) -> Result<(), Error> {
    match step {
        Step::Put {
            segments,
            exec,
            meta,
            size,
            chunks,
        } => {
            let file = put_file(ctx, &FileObj { size, chunks, meta })?;
            edit.put(ctx, &segments, Node::File { file, size, exec }, by)
        }
        Step::Mkdir(segments) => edit.mkdir(ctx, &segments, by),
        Step::Remove(segments) => edit.remove(ctx, &segments),
        Step::Rename { from, to } => edit.rename(ctx, &from, &to),
        Step::Symlink { segments, target } => {
            edit.put(ctx, &segments, Node::Symlink { target }, by)
        }
        Step::SetOwnership { segments, write } => {
            let row = edit.access_along(ctx, &segments)?.pop().flatten();
            edit.set_access(ctx, &segments, owned_access(&segments, row, by, write))
        }
    }
}

fn owned_access(
    segments: &[String],
    row: Option<Access>,
    setter: Option<AccountNumber>,
    write: Option<Ownership>,
) -> Access {
    let recorded = row.and_then(|row| row.creator);
    let creator = match segments {
        [root, _] if root == "home" => setter.or(recorded),
        _ => recorded,
    };
    Access {
        creator,
        write: write.map(|write| write.settled(creator)),
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
