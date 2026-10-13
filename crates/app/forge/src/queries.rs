//! The repository answers [`Forge::query`](crate::Forge) names, and git's
//! own bytes for a git client.

use gitcore::wire::receive::advertise_refs;
use gitcore::wire::smart_http_service_header;
use gitcore::wire::upload::{
    Command, capability_advertisement, fetch, ls_refs_response, parse_command,
};
use guest::Error;
use guest::{QueryCtx, stale};
use store::Listing;

use std::collections::{BTreeSet, VecDeque};

use gitcore::{Commit, Kind, Objects as _, Oid};

use crate::contract::*;
use crate::objects::{ObjectStore, Stat};
use crate::ops::{cap, refusal_of};
use crate::state::{
    ACTIVITY, REFS, load_bounds, load_refs, load_repo, parse_oid, repo_hash, storage, writes,
};

const AGENT: &[u8] = b"ducktape-forge";

/// Repositories, the most recently active first.
pub(crate) fn repos(ctx: &QueryCtx, listing: &Listing) -> Result<PageResponse<RepoInfo>, Error> {
    ACTIVITY.page_of(ctx, &(), listing)?.try_map(|(_, name)| {
        Ok(RepoInfo {
            repo: load_repo(ctx, &name)?,
            name,
        })
    })
}

/// A repository's refs in byte-name order.
pub(crate) fn refs(
    ctx: &QueryCtx,
    name: &str,
    listing: &Listing,
) -> Result<PageResponse<RefInfo>, Error> {
    let hash = repo_hash(&load_repo(ctx, name)?);
    REFS.page_of(ctx, &name.to_owned(), listing)?
        .try_map(|((_, name), bytes)| {
            let oid = gitcore::Oid::from_bytes(hash, &bytes).map_err(|e| storage(e.to_string()))?;
            Ok(RefInfo {
                name,
                target: oid.to_hex(),
            })
        })
}

/// A forge listing is rewritten only by a forge op, so a cursor is pinned
/// to the count of ops accepted when it was answered and is good until
/// forge next writes: a walk crosses the blocks that wrote nothing here,
/// and a push mid-walk restarts it, even one in the block that answered
/// the cursor (the preconfirmed layer answers at the height its ops run
/// at, so a height alone would not tell).
pub(crate) fn listing(ctx: &QueryCtx, page: PageRequest, scope: Vec<u8>) -> Result<Listing, Error> {
    let writes = writes(ctx)?;
    let listing = page.listing(ctx, scope)?.pinned(writes);
    if listing.cursor_pin.is_some_and(|pinned| pinned != writes) {
        return Err(stale("the listing changed; restart it"));
    }
    Ok(listing)
}

pub(crate) fn advertise(ctx: &QueryCtx, name: &str, service: Service) -> Result<Vec<u8>, Error> {
    let repo = load_repo(ctx, name)?;
    let hash = repo_hash(&repo);
    let body = match service {
        Service::ReceivePack => {
            let refs = load_refs(ctx, name, hash)?;
            let object_format = format!("object-format={}", hash.name());
            let agent = format!("agent={}", String::from_utf8_lossy(AGENT));
            advertise_refs(
                &refs,
                hash,
                &[
                    b"report-status",
                    b"delete-refs",
                    b"side-band-64k",
                    b"ofs-delta",
                    object_format.as_bytes(),
                    agent.as_bytes(),
                ],
            )
        }
        Service::UploadPack => {
            let mut body = smart_http_service_header(b"git-upload-pack");
            body.extend_from_slice(&capability_advertisement(hash, AGENT));
            body
        }
    };
    Ok(body)
}

pub(crate) fn upload(ctx: &QueryCtx, name: &str, request: &[u8]) -> Result<Vec<u8>, Error> {
    let repo = load_repo(ctx, name)?;
    let bounds = load_bounds(ctx)?;
    let hash = repo_hash(&repo);
    let refs = load_refs(ctx, name, hash)?;
    let objects = ObjectStore::new(ctx, hash, &bounds);
    let Some(command) = parse_command(request, hash).map_err(refusal_of)? else {
        return Ok(Vec::new());
    };
    let mut response = Vec::new();
    let served = match command {
        Command::LsRefs(command) => ls_refs_response(
            &objects,
            &refs,
            Some(&repo.settings.head),
            &command,
            cap(bounds.fetch_walk),
        )
        .map(|body| response = body),
        Command::Fetch(command) => fetch(
            &objects,
            &refs,
            &command,
            cap(bounds.fetch_walk),
            &mut |chunk| response.extend_from_slice(chunk),
        ),
    };
    served.map_err(refusal_of)?;
    Ok(response)
}

/// Git objects by id ([`Query::Objects`]): the ids in order while the read
/// budget lasts (the first held one past it), then, asked for, the commits
/// they descend from, breadth first, while it still lasts. An object whose
/// pack would be one more than a query loads
/// ([`MAX_PACKS`](crate::objects::MAX_PACKS)) ends the batch as the budget
/// does.
pub(crate) fn objects(
    ctx: &QueryCtx,
    height: u64,
    bounds: &Bounds,
    name: &str,
    ids: &[String],
    ancestors: bool,
) -> Result<Reply, Error> {
    let hash = repo_hash(&load_repo(ctx, name)?);
    let store = ObjectStore::new(ctx, hash, bounds);
    let (mut reads, mut bytes) = (bounds.tree_walk, bounds.diff_bytes);
    let mut objects = Vec::new();
    let mut missing = Vec::new();
    let mut parents = VecDeque::new();
    let mut known = BTreeSet::new();
    for hex in ids {
        let id = parse_oid(hash, hex)?;
        let stat = store.stat(&id).map_err(refusal_of)?;
        let past = |stat: &Stat| stat.len > bytes || store.loads_past_cap(stat);
        let spent = reads == 0
            || stat
                .as_ref()
                .is_some_and(|stat| past(stat) && !objects.is_empty());
        if spent {
            return Ok(Reply::Objects {
                height,
                objects,
                missing,
            });
        }
        reads -= 1;
        known.insert(id);
        let Some(stat) = stat else {
            missing.push(hex.clone());
            continue;
        };
        bytes = bytes.saturating_sub(stat.len);
        let object = read(&store, &id)?;
        if ancestors && object.kind == ObjectKind::Commit {
            parents.extend(
                Commit::parse(&object.body, hash)
                    .map_err(refusal_of)?
                    .parents,
            );
        }
        objects.push(object);
    }
    while let Some(id) = parents.pop_front() {
        if !known.insert(id) {
            continue;
        }
        let Some(stat) = store.stat(&id).map_err(refusal_of)? else {
            continue;
        };
        if reads == 0 || stat.len > bytes || store.loads_past_cap(&stat) {
            break;
        }
        reads -= 1;
        bytes -= stat.len;
        let object = read(&store, &id)?;
        if object.kind == ObjectKind::Commit {
            parents.extend(
                Commit::parse(&object.body, hash)
                    .map_err(refusal_of)?
                    .parents,
            );
        }
        objects.push(object);
    }
    Ok(Reply::Objects {
        height,
        objects,
        missing,
    })
}

fn read(store: &ObjectStore<'_>, id: &Oid) -> Result<GitObject, Error> {
    let object = store
        .get(id)
        .map_err(refusal_of)?
        .ok_or_else(|| crate::objects::object_not_held(id))?;
    let kind = match object.kind {
        Kind::Commit => ObjectKind::Commit,
        Kind::Tree => ObjectKind::Tree,
        Kind::Blob => ObjectKind::Blob,
        Kind::Tag => ObjectKind::Tag,
    };
    Ok(GitObject {
        oid: id.to_hex(),
        kind,
        body: object.body,
    })
}
