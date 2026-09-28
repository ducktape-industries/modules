//! Who led the last [`WINDOW`] blocks: the proposer strip's data, read a
//! page at a time from `chain.blocks` and kept by height, so a new head, a
//! gap after a reconnect and the first fill are one rule: ask for the
//! highest height of the span not held yet.
use ducktape_view_guest::methods::{Block, BlockPage};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::ops::RangeInclusive;

/// The blocks the strip spans.
pub const WINDOW: u64 = 64;
/// Blocks per `chain.blocks` page: explorer-view's, small so one reply's
/// decoding stays well inside a tick's fuel.
const PAGE: u64 = 20;

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Recent {
    /// the key that led each block held, by height; `None` where the node
    /// keeps no certificate for it (a block it state-synced past)
    pub led: BTreeMap<u64, Option<Vec<u8>>>,
    /// the lowest height the node's archive keeps, as a short page showed
    ///
    /// ponytail: set once and never lowered, so an empty page for a tip the
    /// node had not archived yet leaves that height a gap until it scrolls
    /// out, and an archive that later grows below it is not re-read; reset
    /// it on a reconnect if either shows up.
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
            limit: run.min(PAGE) as u32,
        })
    }

    /// Folds in the page `asked` returned, then drops what fell out of
    /// the span at `head`.
    pub fn land(&mut self, asked: &BlockPage, head: u64, page: Vec<Block>) {
        if page.len() < asked.limit as usize {
            self.bottom = page
                .last()
                .map_or(asked.before.unwrap_or(0), |block| block.height);
        }
        for block in page {
            self.led.insert(block.height, block.proposer);
        }
        self.led = self.led.split_off(Self::span(head).start());
    }

    /// Of the span at `head`: the blocks `key` led, the last one it led,
    /// and how many blocks name who led them.
    pub fn proposed(&self, key: &[u8], head: u64) -> (u64, Option<u64>, u64) {
        let mut count = 0;
        let mut last = None;
        let mut known = 0;
        for (height, led) in self.led.range(Self::span(head)) {
            let Some(led) = led else { continue };
            known += 1;
            if led == key {
                count += 1;
                last = Some(*height);
            }
        }
        (count, last, known)
    }

    /// Whether `key` led the block at `height`: `None` where it is not
    /// held or names no proposer.
    pub fn led(&self, key: &[u8], height: u64) -> Option<bool> {
        self.led.get(&height)?.as_ref().map(|led| led == key)
    }
}
