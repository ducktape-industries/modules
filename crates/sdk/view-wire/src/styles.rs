//! The style table. A node names its style by [`StyleId`], and each distinct
//! style crosses once, as one [`Style`] entry of the table: a page of rows
//! sends its row style one time, not once per row.
//!
//! The table belongs to the tree the host holds. A frame that carries a
//! whole tree carries the whole table, which replaces the host's; any other
//! frame carries the entries the host does not have yet, which the host adds
//! after the ones it holds. The guest's half is the [`Interner`], the host's
//! the [`Styles`] it reads at render.

use std::collections::HashMap;
use std::hash::{BuildHasherDefault, Hasher};
use std::sync::Arc;

use gpui::StyleRefinement;
use serde::{Deserialize, Serialize};

mod entry;

/// The most styles a table holds: a tree the host takes has no more nodes
/// than this, and a table past it is storage, not a tree's styles. A whole
/// frame over it is refused; a guest whose table outgrows it between whole
/// frames sends the next frame whole, which starts the table over.
pub const MAX_STYLES: usize = crate::MAX_NODES;

/// The style a node names: an entry of the table its tree crosses with.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct StyleId(pub u32);

/// One entry of the table as it crosses: a `StyleRefinement` in the entry
/// encoding (`styles/entry.rs`). A reader of JSON sees the style itself.
#[derive(Clone, PartialEq, Eq)]
pub struct Style(Box<[u8]>);

impl Style {
    pub fn new(style: &StyleRefinement) -> Self {
        let mut bytes = Vec::new();
        entry::write(style, &mut bytes);
        Self(bytes.into())
    }

    /// The style the entry holds, as the host takes it: a colour rounded to
    /// the wire's eight bits a channel, nothing else changed.
    pub fn read(&self) -> Result<StyleRefinement, &'static str> {
        entry::read(&self.0)
    }
}

impl std::fmt::Debug for Style {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self.read() {
            Ok(style) => style.fmt(formatter),
            Err(refusal) => write!(formatter, "Style({refusal}: {:?})", self.0),
        }
    }
}

impl Serialize for Style {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        match serializer.is_human_readable() {
            true => self
                .read()
                .map_err(serde::ser::Error::custom)?
                .serialize(serializer),
            false => serializer.serialize_bytes(&self.0),
        }
    }
}

/// An entry decodes inside the frame's node budget: a table is not a way
/// to make the host build more than a frame's worth.
impl<'de> Deserialize<'de> for Style {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct Bytes;
        impl serde::de::Visitor<'_> for Bytes {
            type Value = Style;
            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("a style entry's bytes")
            }
            fn visit_bytes<E: serde::de::Error>(self, bytes: &[u8]) -> Result<Style, E> {
                Ok(Style(bytes.into()))
            }
        }
        crate::codec::spend_node().map_err(serde::de::Error::custom)?;
        match deserializer.is_human_readable() {
            true => StyleRefinement::deserialize(deserializer).map(|style| Style::new(&style)),
            false => deserializer.deserialize_bytes(Bytes),
        }
    }
}

pub(crate) fn decode_table<'de, D: serde::Deserializer<'de>>(
    deserializer: D,
) -> Result<Vec<Style>, D::Error> {
    crate::bounded_vec(deserializer, MAX_STYLES, "more styles than the host holds")
}

/// Every refinement an entry holds and its fields, in the order the entry's
/// bitmaps count them: what `schema.txt` pins of the entry encoding.
#[doc(hidden)]
pub fn entry_fields() -> [(&'static str, &'static [&'static str]); 6] {
    entry::fields()
}

/// A key that is already a hash.
#[derive(Default)]
struct Hashed(u64);

impl Hasher for Hashed {
    fn finish(&self) -> u64 {
        self.0
    }
    fn write(&mut self, _: &[u8]) {
        unreachable!("the table's keys are hashes")
    }
    fn write_u64(&mut self, hash: u64) {
        self.0 = hash;
    }
}

/// An entry's bytes, eight at a time.
fn hash(bytes: &[u8]) -> u64 {
    const PRIME: u64 = 0x9e37_79b9_7f4a_7c15;
    let mut hash = bytes.len() as u64;
    let mut chunks = bytes.chunks_exact(8);
    for chunk in &mut chunks {
        let word = u64::from_le_bytes(chunk.try_into().expect("eight bytes"));
        hash = (hash ^ word).wrapping_mul(PRIME);
    }
    let mut last = [0; 8];
    last[..chunks.remainder().len()].copy_from_slice(chunks.remainder());
    hash = (hash ^ u64::from_le_bytes(last)).wrapping_mul(PRIME);
    hash ^ (hash >> 32)
}

const NONE: u32 = u32::MAX;

/// The guest's half of the table: every distinct style its tree named,
/// numbered in the order first met, and how many of them the host has.
///
/// A style keeps its id from frame to frame, so two frames' nodes compare
/// by id. Two styles are one entry when their entry bytes are equal.
#[derive(Default)]
pub struct Interner {
    /// Every entry's bytes end to end, and where each entry ends.
    bytes: Vec<u8>,
    ends: Vec<u32>,
    /// The latest id whose bytes hash to a key, and for each id the one
    /// before it with the same hash.
    latest: HashMap<u64, u32, BuildHasherDefault<Hashed>>,
    before: Vec<u32>,
    /// The entries from here on have not crossed.
    sent: usize,
    /// The entry being looked up.
    scratch: Vec<u8>,
}

impl Interner {
    /// The id of `style`, new if no entry holds it yet.
    pub fn intern(&mut self, style: &StyleRefinement) -> StyleId {
        let mut scratch = std::mem::take(&mut self.scratch);
        scratch.clear();
        entry::write(style, &mut scratch);
        let id = self.find_or_add(&scratch);
        self.scratch = scratch;
        id
    }

    /// The id of the style that sets nothing: [`Self::intern`] of a
    /// default style, without writing one out.
    pub fn intern_empty(&mut self) -> StyleId {
        self.find_or_add(&entry::EMPTY)
    }

    fn entry(&self, id: usize) -> &[u8] {
        let start = match id {
            0 => 0,
            id => self.ends[id - 1] as usize,
        };
        &self.bytes[start..self.ends[id] as usize]
    }

    fn find_or_add(&mut self, entry: &[u8]) -> StyleId {
        let hash = hash(entry);
        let latest = self.latest.get(&hash).copied().unwrap_or(NONE);
        let mut id = latest;
        while id != NONE {
            if self.entry(id as usize) == entry {
                return StyleId(id);
            }
            id = self.before[id as usize];
        }
        let id = self.ends.len() as u32;
        self.bytes.extend_from_slice(entry);
        self.ends.push(self.bytes.len() as u32);
        self.before.push(latest);
        self.latest.insert(hash, id);
        StyleId(id)
    }

    /// How many entries the table holds.
    pub fn len(&self) -> usize {
        self.ends.len()
    }

    pub fn is_empty(&self) -> bool {
        self.ends.is_empty()
    }

    /// The entries the host does not hold yet, in id order: what the next
    /// frame carries. They count as sent from here on.
    pub fn unsent(&mut self) -> Vec<Style> {
        let entries = (self.sent..self.len())
            .map(|id| Style(self.entry(id).into()))
            .collect();
        self.sent = self.len();
        entries
    }

    /// Starts the table over with the styles `visit` names and no others,
    /// numbered in the order it names them, and gives each id it is handed
    /// its new number. None of them has crossed: a whole frame carries the
    /// table whole.
    pub fn retain(&mut self, visit: impl FnOnce(&mut dyn FnMut(&mut StyleId))) {
        let mut kept = Self {
            scratch: std::mem::take(&mut self.scratch),
            ..Self::default()
        };
        let mut renumbered = vec![NONE; self.len()];
        visit(&mut |id| {
            let old = id.0 as usize;
            if renumbered[old] == NONE {
                renumbered[old] = kept.find_or_add(self.entry(old)).0;
            }
            id.0 = renumbered[old];
        });
        *self = kept;
    }
}

/// The host's half of the table: the styles of the tree it holds, each read
/// and bounded once when it arrived. A node's [`StyleId`] indexes it.
///
/// A clone shares the entries: whoever draws a tree takes the table with
/// it, and a tree kept past the next whole frame (a tooltip's content)
/// keeps the table it came with.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Styles {
    /// The tree's own entries, by id.
    held: Vec<Arc<StyleRefinement>>,
    /// The host's own ([`Self::host`]), named from the far end of the ids.
    host: Vec<Arc<StyleRefinement>>,
}

impl Styles {
    fn entry(&self, id: StyleId) -> Option<&Arc<StyleRefinement>> {
        let held = self.held.get(id.0 as usize);
        held.or_else(|| self.host.get((u32::MAX - id.0) as usize))
    }

    /// The style `id` names. The sanitizer refuses a tree that names a
    /// style the table does not hold, so a tree it passed finds every one.
    pub fn get(&self, id: StyleId) -> Option<&StyleRefinement> {
        self.entry(id).map(|style| &**style)
    }

    /// The style `id` names, shared: for a holder that reads it while the
    /// table's owner is borrowed for something else.
    pub fn share(&self, id: StyleId) -> Arc<StyleRefinement> {
        self.entry(id)
            .expect("a sanitized tree names styles the table holds")
            .clone()
    }

    /// How many entries the tree's own table holds: the ids a node of it
    /// may name.
    pub fn len(&self) -> usize {
        self.held.len()
    }

    pub fn is_empty(&self) -> bool {
        self.held.is_empty()
    }

    /// Whether every id `older` held names the same entry here: `older`
    /// with entries added, not another table. Two nodes with equal ids are
    /// equal styles only across tables this holds for.
    pub fn extends(&self, older: &Self) -> bool {
        older.held.len() <= self.held.len()
            && older
                .held
                .iter()
                .zip(&self.held)
                .all(|(old, new)| Arc::ptr_eq(old, new))
    }

    /// Adds a style of the host's own and answers the id that names it: for
    /// a node the host puts in the tree it draws (the box around a view's
    /// root). It is no entry of the tree's table: it is neither bounded nor
    /// counted, and its id is one no frame's node may name.
    pub fn host(&mut self, style: Arc<StyleRefinement>) -> StyleId {
        self.host.push(style);
        StyleId(u32::MAX - (self.host.len() - 1) as u32)
    }

    /// Forgets the entries past the first `len`: the frame that brought
    /// them was refused.
    pub(crate) fn truncate(&mut self, len: usize) {
        self.held.truncate(len);
    }

    /// Adds `entries` after the ones held, each read and bounded here,
    /// once: all of them, or on a refusal none.
    /// [`sanitize`](crate::sanitize) does this with a frame's.
    pub fn extend(&mut self, entries: Vec<Style>) -> Result<(), &'static str> {
        if self.held.len() + entries.len() > MAX_STYLES {
            return Err("more styles than the host holds");
        }
        let held = self.held.len();
        for entry in entries {
            let style = entry.read().and_then(|mut style| {
                crate::style_sanitize::sanitize(&mut style)?;
                Ok(style)
            });
            match style {
                Ok(style) => self.held.push(Arc::new(style)),
                Err(refused) => {
                    self.held.truncate(held);
                    return Err(refused);
                }
            }
        }
        Ok(())
    }
}

/// The style a tree the sanitizer passed names.
impl std::ops::Index<StyleId> for Styles {
    type Output = StyleRefinement;

    fn index(&self, id: StyleId) -> &StyleRefinement {
        self.get(id)
            .expect("a sanitized tree names styles the table holds")
    }
}

/// The table of a test whose nodes' styles are not what it is about: an
/// empty style, which every such node names as `StyleId(0)`, and a second
/// one, `StyleId(1)`, for a node that must differ from another by style
/// alone.
#[cfg(test)]
pub(crate) mod testing {
    use super::*;

    pub(crate) fn plain() -> Vec<Style> {
        let marked = gpui::Styled::bg(StyleRefinement::default(), gpui::rgb(0xffcc00));
        vec![Style::new(&StyleRefinement::default()), Style::new(&marked)]
    }

    pub(crate) fn held() -> Styles {
        let mut styles = Styles::default();
        styles.extend(plain()).unwrap();
        styles
    }

    /// [`sanitize`](crate::sanitize), for a frame whose nodes name the
    /// plain style and which did not bring a table of its own.
    pub(crate) fn sanitize_plain(
        frame: &mut crate::Frame,
    ) -> Result<crate::SanitizeReport, crate::Refused> {
        if frame.styles.is_empty() {
            frame.styles = plain();
        }
        crate::sanitize(frame, &mut Styles::default())
    }
}
