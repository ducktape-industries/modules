//! Reading a node at a path, and the copy-on-write edit a commit applies: only the
//! directories on a touched path are opened, and only those are written back.
use std::collections::BTreeMap;

use guest::{BlobId, Error, ExecCtx, QueryCtx, already_exists, invalid, not_found, wrong_state};

use crate::Node;
use crate::objects::{put_tree, tree};
use crate::paths::join;

pub(crate) fn node_at(
    ctx: &QueryCtx,
    root: &BlobId,
    segments: &[String],
) -> Result<Option<Node>, Error> {
    let mut dir = *root;
    for (depth, name) in segments.iter().enumerate() {
        let Some(node) = tree(ctx, &dir)?.entries.remove(name) else {
            return Ok(None);
        };
        let last = depth + 1 == segments.len();
        if last {
            return Ok(Some(node));
        }
        let Node::Dir { tree, .. } = node else {
            return Ok(None);
        };
        dir = tree;
    }
    Ok(None)
}

enum Slot {
    Kept(Node),
    Open(BTreeMap<String, Slot>),
}

pub(crate) struct TreeEdit {
    root: Slot,
}

impl TreeEdit {
    pub(crate) fn over(root: Option<BlobId>) -> TreeEdit {
        let root = match root {
            Some(tree) => Slot::Kept(Node::Dir { tree, entries: 0 }),
            None => Slot::Open(BTreeMap::new()),
        };
        TreeEdit { root }
    }

    pub(crate) fn put(
        &mut self,
        ctx: &QueryCtx,
        segments: &[String],
        node: Node,
    ) -> Result<(), Error> {
        let (name, dirs) = split(segments)?;
        let parent = open_dir(&mut self.root, ctx, dirs, Create::Missing)?;
        parent.insert(name.clone(), Slot::Kept(node));
        Ok(())
    }

    pub(crate) fn mkdir(&mut self, ctx: &QueryCtx, segments: &[String]) -> Result<(), Error> {
        let (name, dirs) = split(segments)?;
        let parent = open_dir(&mut self.root, ctx, dirs, Create::Missing)?;
        if parent.contains_key(name) {
            return Err(already_exists(format!("{} exists", join(segments))));
        }
        parent.insert(name.clone(), Slot::Open(BTreeMap::new()));
        Ok(())
    }

    pub(crate) fn remove(&mut self, ctx: &QueryCtx, segments: &[String]) -> Result<(), Error> {
        let (name, dirs) = split(segments)?;
        let parent = open_dir(&mut self.root, ctx, dirs, Create::Never)?;
        if parent.remove(name).is_none() {
            return Err(not_found(format!("nothing is at {}", join(segments))));
        }
        Ok(())
    }

    pub(crate) fn rename(
        &mut self,
        ctx: &QueryCtx,
        from: &[String],
        to: &[String],
    ) -> Result<(), Error> {
        let (from_name, from_dirs) = split(from)?;
        let (to_name, to_dirs) = split(to)?;
        let into_itself = to.len() >= from.len() && to[..from.len()] == *from;
        if into_itself {
            return Err(invalid(format!("{} cannot move into itself", join(from))));
        }
        let source = open_dir(&mut self.root, ctx, from_dirs, Create::Never)?;
        if !source.contains_key(from_name) {
            return Err(not_found(format!("nothing is at {}", join(from))));
        }
        let target = open_dir(&mut self.root, ctx, to_dirs, Create::Never)?;
        if target.contains_key(to_name) {
            return Err(already_exists(format!("{} exists", join(to))));
        }
        let moved = open_dir(&mut self.root, ctx, from_dirs, Create::Never)?
            .remove(from_name)
            .expect("checked present above");
        open_dir(&mut self.root, ctx, to_dirs, Create::Never)?.insert(to_name.clone(), moved);
        Ok(())
    }

    pub(crate) fn build(self, ctx: &ExecCtx) -> Result<(BlobId, u64), Error> {
        match self.root {
            Slot::Kept(Node::Dir { tree, .. }) => {
                let entries = crate::objects::tree(ctx, &tree)?.entries.len() as u64;
                Ok((tree, entries))
            }
            Slot::Kept(other) => Err(guest::corrupt(
                crate::TREE,
                &[],
                format!("the root is a {}", other.kind()),
            )),
            Slot::Open(children) => write(ctx, children),
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Create {
    Missing,
    Never,
}

fn split(segments: &[String]) -> Result<(&String, &[String]), Error> {
    segments
        .split_last()
        .ok_or_else(|| invalid("the root is not a path a change names"))
}

fn open(slot: &mut Slot, ctx: &QueryCtx, path: &[String]) -> Result<(), Error> {
    let Slot::Kept(node) = slot else {
        return Ok(());
    };
    let Node::Dir { tree, .. } = node else {
        return Err(wrong_state(format!(
            "{} is a {}, not a directory",
            join(path),
            node.kind()
        )));
    };
    let children = crate::objects::tree(ctx, tree)?
        .entries
        .into_iter()
        .map(|(name, node)| (name, Slot::Kept(node)))
        .collect();
    *slot = Slot::Open(children);
    Ok(())
}

fn open_dir<'e>(
    root: &'e mut Slot,
    ctx: &QueryCtx,
    dirs: &[String],
    create: Create,
) -> Result<&'e mut BTreeMap<String, Slot>, Error> {
    let mut slot = root;
    for (depth, name) in dirs.iter().enumerate() {
        open(slot, ctx, &dirs[..depth])?;
        let Slot::Open(children) = slot else {
            unreachable!("open leaves a directory open");
        };
        let missing = !children.contains_key(name);
        if missing && create == Create::Never {
            return Err(not_found(format!("no directory {}", join(&dirs[..=depth]))));
        }
        slot = children
            .entry(name.clone())
            .or_insert_with(|| Slot::Open(BTreeMap::new()));
    }
    open(slot, ctx, dirs)?;
    let Slot::Open(children) = slot else {
        unreachable!("open leaves a directory open");
    };
    Ok(children)
}

fn write(ctx: &ExecCtx, children: BTreeMap<String, Slot>) -> Result<(BlobId, u64), Error> {
    let mut entries = BTreeMap::new();
    for (name, slot) in children {
        let node = match slot {
            Slot::Kept(node) => node,
            Slot::Open(grandchildren) => {
                let (tree, entries) = write(ctx, grandchildren)?;
                Node::Dir { tree, entries }
            }
        };
        entries.insert(name, node);
    }
    let count = entries.len() as u64;
    let id = put_tree(ctx, &crate::Tree { entries })?;
    Ok((id, count))
}
