use crate::*;
use serde::{Deserialize, Serialize};

/// What one `decode` may build before it is refused: enough that
/// [`sanitize`]'s truncation still shapes any tree a real view sends, and
/// few enough that a hostile one cannot make the host allocate its way
/// through a frame's worth of nodes every tick.
pub const MAX_DECODED_NODES: usize = 16 * MAX_NODES;

/// Bounds what a decode may descend into, since decoding is recursive: a
/// [`Node`] holds its children and serde builds them from the inside out, so
/// a chain of containers is a chain of stack frames. The tree the host walks
/// afterwards — [`sanitize`], the renderer, `Drop` — recurses the same way,
/// which is why the limit sits at decode rather than in each walk.
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
            let depth = DEPTH.get() + 1;
            if depth > MAX_DEPTH {
                return Err("a tree deeper than the host renders");
            }
            let nodes = NODES.get() + 1;
            if nodes > MAX_DECODED_NODES {
                return Err("more nodes than the host holds");
            }
            NODES.set(nodes);
            DEPTH.set(depth);
            Ok(Self(()))
        }
    }

    impl Drop for Node {
        fn drop(&mut self) {
            DEPTH.set(DEPTH.get().saturating_sub(1));
        }
    }

    /// A fresh budget for one top-level [`decode`](super::decode). The depth
    /// unwinds itself; the node count is what one frame may spend.
    pub(super) fn reset() {
        NODES.set(0);
    }
}

pub(crate) fn decode_child<'de, D: serde::Deserializer<'de>>(
    deserializer: D,
) -> Result<Box<Node>, D::Error> {
    let _node = budget::Node::enter().map_err(serde::de::Error::custom)?;
    Box::<Node>::deserialize(deserializer)
}

pub(crate) fn decode_children<'de, D: serde::Deserializer<'de>>(
    deserializer: D,
) -> Result<Vec<Node>, D::Error> {
    struct Children;

    impl<'de> serde::de::Visitor<'de> for Children {
        type Value = Vec<Node>;

        fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            formatter.write_str("a list of nodes")
        }

        fn visit_seq<A: serde::de::SeqAccess<'de>>(
            self,
            mut children: A,
        ) -> Result<Self::Value, A::Error> {
            let mut nodes = Vec::new();
            // The guard is held while one child is built and dropped before
            // the next: siblings share a depth, and each costs a node.
            while let Some(child) = {
                let _node = budget::Node::enter().map_err(serde::de::Error::custom)?;
                children.next_element::<Node>()?
            } {
                nodes.push(child);
            }
            Ok(nodes)
        }
    }

    deserializer.deserialize_seq(Children)
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

pub fn encode<T: Serialize>(value: &T) -> Vec<u8> {
    let mut bytes = Vec::new();
    write(value, &mut bytes).expect("wire types are plain data");
    bytes
}

/// [`encode`] for a value whose serde is not the wire's own — a view's
/// state, whatever it derives or writes by hand — so a `Serialize` that
/// refuses (an unsized sequence, a custom error) is an answer, not a panic
/// inside the guest.
pub fn try_encode<T: Serialize>(value: &T) -> Result<Vec<u8>, String> {
    let mut bytes = Vec::new();
    write(value, &mut bytes).map_err(|error| error.to_string())?;
    Ok(bytes)
}

// One serializer instantiation for buffers and size counting: a second writer
// type would duplicate the entire node serialization graph.
#[inline(never)]
fn write<T: Serialize>(
    value: &T,
    writer: &mut dyn std::io::Write,
) -> Result<(), rmp_serde::encode::Error> {
    value.serialize(&mut rmp_serde::Serializer::new(writer).with_struct_map())
}

/// Counts bytes without keeping them.
struct Count(u64);

impl std::io::Write for Count {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        self.0 += bytes.len() as u64;
        Ok(bytes.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

/// How many bytes [`encode`] would write, without writing them.
pub fn encoded_size<T: Serialize>(value: &T) -> u64 {
    let mut count = Count(0);
    write(value, &mut count).expect("wire types are plain data");
    count.0
}

pub fn decode<'a, T: Deserialize<'a>>(bytes: &'a [u8]) -> Result<T, String> {
    budget::reset();
    editor_transaction::reset_decode_budget();
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
}
