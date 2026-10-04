use crate::*;
use serde::{Deserialize, Serialize};

mod write;
pub(crate) use write::SPARSE;

/// What one `decode` may build before it is refused: enough that
/// [`sanitize`]'s truncation still shapes any tree a real view sends, and
/// few enough that a hostile one cannot make the host allocate its way
/// through a frame's worth of nodes every tick.
pub const MAX_DECODED_NODES: usize = 16 * MAX_NODES;

/// Bounds what a decode may descend into, since decoding is recursive: a
/// [`Node`] holds its children, a tooltip's content and a patch's subtree,
/// and serde builds them from the inside out, so a chain of nodes is a
/// chain of stack frames. The tree the host walks afterwards —
/// [`sanitize`], the renderer, `Drop` — recurses the same way, which is why
/// the limit sits at decode rather than in each walk, and on [`Node`]
/// itself rather than on the fields that happen to hold one.
///
/// [`MAX_FRAME_BYTES`] of input is no protection: a chain deep
/// enough to overflow a host thread's stack is a few tens of kilobytes.
mod budget {
    use std::cell::Cell;

    use super::{MAX_DECODED_NODES, MAX_DEPTH};

    thread_local! {
        static DEPTH: Cell<usize> = const { Cell::new(0) };
        static NODES: Cell<usize> = const { Cell::new(0) };
    }

    /// One node being decoded. Descending past what the host walks, or
    /// building more nodes than it will hold, refuses the whole frame:
    /// there is no partial tree to keep, and a truncated one would be a
    /// tree the guest did not write.
    pub(super) struct Node(());

    impl Node {
        pub(super) fn enter() -> Result<Self, &'static str> {
            // The nodes this one sits inside: a decode's outermost node is
            // at 0, as `sanitize` counts depth.
            let depth = DEPTH.get();
            if depth > MAX_DEPTH {
                return Err("a tree deeper than the host renders");
            }
            spend()?;
            DEPTH.set(depth + 1);
            Ok(Self(()))
        }
    }

    impl Drop for Node {
        fn drop(&mut self) {
            DEPTH.set(DEPTH.get().saturating_sub(1));
        }
    }

    /// One more thing the host will hold: a node, or an entry of the style
    /// table, which costs what a node costs at no depth.
    pub(super) fn spend() -> Result<(), &'static str> {
        let nodes = NODES.get() + 1;
        if nodes > MAX_DECODED_NODES {
            return Err("more nodes than the host holds");
        }
        NODES.set(nodes);
        Ok(())
    }

    /// A fresh budget for one top-level [`decode`](super::decode). The depth
    /// unwinds itself; the node count is what one frame may spend.
    pub(super) fn reset() {
        NODES.set(0);
    }
}

/// An entry of the style table decodes on the frame's node budget.
pub(crate) fn spend_node() -> Result<(), &'static str> {
    budget::spend()
}

/// `Node`'s derived shape (`#[serde(remote = "Self")]`).
impl Serialize for Node {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        Node::serialize(self, serializer)
    }
}

/// Every node decodes inside the budget, wherever it sits: a frame's root,
/// a child, a tooltip's content, a patch's subtree. The guard is held while
/// the node is built: siblings share a depth, and each costs a node.
impl<'de> Deserialize<'de> for Node {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let _node = budget::Node::enter().map_err(serde::de::Error::custom)?;
        Node::deserialize(deserializer)
    }
}

/// Decodes a sequence of at most `limit` elements and refuses a longer one
/// as `message`: a length header past the limit is refused before any
/// element is read, and nothing is reserved from the header.
pub(crate) fn bounded_vec<'de, D, T>(
    deserializer: D,
    limit: usize,
    message: &'static str,
) -> Result<Vec<T>, D::Error>
where
    D: serde::Deserializer<'de>,
    T: Deserialize<'de>,
{
    struct Values<T>(usize, &'static str, std::marker::PhantomData<T>);
    impl<'de, T: Deserialize<'de>> serde::de::Visitor<'de> for Values<T> {
        type Value = Vec<T>;
        fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            formatter.write_str(self.1)
        }
        fn visit_seq<A: serde::de::SeqAccess<'de>>(
            self,
            mut seq: A,
        ) -> Result<Self::Value, A::Error> {
            if seq.size_hint().is_some_and(|len| len > self.0) {
                return Err(serde::de::Error::custom(self.1));
            }
            let mut values = Vec::new();
            while let Some(value) = seq.next_element()? {
                if values.len() == self.0 {
                    return Err(serde::de::Error::custom(self.1));
                }
                values.push(value);
            }
            Ok(values)
        }
    }
    deserializer.deserialize_seq(Values(limit, message, std::marker::PhantomData))
}

/// The bytes a wire value crosses as: see [`write`] for the encoding.
pub fn encode<T: Serialize>(value: &T) -> Vec<u8> {
    let mut bytes = Vec::new();
    value
        .serialize(write::Writer::new(&mut bytes))
        .expect("wire types are plain data");
    bytes
}

/// The bytes of a value whose serde is not the wire's own: a view's state,
/// whatever it derives or writes by hand. It is written with its field
/// names, so the build that reads it back may have added a field, and a
/// `Serialize` that refuses (a custom error) is an answer, not a panic
/// inside the guest.
pub fn try_encode<T: Serialize>(value: &T) -> Result<Vec<u8>, String> {
    let mut bytes = Vec::new();
    value
        .serialize(&mut rmp_serde::Serializer::new(&mut bytes).with_struct_map())
        .map_err(|error| error.to_string())?;
    Ok(bytes)
}

/// Marks a derived struct as sparse: most of its fields say nothing most of
/// the time, each with a `skip_serializing_if`, and on the wire it is the
/// fields that say something, each keyed by its declaration index.
///
/// The struct derives with `#[serde(remote = "Self")]`, and this writes the
/// two trait impls over that derived pair, as `Node`'s are written by hand
/// above.
macro_rules! sparse {
    ($type:ident) => {
        impl serde::Serialize for $type {
            fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
                struct Fields<'a>(&'a $type);
                impl serde::Serialize for Fields<'_> {
                    fn serialize<S: serde::Serializer>(
                        &self,
                        serializer: S,
                    ) -> Result<S::Ok, S::Error> {
                        $type::serialize(self.0, serializer)
                    }
                }
                serializer.serialize_newtype_struct($crate::codec::SPARSE, &Fields(self))
            }
        }
        impl<'de> serde::Deserialize<'de> for $type {
            fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
                $type::deserialize(deserializer)
            }
        }
    };
}
pub(crate) use sparse;

pub fn decode<'a, T: Deserialize<'a>>(bytes: &'a [u8]) -> Result<T, String> {
    budget::reset();
    canvas::reset_decode_budget();
    let mut deserializer = rmp_serde::Deserializer::new(std::io::Cursor::new(bytes));
    let value = T::deserialize(&mut deserializer).map_err(|error| error.to_string())?;
    if deserializer.position() != bytes.len() as u64 {
        return Err("trailing MessagePack bytes".into());
    }
    Ok(value)
}

#[cfg(test)]
mod tests {
    /// A `Serialize` written by hand that refuses.
    struct Refusing;

    impl serde::Serialize for Refusing {
        fn serialize<S: serde::Serializer>(&self, _: S) -> Result<S::Ok, S::Error> {
            Err(serde::ser::Error::custom("not while a transfer is open"))
        }
    }

    #[test]
    fn a_views_own_serde_answers_instead_of_panicking() {
        assert_eq!(
            super::try_encode(&Refusing).unwrap_err(),
            "not while a transfer is open"
        );
        assert_eq!(super::try_encode(&7u8).unwrap(), super::encode(&7u8));
    }

    /// A struct crosses as its fields in order and a variant as its index:
    /// no name is in the bytes.
    #[test]
    fn a_struct_is_its_fields_and_a_variant_is_its_index() {
        #[derive(serde::Serialize, serde::Deserialize, PartialEq, Debug)]
        enum Kind {
            Plain,
            Pair { left: u8, right: Option<bool> },
            One(String),
        }
        let pair = Kind::Pair {
            left: 7,
            right: None,
        };
        // {1: [7, nil]}
        assert_eq!(super::encode(&pair), [0x81, 0x01, 0x92, 0x07, 0xc0]);
        assert_eq!(super::encode(&Kind::Plain), [0x00]);
        // {2: "a"}
        assert_eq!(
            super::encode(&Kind::One("a".into())),
            [0x81, 0x02, 0xa1, b'a']
        );
        for kind in [pair, Kind::Plain, Kind::One("a".into())] {
            assert_eq!(super::decode::<Kind>(&super::encode(&kind)).unwrap(), kind);
        }
    }

    /// A map or a sequence that does not say its length up front (gpui's
    /// `FontFeatures` writes one) is counted as it is written.
    #[test]
    fn a_map_of_unknown_length_is_counted_as_it_is_written() {
        let features = gpui::FontFeatures(std::sync::Arc::new(vec![
            ("calt".into(), 1),
            ("liga".into(), 0),
        ]));
        let bytes = super::encode(&features);
        assert_eq!(bytes[..5], [0xdf, 0, 0, 0, 2]);
        assert_eq!(
            super::decode::<gpui::FontFeatures>(&bytes).unwrap(),
            features
        );

        struct Unsized;
        impl serde::Serialize for Unsized {
            fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
                serializer.collect_seq((0..3u8).filter(|_| true))
            }
        }
        assert_eq!(super::encode(&Unsized), [0xdd, 0, 0, 0, 3, 0, 1, 2]);
    }

    /// A struct that leaves a field out, and never said it is sparse, would
    /// shift every field after it: the writer refuses it.
    #[test]
    #[should_panic(expected = "wire types are plain data")]
    fn a_struct_that_skips_a_field_must_be_sparse() {
        #[derive(serde::Serialize)]
        struct Skips {
            #[serde(skip_serializing_if = "Option::is_none")]
            first: Option<u8>,
            second: u8,
        }
        super::encode(&Skips {
            first: None,
            second: 1,
        });
    }
}
