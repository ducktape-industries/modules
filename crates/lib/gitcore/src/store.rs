// The Objects trait the host supplies, and MemoryObjects, a BTreeMap-backed store for tests and small uses.

use crate::error::{Error, Result};
use crate::object::{Kind, Object};
use crate::oid::{Hash, Oid, oid_of};
use std::collections::BTreeMap;

pub trait Objects {
    fn get(&self, id: &Oid) -> Result<Option<Object>>;
    fn has(&self, id: &Oid) -> Result<bool>;
}

pub trait WriteObjects: Objects {
    fn keep(&mut self, id: Oid, object: Object) -> Result<()>;
}

#[derive(Clone, Debug)]
pub struct MemoryObjects {
    hash: Hash,
    objects: BTreeMap<Oid, Object>,
}

impl MemoryObjects {
    pub fn new(hash: Hash) -> MemoryObjects {
        MemoryObjects {
            hash,
            objects: BTreeMap::new(),
        }
    }

    pub fn hash(&self) -> Hash {
        self.hash
    }

    pub fn count(&self) -> usize {
        self.objects.len()
    }

    pub fn ids(&self) -> impl Iterator<Item = &Oid> {
        self.objects.keys()
    }

    pub fn put(&mut self, kind: Kind, body: &[u8]) -> Result<Oid> {
        let id = oid_of(self.hash, kind, body)?;
        self.objects
            .entry(id)
            .or_insert_with(|| Object::new(kind, body.to_vec()));
        Ok(id)
    }
}

impl Objects for MemoryObjects {
    fn get(&self, id: &Oid) -> Result<Option<Object>> {
        Ok(self.objects.get(id).cloned())
    }

    fn has(&self, id: &Oid) -> Result<bool> {
        Ok(self.objects.contains_key(id))
    }
}

impl WriteObjects for MemoryObjects {
    fn keep(&mut self, id: Oid, object: Object) -> Result<()> {
        self.objects.entry(id).or_insert(object);
        Ok(())
    }
}

pub(crate) fn load<S: Objects + ?Sized>(store: &S, id: &Oid) -> Result<Object> {
    store.get(id)?.ok_or(Error::MissingObject(*id))
}

pub(crate) fn load_kind<S: Objects + ?Sized>(store: &S, id: &Oid, kind: Kind) -> Result<Object> {
    let object = load(store, id)?;
    let right_kind = object.kind == kind;
    if !right_kind {
        return Err(Error::WrongKind {
            id: *id,
            expected: kind,
        });
    }
    Ok(object)
}
