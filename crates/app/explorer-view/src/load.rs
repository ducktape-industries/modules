//! The loaders: the head and the blocks it brings (`chain.status`,
//! `chain.blocks`), the lists the system programs answer, the block a
//! transaction row sits in (`chain.block`), and what an op says it does
//! (`program.describe`). A list is read with `cx.load`, which keeps what
//! is on screen while it reads again and draws only what changed.
use view::prelude::*;

use crate::chain::{BlockRow, TxRow, rows};
use crate::recent::{self, Listing};
use crate::state::Route;
use crate::ui::LATEST;
use crate::{Explorer, WINDOW, decode, queries};

impl Explorer {
    /// Everything, again: the boot, a restore, a retry.
    pub(crate) fn read_all(&mut self, cx: &mut Context<Self>) {
        self.read_head(cx);
        self.read_accounts(cx);
        self.read_validators(cx);
        self.read_network(cx);
        self.read_used(cx);
        self.read_recent(cx);
        self.read_acted(cx);
        // a block refused once is asked for again
        self.blocks.retain(|_, block| block.failed().is_none());
        if let Route::Block(height) | Route::Tx { height, .. } = self.route {
            self.locate([height], cx);
        }
        self.pull(cx);
    }

    /// A head `chain.heads` pushed: the blocks held follow it without a
    /// read, and the status moves to it when the page lands. A head is not
    /// drawn on its own: that redraw changed nothing (the page lands a tick
    /// later and draws once) and cost a full render of the page that was
    /// up, 74M fuel on Transactions, every block.
    pub(crate) fn at_head(&mut self, head: Head, cx: &mut Context<Self>) {
        // a change lands at its height without a write to the programs role, so
        // the programs role's live heads say nothing: the head that reaches it
        // reads the roster again
        if let Loadable::Ready(network) = &self.network
            && network
                .changes
                .first()
                .is_some_and(|change| change.height <= head.height)
        {
            self.read_network(cx);
        }
        match &self.status {
            Loadable::Ready(_) | Loadable::Reloading(_, _) => {
                self.head = self.head.max(head.height);
                self.pull(cx);
            }
            _ => self.read_head(cx),
        }
    }

    /// The node's status, and the blocks it moves. A status on screen stays
    /// while it is read again; a refused re-read is logged and leaves it.
    pub(crate) fn read_head(&mut self, cx: &mut Context<Self>) {
        let ask = cx.host().ask::<ChainStatus>(());
        if self.status.ready().is_some() {
            self.rereading_status = Some(cx.land(ask, |view, status, cx| match status {
                Ok(status) => {
                    if view.status.ready() != Some(&status) {
                        view.status = Loadable::Ready(status);
                        cx.notify();
                    }
                    view.pull(cx);
                }
                Err(refusal) => cx.log_refused("the node's status", &refusal),
            }));
        } else if !self.status.is_loading() {
            cx.load(self, ask, |view| &mut view.status);
        }
        // a failed page is read again with the head, not in a loop
        if self.chain.failed.is_some() {
            self.pull(cx);
        }
    }

    pub(crate) fn read_accounts(&mut self, cx: &mut Context<Self>) {
        let work = queries::accounts(cx.host());
        cx.load(self, work, |view| &mut view.accounts);
    }

    pub(crate) fn read_validators(&mut self, cx: &mut Context<Self>) {
        let work = queries::validators(cx.host());
        cx.load(self, work, |view| &mut view.validators);
    }

    pub(crate) fn read_network(&mut self, cx: &mut Context<Self>) {
        let work = queries::network(cx.host());
        cx.load(self, work, |view| &mut view.network);
    }

    pub(crate) fn read_used(&mut self, cx: &mut Context<Self>) {
        let work = queries::used(cx.host());
        cx.load(self, work, |view| &mut view.used);
    }

    /// The list the route shows: the chain's latest transactions, or the
    /// named program's. The list held stays while it is the one shown.
    pub(crate) fn read_recent(&mut self, cx: &mut Context<Self>) {
        let program = match &self.route {
            Route::Transactions(program) => program.clone(),
            Route::Overview => None,
            _ => return,
        };
        if self.recent(program.as_deref()).is_some() {
            return;
        }
        let (host, asked) = (cx.host(), program.clone());
        let listing = Listing::new(
            move |after| recent::recent(host.clone(), asked.clone(), after),
            cx,
        );
        self.recent = Some((program, listing));
    }

    /// What the account on screen did, and when its keys last signed.
    pub(crate) fn read_acted(&mut self, cx: &mut Context<Self>) {
        let Route::Account(number) = self.route else {
            return;
        };
        if self.last_used(number).is_none() {
            self.devices = Loadable::Idle;
        }
        let work = queries::devices(cx.host(), number);
        cx.load(self, work, |view| &mut view.devices);
        if self.acted(number).is_some() {
            return;
        }
        let host = cx.host();
        let listing = Listing::new(move |after| recent::acted(host.clone(), number, after), cx);
        self.acted = Some((number, listing));
    }

    /// A frame the index just kept: each list it answers is read again
    /// from its start, as far as it was read.
    pub(crate) fn reread_index(&mut self, cx: &mut Context<Self>) {
        self.read_used(cx);
        if let Some((_, recent)) = &self.recent {
            recent.reread(cx);
        }
        if let Some((_, acted)) = &self.acted {
            acted.reread(cx);
        }
        if let Route::Account(number) = self.route {
            let work = queries::devices(cx.host(), number);
            cx.load(self, work, |view| &mut view.devices);
        }
    }

    /// The blocks the head wants: the newest page when none is held, else
    /// the blocks since the top. Older ones are the Blocks tab's
    /// ([`pull_older`](Self::pull_older)).
    pub(crate) fn pull(&mut self, cx: &mut Context<Self>) {
        let head = self
            .status
            .ready()
            .map(|status| status.height.max(self.head));
        let limit = match (self.chain.top(), head) {
            (None, _) => u32::MAX,
            (Some(top), Some(head)) if head > top => u32::try_from(head - top).unwrap_or(u32::MAX),
            _ => return,
        };
        self.ask_blocks(None, limit, cx);
    }

    /// The page below the oldest block held: what the Blocks tab asks for
    /// when its last line is drawn, until it holds [`WINDOW`] blocks. A
    /// quiet chain folds into a line or two, so the last line is drawn at
    /// once: without the bound the tab would read every block ever.
    pub(crate) fn pull_older(&mut self, cx: &mut Context<Self>) {
        if self.chain.complete || self.chain.blocks.len() >= WINDOW {
            return;
        }
        let Some(oldest) = self.chain.blocks.last().map(|block| block.height) else {
            return;
        };
        self.ask_blocks(Some(oldest), u32::MAX, cx);
    }

    fn ask_blocks(&mut self, before: Option<u64>, limit: u32, cx: &mut Context<Self>) {
        if self.pulling {
            return;
        }
        self.pulling = true;
        let ask = cx.host().ask::<ChainBlocks>(BlockPage { before, limit });
        cx.land(ask, move |view, page, cx| {
            view.pulling = false;
            match page {
                Ok(page) => {
                    let was = (view.chain.top(), view.chain.blocks.len());
                    view.chain.land(before, page);
                    // the blocks the Blocks tab is reading down through stay
                    // with it; any other page holds the newest alone
                    if before.is_none() && view.route != Route::Blocks {
                        view.chain.trim(WINDOW);
                    }
                    // the status moves with the page that reached the head
                    if let (Loadable::Ready(status) | Loadable::Reloading(status, _), Some(top)) =
                        (&mut view.status, view.chain.blocks.first())
                        && top.height > status.height
                    {
                        status.height = top.height;
                        status.tip = top.id;
                    }
                    // a page that moved nothing is not asked for again
                    // until the head moves
                    if before.is_none() && (view.chain.top(), view.chain.blocks.len()) != was {
                        view.pull(cx);
                    }
                }
                Err(refusal) => view.chain.failed = Some(refusal.message),
            }
            cx.notify();
        })
        .detach();
    }

    /// Reads the blocks at `heights` that are neither held nor read nor
    /// being read: what a transaction row drawn from the index needs to
    /// draw its hash, its signer, how it ran and, when short, its op. The
    /// node lists a payload with its block only when short, so a drawn row
    /// never pulls a long one: that is [`open`](Self::open)'s, on the row's
    /// own page.
    pub(crate) fn locate(
        &mut self,
        heights: impl IntoIterator<Item = u64>,
        cx: &mut Context<Self>,
    ) {
        for height in heights {
            if self.chain.block(height).is_some() || self.blocks.contains_key(&height) {
                continue;
            }
            let ask = cx.host().ask::<ChainBlock>(BlockRef::Height(height));
            cx.load(
                self,
                async move { ask.await.map(|block| block.map(rows)) },
                move |view| view.blocks.entry(height).or_default(),
            );
        }
    }

    /// The blocks the Overview's latest transactions sit in.
    pub(crate) fn locate_latest(&mut self, cx: &mut Context<Self>) {
        if self.route != Route::Overview {
            return;
        }
        let heights: Vec<u64> = self
            .recent(None)
            .map(|recent| recent.held(Some(LATEST)))
            .unwrap_or_default()
            .iter()
            .map(|row| row.height)
            .collect();
        self.locate(heights, cx);
    }

    /// Asks the host what `tx` does, once, the first time it is drawn; a
    /// program that says nothing, or a refusal, leaves its bytes.
    ///
    /// ponytail: drawing asks (through `TxRow`'s cells) so only rows on
    /// screen are described, not every transaction held; a render that
    /// stays pure needs the visible rows computed ahead of it.
    pub(crate) fn describe(&self, tx: &TxRow, cx: &mut Context<Self>) {
        let Some(payload) = &tx.payload else {
            return;
        };
        if tx.op.get().is_some() || tx.asked.replace(true) {
            return;
        }
        let ask = cx
            .host()
            .ask::<ProgramDescribe>((tx.target.clone(), payload.clone()));
        let (place, target, payload) = ((tx.height, tx.index), tx.target.clone(), payload.clone());
        cx.land(ask, move |view, described, cx| {
            let op = worded(&target, &payload, described, cx);
            view.described(place, op, cx);
        })
        .detach();
    }

    /// The whole payload of a transaction opened, where the node listed its
    /// block without it (too long to list): read by its place and described.
    pub(crate) fn open(&self, tx: &TxRow, cx: &mut Context<Self>) {
        if tx.payload.is_some() {
            return self.describe(tx, cx);
        }
        if tx.op.get().is_some() || tx.asked.replace(true) {
            return;
        }
        let host = cx.host();
        let whole = TxAsk {
            tx: TxRef::At {
                height: tx.height,
                position: tx.index,
            },
            most: u32::MAX,
        };
        let target = tx.target.clone();
        let read = async move {
            let located = host.ask::<ChainTx>(whole).await?;
            let Some(payload) = located.and_then(|located| located.tx.payload) else {
                return Ok(None);
            };
            let described = host.ask::<ProgramDescribe>((target, payload.clone())).await;
            Ok::<_, Error>(Some((payload, described)))
        };
        let (place, target, size) = ((tx.height, tx.index), tx.target.clone(), u64::from(tx.size));
        cx.land(read, move |view, read, cx| {
            let op = match read {
                Ok(Some((payload, described))) => worded(&target, &payload, described, cx),
                Ok(None) => unread(&target, size),
                Err(refusal) => {
                    cx.log_refused("a transaction's payload", &refusal);
                    unread(&target, size)
                }
            };
            view.described(place, op, cx);
        })
        .detach();
    }

    fn described(&mut self, place: (u64, u32), op: Description, cx: &mut Context<Self>) {
        // the row at that place, held with its page and read with its block
        let rows = self.chain.txs.iter().chain(self.read_txs());
        for tx in rows.filter(|tx| (tx.height, tx.index) == place) {
            let _ = tx.op.set(op.clone());
        }
        cx.notify();
    }

    /// Every transaction of the blocks read outside the ones held.
    fn read_txs(&self) -> impl Iterator<Item = &TxRow> {
        self.blocks
            .values()
            .filter_map(|block| block.ready()?.as_ref())
            .flat_map(|(_, txs)| txs)
    }

    /// The transaction at its place, held with its page or read with its block.
    pub(crate) fn tx_at(&self, height: u64, position: u32) -> Option<&TxRow> {
        let at = |tx: &&TxRow| (tx.height, tx.index) == (height, position);
        self.chain
            .txs
            .iter()
            .find(at)
            .or_else(|| self.read_txs().find(at))
    }

    pub(crate) fn tx_by_hash(&self, hash: &[u8; 32]) -> Option<&TxRow> {
        let hashed = |tx: &&TxRow| &tx.hash == hash;
        self.chain
            .txs
            .iter()
            .find(hashed)
            .or_else(|| self.read_txs().find(hashed))
    }

    /// A block held with its page, or read outside them.
    pub(crate) fn block(&self, height: u64) -> Option<(BlockRow, Vec<&TxRow>)> {
        if let Some(row) = self.chain.block(height) {
            let txs = self.chain.txs.iter().filter(|tx| tx.height == height);
            return Some((row.clone(), txs.collect()));
        }
        let (row, txs) = self.blocks.get(&height)?.ready()?.as_ref()?;
        Some((row.clone(), txs.iter().collect()))
    }
}

fn worded(
    target: &str,
    payload: &[u8],
    described: Result<Option<Description>, Error>,
    cx: &mut Context<Explorer>,
) -> Description {
    match described {
        Ok(Some(op)) => op,
        Ok(None) => decode::bytes(target, payload),
        Err(refusal) => {
            cx.log_refused("a description", &refusal);
            decode::bytes(target, payload)
        }
    }
}

fn unread(target: &str, size: u64) -> Description {
    Description {
        title: decode::sized(target, size),
        fields: Vec::new(),
    }
}
