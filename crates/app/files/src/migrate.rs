//! The one-time move of a network whose write lists rode in files' trees, each list stored as an
//! owner, account numbers or anyone: the vote copies each list at the head into the access
//! program, under its path, and commits the head again with no list in its trees. Older
//! snapshots keep theirs as they were. Deleted once every network ran it.
use std::collections::BTreeMap;

use ::program::{AccountNumber, BlobId, Error, ExecCtx, Ownership, Party, Principal, Reply};
use borsh::{BorshDeserialize, BorshSerialize};

use crate::Access;
use crate::Node;
use crate::commit::record;
use crate::objects::{SnapshotObj, TREE, Tree, get, put_snapshot, put_tree};
use crate::paths::join;
use crate::state::HEAD;

#[derive(BorshSerialize, BorshDeserialize)]
pub(crate) enum Stored {
    Owner,
    Accounts(Vec<AccountNumber>),
    Anyone,
}

impl Stored {
    fn listed(self) -> Ownership {
        match self {
            Stored::Owner => Ownership::Owner,
            Stored::Accounts(accounts) => {
                Ownership::Listed(accounts.into_iter().map(Party::Account).collect())
            }
            Stored::Anyone => Ownership::Anyone,
        }
    }
}

#[derive(BorshSerialize, BorshDeserialize)]
pub(crate) struct StoredAccess {
    pub(crate) creator: Option<AccountNumber>,
    pub(crate) write: Option<Stored>,
}

#[derive(BorshSerialize, BorshDeserialize)]
pub(crate) struct StoredTree {
    pub(crate) entries: BTreeMap<String, Node>,
    pub(crate) access: BTreeMap<String, StoredAccess>,
}

pub(crate) fn migrate(ctx: &ExecCtx) -> Result<(), Error> {
    let env = ctx.env();
    env.authority()?;
    let Some(head) = HEAD.get(ctx)? else {
        return Ok(());
    };
    let mut lists = Vec::new();
    let root = rewrite(ctx, head.root, &mut Vec::new(), &mut lists)?;
    if lists.is_empty() {
        return Ok(());
    }
    let snapshot = SnapshotObj {
        root,
        parent: Some(head.id),
        author: Principal::Root,
        height: env.height,
        time: env.time,
        message: "the write lists move to the access program".into(),
    };
    let id = put_snapshot(ctx, &snapshot)?;
    record(ctx, id, snapshot)?;
    let access = env.role(abi::role::ACCESS)?.clone();
    for (path, list) in lists {
        let set = access::Op::Set {
            program: crate::PROGRAM.into(),
            key: path,
            list: Some(list),
        };
        ctx.emit(access.clone(), &set, Reply::None);
    }
    Ok(())
}

fn rewrite(
    ctx: &ExecCtx,
    id: BlobId,
    at: &mut Vec<String>,
    lists: &mut Vec<(String, Ownership)>,
) -> Result<BlobId, Error> {
    let stored: StoredTree = get(ctx, TREE, &id)?;
    let mut access = BTreeMap::new();
    for (name, row) in stored.access {
        if let Some(list) = row.write {
            at.push(name.clone());
            lists.push((join(at), list.listed()));
            at.pop();
        }
        let kept = Access {
            creator: row.creator,
            write: None,
        };
        access.insert(name, kept);
    }
    let mut entries = BTreeMap::new();
    for (name, node) in stored.entries {
        let node = match node {
            Node::Dir { tree, entries } => {
                at.push(name.clone());
                let tree = rewrite(ctx, tree, at, lists)?;
                at.pop();
                Node::Dir { tree, entries }
            }
            other => other,
        };
        entries.insert(name, node);
    }
    put_tree(ctx, &Tree { entries, access })
}
