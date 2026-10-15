//! Who led the last [`WINDOW`] blocks: the proposer strip's data, read a
//! page at a time from `chain.blocks` and kept by height, so a new head, a
//! gap after a reconnect and the first fill are one rule: ask for the
//! highest height of the span not held yet.
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::ops::RangeInclusive;
use view::prelude::*;

/// The blocks the strip spans.
pub const WINDOW: u64 = 64;

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Recent {
    /// the key that led each block held, by height; `None` where the node
    /// keeps no certificate for it (a block it state-synced past)
    pub led: BTreeMap<u64, Option<Vec<u8>>>,
    pub bottom: u64,
}

impl Recent {
    /// The heights the strip shows at `head`, oldest first.
    pub fn span(head: u64) -> RangeInclusive<u64> {
        head.saturating_sub(WINDOW - 1)..=head
    }

    /// The page that fills the highest height of the span not held yet,
    /// and the run of missing heights under it; `None` once all are held.
    pub fn next(&self, head: u64) -> Option<BlockPage> {
        let low = (*Self::span(head).start()).max(self.bottom);
        let missing = |height: &u64| !self.led.contains_key(height);
        let high = (low..=head).rev().find(missing)?;
        let run = (low..=high).rev().take_while(missing).count() as u64;
        Some(BlockPage {
            before: Some(high + 1),
            limit: run as u32,
        })
    }

    pub fn land(&mut self, head: u64, page: BlockList) {
        self.bottom = page.listed_from.unwrap_or(head + 1);
        for block in page.blocks {
            self.led.insert(block.height, block.proposer);
        }
        self.led = self.led.split_off(Self::span(head).start());
    }

    /// Whether `key` led the block at `height`: `None` where it is not
    /// held or names no proposer.
    pub fn led(&self, key: &[u8], height: u64) -> Option<bool> {
        self.led.get(&height)?.as_ref().map(|led| led == key)
    }
}
