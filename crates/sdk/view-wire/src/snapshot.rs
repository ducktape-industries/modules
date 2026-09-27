//! Complete owned guest state: the view's own serde as named MessagePack,
//! crossing whole. Unlike a rendered tree, it is never truncated, so the
//! one bound is its size.
pub const MAX_SNAPSHOT_BYTES: usize = 8 * 1024 * 1024;
