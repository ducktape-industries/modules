//! `gitcore::Objects` over forge's packs: each pack a push or a stage
//! brought is one blob, kept as git compressed it, and each object it
//! brought is indexed at its entry ([`PACKED`]); a read takes the bytes its
//! delta chain spans, never the pack. Objects forge stored before packs
//! are blobs whose id is the oid, read as they are.

use std::cell::{Cell, RefCell};
use std::collections::BTreeMap;

use gitcore::pack::{self, Base, Payload};
use gitcore::{Error as GitError, Hash, Kind, Object, Objects, Oid};
use guest::{BlobHeader, BlobId, Error, HashKind};
use guest::{ExecCtx, QueryCtx, capacity, not_found};

use crate::contract::Bounds;
use crate::state::{Location, NEXT_PACK, PACKED, PACKS, storage};

/// A different serving node or object replication can satisfy this query.
pub(crate) fn object_not_held(oid: impl std::fmt::Display) -> Error {
    not_found(format!("object {oid} is not held by this node"))
}

/// What a presence check and a read budget need of an object: its kind
/// and its size whole.
#[derive(Clone, Copy, Debug)]
pub struct Stat {
    pub kind: Kind,
    pub len: u64,
}

/// Reads objects; a `put` computes the id and persists nothing (a compare
/// may build a prospective tree off consensus). A push writes through
/// [`ObjectWriter`].
pub struct ObjectStore<'a> {
    packs: Packs<'a>,
    hash: Hash,
    budget: Option<Cell<(u64, u64)>>,
    max_object_size: u64,
}

impl<'a> ObjectStore<'a> {
    pub fn new(sandbox: &'a QueryCtx, hash: Hash, bounds: &Bounds) -> ObjectStore<'a> {
        ObjectStore {
            packs: Packs::new(sandbox, hash, bounds),
            hash,
            budget: None,
            max_object_size: u64::MAX,
        }
    }

    pub fn querying(sandbox: &'a QueryCtx, hash: Hash, bounds: &Bounds, reads: u64) -> Self {
        Self {
            budget: Some(Cell::new((reads, bounds.diff_bytes))),
            max_object_size: bounds.max_object_size,
            ..Self::new(sandbox, hash, bounds)
        }
    }

    pub fn header(&self, id: &Oid) -> gitcore::Result<BlobHeader> {
        let stat = self.stat(id)?.ok_or(GitError::MissingObject(*id))?;
        Ok(BlobHeader {
            kind: stat.kind.as_str().to_owned(),
            len: stat.len,
        })
    }

    pub fn stat(&self, id: &Oid) -> gitcore::Result<Option<Stat>> {
        self.packs.stat(id)
    }
}

impl Objects for ObjectStore<'_> {
    fn get(&self, id: &Oid) -> gitcore::Result<Option<Object>> {
        if let Some(budget) = &self.budget {
            let Some(stat) = self.packs.stat(id)? else {
                return Ok(None);
            };
            let (reads, bytes) = budget.get();
            if reads == 0 || stat.len > bytes || stat.len > self.max_object_size {
                return Err(GitError::CapReached);
            }
            budget.set((reads - 1, bytes - stat.len));
        }
        self.packs.get(id)
    }

    fn has(&self, id: &Oid) -> gitcore::Result<bool> {
        Ok(self.packs.stat(id)?.is_some())
    }

    fn put(&mut self, kind: Kind, body: &[u8]) -> gitcore::Result<Oid> {
        gitcore::oid_of(self.hash, kind, body)
    }
}

/// The reads both stores share: an object from its pack, through the index,
/// or as a loose blob.
struct Packs<'a> {
    ctx: &'a QueryCtx,
    hash: Hash,
    max_depth: usize,
    max_size: usize,
    /// The blob of each pack this store read from.
    blobs: RefCell<BTreeMap<u32, BlobId>>,
}

impl<'a> Packs<'a> {
    fn new(ctx: &'a QueryCtx, hash: Hash, bounds: &Bounds) -> Self {
        Packs {
            ctx,
            hash,
            max_depth: crate::ops::cap(bounds.max_delta_depth),
            max_size: crate::ops::cap(bounds.max_object_size),
            blobs: RefCell::default(),
        }
    }

    fn locate(&self, id: &Oid) -> gitcore::Result<Option<Location>> {
        PACKED
            .get(self.ctx, &id.as_bytes().to_vec())
            .map_err(|_| GitError::Storage)
    }

    fn stat(&self, id: &Oid) -> gitcore::Result<Option<Stat>> {
        if let Some(at) = self.locate(id)? {
            let kind = pack::kind_from_code(at.kind).ok_or(GitError::Storage)?;
            return Ok(Some(Stat { kind, len: at.size }));
        }
        let Some(header) = self.ctx.blob_stat(blob_id_of(id)) else {
            return Ok(None);
        };
        Ok(Some(Stat {
            kind: Kind::parse(header.kind.as_bytes())?,
            len: header.len,
        }))
    }

    fn get(&self, id: &Oid) -> gitcore::Result<Option<Object>> {
        match self.locate(id)? {
            Some(at) => self.unpack(id, at).map(Some),
            None => {
                let Some(blob) = self.ctx.blob_get(blob_id_of(id)) else {
                    return Ok(None);
                };
                let kind = Kind::parse(blob.kind.as_bytes())?;
                Ok(Some(Object::new(kind, blob.body)))
            }
        }
    }

    /// The object at `at`, its delta chain applied, read from the bytes of
    /// its pack the chain spans (`at.start..at.end`).
    /// [`ObjectWriter::flush`] indexes an object in a pack only when its
    /// whole chain reads inside that pack, so the chain is no deeper than
    /// the pack was let have; one that leaves the span is a corrupt index.
    fn unpack(&self, id: &Oid, at: Location) -> gitcore::Result<Object> {
        let start = at.start as usize;
        let len = at.end.checked_sub(at.start).ok_or(GitError::Storage)?;
        let window = self
            .ctx
            .blob_read(self.blob(at.pack)?, u64::from(at.start), u64::from(len))
            .ok_or(GitError::MissingObject(*id))?;
        if window.len() != len as usize {
            return Err(GitError::Storage);
        }
        let in_window = |offset: u32| {
            (offset as usize)
                .checked_sub(start)
                .filter(|offset| *offset < window.len())
                .ok_or(GitError::Storage)
        };
        let mut deltas = Vec::new();
        let mut offset = in_window(at.offset)?;
        let (kind, mut body) = loop {
            if deltas.len() > self.max_depth {
                return Err(GitError::DeltaTooDeep);
            }
            match pack::entry_at(&window, offset, self.hash, self.max_size)?.0 {
                Payload::Full(kind, body) => break (kind, body),
                Payload::OfsDelta { base_offset, delta } => {
                    deltas.push(delta);
                    offset = base_offset;
                }
                Payload::RefDelta { base, delta } => {
                    deltas.push(delta);
                    let base = self.locate(&base)?.filter(|base| base.pack == at.pack);
                    offset = in_window(base.ok_or(GitError::Storage)?.offset)?;
                }
            }
        };
        for delta in deltas.iter().rev() {
            body = pack::apply_delta(&body, delta, self.max_size)?;
        }
        let as_indexed = pack::kind_code(kind) == at.kind && body.len() as u64 == at.size;
        if !as_indexed {
            return Err(GitError::Storage);
        }
        Ok(Object::new(kind, body))
    }

    /// The blob that holds pack `number`.
    fn blob(&self, number: u32) -> gitcore::Result<BlobId> {
        if let Some(blob) = self.blobs.borrow().get(&number) {
            return Ok(*blob);
        }
        let blob = PACKS
            .get(self.ctx, &number)
            .map_err(|_| GitError::Storage)?
            .ok_or(GitError::Storage)?;
        self.blobs.borrow_mut().insert(number, blob);
        Ok(blob)
    }
}

/// The object store a push or a stage writes into. Objects wait here until
/// the whole pack is admitted ([`ObjectWriter::flush`]), so a pack that
/// does not unpack, and an op that is refused, store nothing.
// ponytail: holds a pack's objects in memory until admitted; the pack is
// in memory already and `Bounds.max_objects`/`max_object_size` cap both.
pub struct ObjectWriter<'a> {
    sandbox: &'a ExecCtx,
    packs: Packs<'a>,
    hash: Hash,
    pending: BTreeMap<Oid, (Kind, Vec<u8>)>,
    /// Each `put`, in order: `server::admit_pack` puts a pack's objects in
    /// the order `pack::read` lists them, the pack's own entry order.
    order: Vec<Oid>,
}

impl<'a> ObjectWriter<'a> {
    pub fn new(sandbox: &'a ExecCtx, hash: Hash, bounds: &Bounds) -> Self {
        ObjectWriter {
            sandbox,
            packs: Packs::new(sandbox, hash, bounds),
            hash,
            pending: BTreeMap::new(),
            order: Vec::new(),
        }
    }

    /// Stores what the admitted `pack` brought that forge does not hold:
    /// the pack itself, as one blob, indexing each new object whose delta
    /// chain reads inside it; and the rest whole, in one pack forge writes
    /// (no deltas), as are objects that came in no pack (an empty `pack`).
    /// A delta against an object forge held before (a thin pack's, or one
    /// in an earlier part of a split push) goes whole, so a read never
    /// follows a chain from pack to pack. An object forge already holds is
    /// not indexed again: the first pack that brought it keeps it, and the
    /// later pack's copy stays in its bytes unread. A pack that adds
    /// nothing is not stored.
    pub fn flush(self, pack: &[u8]) -> Result<(), Error> {
        let mut new = BTreeMap::new();
        for (index, id) in self.order.iter().enumerate() {
            if !new.contains_key(id) && self.packs.stat(id).map_err(read_error)?.is_none() {
                new.insert(*id, index);
            }
        }
        let entries = match pack.is_empty() {
            true => Vec::new(),
            false => self.entries(pack)?,
        };
        if !entries.is_empty() && entries.len() != self.order.len() {
            return Err(storage(
                "the admitted pack's entries and its objects differ",
            ));
        }
        let mut memo = vec![None; entries.len()];
        let mut inside = Vec::new();
        let mut whole = Vec::new();
        for (id, index) in &new {
            match entries.is_empty() {
                true => whole.push(*id),
                false => match span(*index, &entries, &new, &mut memo) {
                    Some(span) => inside.push((*id, entries[*index].offset, span)),
                    None => whole.push(*id),
                },
            }
        }
        if !inside.is_empty() {
            self.keep(pack.to_vec(), inside)?;
        }
        if !whole.is_empty() {
            let objects = whole.iter().map(|id| {
                let (kind, body) = &self.pending[id];
                Object::new(*kind, body.clone())
            });
            let written = pack::write(objects, self.hash).map_err(read_error)?;
            let placed = whole
                .iter()
                .zip(self.entries(&written)?)
                .map(|(id, entry)| (*id, entry.offset, (entry.offset, entry.end)))
                .collect();
            self.keep(written, placed)?;
        }
        Ok(())
    }

    /// Each entry of an admitted pack: where it starts and ends, and what
    /// it is read from.
    fn entries(&self, pack: &[u8]) -> Result<Vec<Entry>, Error> {
        let listed = pack::entries(pack, self.hash, self.packs.max_size).map_err(read_error)?;
        let last = pack.len() - self.hash.size();
        let ends = listed.iter().skip(1).map(|(offset, _)| *offset);
        Ok(listed
            .iter()
            .zip(ends.chain([last]))
            .map(|((offset, base), end)| Entry {
                offset: *offset,
                end,
                base: *base,
            })
            .collect())
    }

    /// Stores `bytes` as pack blob number [`NEXT_PACK`] and indexes each
    /// placed object at its entry, with the span its chain reads.
    fn keep(&self, bytes: Vec<u8>, placed: Vec<(Oid, usize, (usize, usize))>) -> Result<(), Error> {
        let number = NEXT_PACK.get(self.sandbox)?.unwrap_or(0);
        let next = number
            .checked_add(1)
            .ok_or_else(|| Error::new(guest::code::EXHAUSTED, "pack counter exhausted"))?;
        let at =
            |offset: usize| u32::try_from(offset).map_err(|_| capacity("a pack is at most 4 GiB"));
        let mut rows = Vec::with_capacity(placed.len());
        for (id, offset, (start, end)) in placed {
            let (kind, body) = &self.pending[&id];
            let location = Location {
                pack: number,
                offset: at(offset)?,
                start: at(start)?,
                end: at(end)?,
                kind: pack::kind_code(*kind),
                size: body.len() as u64,
            };
            rows.push((id.as_bytes().to_vec(), location));
        }
        let blob = self
            .sandbox
            .blob_put(hash_kind_of(self.hash), "pack", bytes)?;
        PACKS.put(self.sandbox, &number, &blob);
        NEXT_PACK.put(self.sandbox, &next);
        for (key, location) in rows {
            PACKED.put(self.sandbox, &key, &location);
        }
        Ok(())
    }
}

/// An entry of an admitted pack.
#[derive(Clone, Copy)]
struct Entry {
    offset: usize,
    end: usize,
    base: Base,
}

/// The bytes entry `index`'s chain spans when it reads inside its pack:
/// whole, or a delta whose base does (an offset of the pack, or an object
/// new in it that does); `None` when it does not. An entry being decided
/// does not (no cycle survives admission anyway).
fn span(
    index: usize,
    entries: &[Entry],
    new: &BTreeMap<Oid, usize>,
    memo: &mut Vec<Option<Option<(usize, usize)>>>,
) -> Option<(usize, usize)> {
    if let Some(known) = memo[index] {
        return known;
    }
    memo[index] = Some(None);
    let entry = entries[index];
    let base = match entry.base {
        Base::Whole => None,
        Base::Offset(offset) => Some(entries.binary_search_by_key(&offset, |e| e.offset).ok()),
        Base::Object(id) => Some(new.get(&id).copied()),
    };
    let span = match base {
        None => Some((entry.offset, entry.end)),
        Some(None) => None,
        Some(Some(base)) => span(base, entries, new, memo)
            .map(|(start, end)| (start.min(entry.offset), end.max(entry.end))),
    };
    memo[index] = Some(span);
    span
}

fn read_error(error: GitError) -> Error {
    crate::ops::refusal_of(error)
}

impl Objects for ObjectWriter<'_> {
    fn get(&self, id: &Oid) -> gitcore::Result<Option<Object>> {
        if let Some((kind, body)) = self.pending.get(id) {
            return Ok(Some(Object::new(*kind, body.clone())));
        }
        self.packs.get(id)
    }

    fn has(&self, id: &Oid) -> gitcore::Result<bool> {
        Ok(self.pending.contains_key(id) || self.packs.stat(id)?.is_some())
    }

    fn put(&mut self, kind: Kind, body: &[u8]) -> gitcore::Result<Oid> {
        let id = gitcore::oid_of(self.hash, kind, body)?;
        self.pending.insert(id, (kind, body.to_vec()));
        self.order.push(id);
        Ok(id)
    }
}

pub fn hash_of(kind: HashKind) -> gitcore::Hash {
    match kind {
        HashKind::Sha1 => gitcore::Hash::Sha1,
        HashKind::Sha256 => gitcore::Hash::Sha256,
    }
}

pub fn hash_kind_of(hash: gitcore::Hash) -> HashKind {
    match hash {
        gitcore::Hash::Sha1 => HashKind::Sha1,
        gitcore::Hash::Sha256 => HashKind::Sha256,
    }
}

pub fn blob_id_of(oid: &Oid) -> BlobId {
    match oid {
        Oid::Sha1(digest) => BlobId::Sha1(*digest),
        Oid::Sha256(digest) => BlobId::Sha256(*digest),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Objects forge makes itself come in no pack: they land whole in one
    /// pack forge writes, indexed, and read back.
    #[test]
    fn objects_made_outside_a_pack_land_whole_in_one_pack() {
        let host = guest::MockHost::default();
        let ctx = host.exec(guest::MockHost::env("forge"));
        let bounds = Bounds {
            max_objects: 100,
            max_delta_depth: 64,
            max_object_size: 1 << 20,
            push_walk: 100,
            fetch_walk: 100,
            merge_cost: 100,
            page_size: 16,
            log_walk: 100,
            tree_walk: 100,
            diff_bytes: 1 << 20,
            blob_bytes: 1 << 20,
            record_bytes: 1 << 10,
        };
        let mut writer = ObjectWriter::new(&ctx, Hash::Sha1, &bounds);
        let made = [
            writer.put(Kind::Blob, b"made here").unwrap(),
            writer.put(Kind::Blob, b"and here").unwrap(),
        ];
        writer.flush(&[]).unwrap();
        assert_eq!(host.borrow().blobs.len(), 1);
        let store = ObjectStore::new(&ctx, Hash::Sha1, &bounds);
        for (id, body) in made.iter().zip([&b"made here"[..], b"and here"]) {
            assert_eq!(store.get(id).unwrap().unwrap().body, body);
            let at = PACKED.get(&ctx, &id.as_bytes().to_vec()).unwrap().unwrap();
            assert_eq!(at.pack, 0);
        }
    }
}
