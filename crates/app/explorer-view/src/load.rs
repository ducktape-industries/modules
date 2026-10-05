//! The loaders: the head and the window it pulls (`chain.status`,
//! `chain.blocks`), the lists the system programs answer, and what an op
//! says it does (`module.describe`). A list is read with `cx.load`,
//! which keeps what is on screen while it reads again and draws only what
//! changed.
use ducktape_view_guest::prelude::*;

use crate::chain::{BlockRow, TxRow};
use crate::{Explorer, PAGE, WINDOW, decode, queries};

impl Explorer {
    /// Everything, again: the boot, a restore, a retry.
    pub(crate) fn read_all(&mut self, cx: &mut Context<Self>) {
        self.read_head(cx);
        self.read_accounts(cx);
        self.read_validators(cx);
        self.read_network(cx);
        self.pull(cx);
    }

    /// A head `chain.heads` pushed: the window follows it without a read,
    /// and the status moves to it when the page lands. A head is not drawn
    /// on its own: that redraw changed nothing (the page lands a tick later
    /// and draws once) and cost a full render of the page that was up, 74M
    /// fuel on Transactions, every block.
    pub(crate) fn at_head(&mut self, head: Head, cx: &mut Context<Self>) {
        match &self.status {
            Loadable::Ready(_) | Loadable::Reloading(_, _) => {
                self.head = self.head.max(head.height);
                self.pull(cx);
            }
            _ => self.read_head(cx),
        }
    }

    /// The node's status, and the window it moves. A status on screen stays
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
        // a failed window is read again with the head, not in a loop
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

    /// Reads the next page the window wants, if any: the blocks since the
    /// top when the head is past it (one new block is one block, at most a
    /// page), else older blocks until the window is full.
    pub(crate) fn pull(&mut self, cx: &mut Context<Self>) {
        if self.pulling {
            return;
        }
        let head = self
            .status
            .ready()
            .map(|status| status.height.max(self.head));
        let (before, limit) = match (self.chain.top(), head) {
            (None, _) => (None, PAGE),
            (Some(top), Some(head)) if head > top => {
                (None, (head - top).min(u64::from(PAGE)) as u32)
            }
            _ if !self.chain.complete && self.chain.blocks.len() < WINDOW => {
                (self.chain.blocks.last().map(|block| block.height), PAGE)
            }
            _ => return,
        };
        self.pulling = true;
        let ask = cx.host().ask::<ChainBlocks>(BlockPage { before, limit });
        cx.land(ask, move |view, page, cx| {
            view.pulling = false;
            match page {
                Ok(page) => {
                    let was = (view.chain.top(), view.chain.blocks.len());
                    view.chain.land(before, page);
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
                    if (view.chain.top(), view.chain.blocks.len()) != was {
                        view.pull(cx);
                    }
                }
                Err(refusal) => view.chain.failed = Some(refusal.message),
            }
            cx.notify();
        })
        .detach();
    }

    /// Asks the host what `tx` does, once, the first time it is drawn; a
    /// program that says nothing, or a refusal, leaves its bytes.
    ///
    /// ponytail: drawing asks (through `TxRow`'s cells) so only rows on
    /// screen are described, not every transaction of a 1,000-block
    /// window; a render that stays pure needs the visible rows computed
    /// ahead of it.
    pub(crate) fn describe(&self, tx: &TxRow, cx: &mut Context<Self>) {
        if tx.op.get().is_some() || tx.asked.replace(true) {
            return;
        }
        let ask = cx
            .host()
            .ask::<ModuleDescribe>((tx.target.clone(), tx.payload.clone()));
        let hash = tx.hash;
        cx.land(ask, move |view, described, cx| {
            let Some(tx) = view.tx(&hash) else {
                return;
            };
            let op = match described {
                Ok(Some(op)) => op,
                Ok(None) => decode::bytes(&tx.target, &tx.payload),
                Err(refusal) => {
                    cx.log_refused("a description", &refusal);
                    decode::bytes(&tx.target, &tx.payload)
                }
            };
            // every row of the hash: a frame the node landed again is
            // the same op, drawn once per landing
            let rows = view.chain.txs.iter().chain(view.opened_txs());
            for tx in rows.filter(|tx| tx.hash == hash) {
                let _ = tx.op.set(op.clone());
            }
            cx.notify();
        })
        .detach();
    }

    /// A transaction by hash, in the window or the block opened outside it.
    pub(crate) fn tx(&self, hash: &[u8; 32]) -> Option<&TxRow> {
        self.chain
            .txs
            .iter()
            .find(|tx| &tx.hash == hash)
            .or_else(|| self.opened_tx(hash))
    }

    pub(crate) fn opened_tx(&self, hash: &[u8; 32]) -> Option<&TxRow> {
        self.opened_txs().iter().find(|tx| &tx.hash == hash)
    }

    fn opened_txs(&self) -> &[TxRow] {
        match self.opened.ready() {
            Some(Some((_, txs))) => txs,
            _ => &[],
        }
    }

    /// A block and its transactions, in the window or opened outside it.
    pub(crate) fn block(&self, height: u64) -> Option<(BlockRow, Vec<&TxRow>)> {
        if let Some(row) = self.chain.block(height) {
            let txs = self.chain.txs.iter().filter(|tx| tx.height == height);
            return Some((row.clone(), txs.collect()));
        }
        match self.opened.ready() {
            Some(Some((row, txs))) if row.height == height => {
                Some((row.clone(), txs.iter().collect()))
            }
            _ => None,
        }
    }
}
