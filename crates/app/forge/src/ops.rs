//! The execute path: who acts, which op, and the repository ops (create,
//! configure, grant, revoke, push, stage). Change ops live in `changes`.

use ::program::{BlobId, Error, HashKind};
use ::program::{
    ExecCtx, QueryCtx, already_exists, capacity, decoded, invalid, not_found, unauthorized,
};
use gitcore::server::{Policy, RefUpdate};
use gitcore::{Error as GitError, Limits, server};

use crate::contract::{
    Bounds, MAX_PATH_BYTES, PACK_KIND, Principal, Repo, Settings, valid_repo_name,
};
use crate::objects::{ObjectWriter, object_not_held};
use crate::state::{
    WRITERS, delete_ref, is_writer, load_bounds, load_refs, load_repo, repo_exists, repo_hash,
    save_bounds, save_repo, set_ref, storage,
};

pub const PROGRAM: &str = "forge";

pub(crate) fn init(ctx: &ExecCtx, params: &[u8]) -> Result<(), Error> {
    let bounds: Bounds = decoded(PROGRAM, "Bounds", params)?;
    let usable = bounds.page_size > 0
        && bounds.log_walk > 0
        && bounds.tree_walk > 0
        && bounds.blob_bytes > 0
        && bounds.record_bytes > 0
        && bounds.diff_bytes >= bounds.blob_bytes;
    if !usable {
        return Err(invalid(
            "bounds need a positive page size and positive read/record budgets; diff_bytes >= blob_bytes",
        ));
    }
    save_bounds(ctx, &bounds);
    Ok(())
}

/// Forge is written by keys (a person's or an agent's account), never by a
/// program or the system: the frame is signed, and acts as the account its
/// key holds. The host rejects a frame whose key's account is not live
/// before it gets here; [`ExecCtx::sender`](program::ExecCtx::sender) refuses
/// a key that holds no account.
pub(crate) fn signed_account(ctx: &ExecCtx) -> Result<Principal, Error> {
    ctx.env()
        .signer()
        .map_err(|_| unauthorized("a repository op is signed by a key"))?;
    ctx.sender()
}

/// Every accepted op marks its repository active at this height and counts
/// as a write, which every listing cursor is pinned to.
pub(crate) fn touch(ctx: &ExecCtx, name: &str, height: u64) -> Result<(), Error> {
    let mut repo = load_repo(ctx, name)?;
    repo.last_activity = height;
    save_repo(ctx, name, &repo)?;
    crate::state::wrote(ctx)
}

pub(crate) fn create(
    ctx: &ExecCtx,
    actor: &Principal,
    name: &str,
    hash: HashKind,
) -> Result<(), Error> {
    if !valid_repo_name(name) {
        return Err(invalid(format!("{name:?} is not a repository name")));
    }
    if repo_exists(ctx, name) {
        return Err(already_exists(format!("a repository named {name} exists")));
    }
    let repo = Repo {
        hash,
        owner: actor.clone(),
        settings: Settings::default(),
        refs_count: 0,
        last_activity: 0,
    };
    save_repo(ctx, name, &repo)
}

pub(crate) fn configure(
    ctx: &ExecCtx,
    actor: &Principal,
    name: &str,
    settings: Settings,
) -> Result<(), Error> {
    let mut repo = load_repo(ctx, name)?;
    require_owner(&repo, actor)?;
    let head_is_a_ref =
        settings.head.len() <= MAX_PATH_BYTES && server::valid_ref_name(&settings.head);
    if !head_is_a_ref {
        return Err(invalid("head names a ref under refs/"));
    }
    repo.settings = settings;
    save_repo(ctx, name, &repo)
}

pub(crate) fn grant(
    ctx: &ExecCtx,
    actor: &Principal,
    name: &str,
    principal: Principal,
) -> Result<(), Error> {
    require_owner(&load_repo(ctx, name)?, actor)?;
    require_person_or_agent(ctx, &principal)?;
    WRITERS.insert(ctx, &(name.to_owned(), principal));
    Ok(())
}

pub(crate) fn revoke(
    ctx: &ExecCtx,
    actor: &Principal,
    name: &str,
    principal: Principal,
) -> Result<(), Error> {
    require_owner(&load_repo(ctx, name)?, actor)?;
    require_named(&principal)?;
    WRITERS.remove(ctx, &(name.to_owned(), principal));
    Ok(())
}

/// A git receive-pack: once the pack unpacks, its objects land as blobs and
/// each accepted ref moves; git's own report is the op's output. The
/// objects land even when every ref is refused, as git's receive-pack keeps
/// what it unpacked: a stage stores objects no ref reaches as well, and a
/// client that pushes again after a refusal (a branch moved meanwhile)
/// sends less. A pack that does not unpack stores nothing.
pub(crate) fn push(
    ctx: &ExecCtx,
    actor: &Principal,
    name: &str,
    request: &[u8],
    pack: Option<BlobId>,
) -> Result<(), Error> {
    let mut repo = load_repo(ctx, name)?;
    require_writer(ctx, name, &repo, actor)?;
    let pack = match pack {
        Some(id) => held_pack(ctx, id)?,
        None => Vec::new(),
    };
    let bounds = load_bounds(ctx)?;
    let hash = repo_hash(&repo);
    let refs = load_refs(ctx, name, hash)?;
    let policy = Policy {
        allow_force: repo.settings.allow_force,
        allow_delete: repo.settings.allow_delete,
    };
    let mut objects = ObjectWriter::new(ctx, hash);
    let outcome = server::push(
        &mut objects,
        &refs,
        request,
        &pack,
        hash,
        &limits_of(&bounds),
        &policy,
        cap(bounds.push_walk),
    )
    .map_err(refusal_of)?;
    if outcome.unpack_error.is_none() {
        objects.flush()?;
    }
    for (reference, update) in &outcome.moves {
        match update {
            RefUpdate::Set(target) => {
                if !refs.contains_key(reference) {
                    repo.refs_count += 1;
                }
                set_ref(ctx, name, reference, target);
            }
            RefUpdate::Delete => {
                repo.refs_count -= 1;
                delete_ref(ctx, name, reference);
            }
        }
    }
    save_repo(ctx, name, &repo)?;
    ctx.set_return_data(outcome.report);
    Ok(())
}

/// The objects half of a push: the pack's objects land as blobs, checked as
/// a push checks them (bounds, deltas against what is held, every object
/// they name held), and no ref moves.
pub(crate) fn stage(
    ctx: &ExecCtx,
    actor: &Principal,
    name: &str,
    pack: BlobId,
) -> Result<(), Error> {
    let repo = load_repo(ctx, name)?;
    require_writer(ctx, name, &repo, actor)?;
    let pack = held_pack(ctx, pack)?;
    let bounds = load_bounds(ctx)?;
    let hash = repo_hash(&repo);
    let mut objects = ObjectWriter::new(ctx, hash);
    server::admit_pack(&mut objects, &pack, hash, &limits_of(&bounds)).map_err(refusal_of)?;
    objects.flush()
}

fn held_pack(ctx: &ExecCtx, id: BlobId) -> Result<Vec<u8>, Error> {
    let Some(blob) = ctx.blob_get(id) else {
        return Err(not_found(format!(
            "pack {id:?} is not held: put it as a {PACK_KIND:?} blob and wait for a quorum of validators to hold it"
        )));
    };
    if blob.kind != PACK_KIND {
        return Err(invalid(format!(
            "pack {id:?} is a {:?} blob, not {PACK_KIND:?}",
            blob.kind
        )));
    }
    Ok(blob.body)
}

fn require_owner(repo: &Repo, actor: &Principal) -> Result<(), Error> {
    if repo.owner != *actor {
        return Err(unauthorized("only the owner changes a repository"));
    }
    Ok(())
}

pub(crate) fn require_writer(
    ctx: &QueryCtx,
    name: &str,
    repo: &Repo,
    actor: &Principal,
) -> Result<(), Error> {
    let may_write = repo.owner == *actor || is_writer(ctx, name, actor);
    if !may_write {
        return Err(unauthorized(
            "only the owner and its writers push, merge and close",
        ));
    }
    Ok(())
}

/// Whom an op names (a writer, a reviewer): an account, never the system.
pub(crate) fn require_named(principal: &Principal) -> Result<(), Error> {
    if principal.account().is_none() {
        return Err(invalid("only an account is named here"));
    }
    Ok(())
}

/// Whom a person asks to write or review: an account the identity role
/// profiles as a person or an agent that acts.
pub(crate) fn require_person_or_agent(ctx: &QueryCtx, principal: &Principal) -> Result<(), Error> {
    let Some(number) = principal.account() else {
        return Err(invalid("only an account is named here"));
    };
    ctx.require_person_or_agent(number)
}

pub fn limits_of(bounds: &Bounds) -> Limits {
    Limits {
        max_objects: cap(bounds.max_objects),
        max_delta_depth: cap(bounds.max_delta_depth),
        max_object_size: cap(bounds.max_object_size),
    }
}

pub fn cap(bound: u64) -> usize {
    usize::try_from(bound).unwrap_or(usize::MAX)
}

pub fn refusal_of(error: GitError) -> Error {
    match error {
        GitError::Storage => storage("the blob ctx refused a write"),
        GitError::CapReached | GitError::ObjectTooLarge => {
            capacity("query or operation exceeds its configured work/byte bound")
        }
        GitError::MissingObject(id) | GitError::MissingBase(id) => object_not_held(id),
        other => invalid(other.to_string()),
    }
}
