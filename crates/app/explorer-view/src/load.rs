//! The loaders: the head and the window it pulls (`chain.status`,
//! `chain.blocks`), the lists the system programs answer, and what an op
//! says it does (`program.describe`). A list is read with `cx.load`,
//! which keeps what is on screen while it reads again and draws only what
//! changed.
use view::prelude::*;

use crate::chain::{BlockRow, TxRow};
use crate::state::{Route, blob_ref};
use crate::{Explorer, WINDOW, decode, queries};

impl Explorer {
    /// Everything, again: the boot, a restore, a retry.
    pub(crate) fn read_all(&mut self, cx: &mut Context<Self>) {
        self.read_head(cx);
        self.read_accounts(cx);
        self.read_validators(cx);
        self.read_network(cx);
        self.read_used(cx);
        self.read_acted(cx);
        self.manifest_of = None;
        self.read_manifest(cx);
        self.pull(cx);
    }

    /// What the code behind the program page on screen declares, asked once
    /// per code id (`blob.manifest`): on every draw, so a page opened before
    /// the roster landed asks when it does, and a page that names other
    /// code blanks the one shown. Nothing to ask while no program page is
    /// open or the roster does not name it.
    pub(crate) fn read_manifest(&mut self, cx: &mut Context<Self>) {
        let Route::Program(name) = &self.route else {
            return;
        };
        let Some(code) = self.code_of(name) else {
            return;
        };
        let id = blob_ref(&code);
        if self.manifest_of.as_deref() == Some(id.as_str()) {
            return;
        }
        self.manifest_of = Some(id.clone());
        self.manifest = Loadable::Idle;
        let ask = cx.host().ask::<BlobManifest>(id);
        cx.load(self, ask, |view| &mut view.manifest);
    }

    /// A head `chain.heads` pushed: the window follows it without a read,
    /// and the status moves to it when the page lands. A head is not drawn
    /// on its own: that redraw changed nothing (the page lands a tick later
    /// and draws once) and cost a full render of the page that was up, 74M
    /// fuel on Transactions, every block.
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

    pub(crate) fn read_used(&mut self, cx: &mut Context<Self>) {
        let work = queries::used(cx.host());
        cx.load(self, work, |view| &mut view.used);
    }

    pub(crate) fn read_acted(&mut self, cx: &mut Context<Self>) {
        let Route::Account(number) = self.route else {
            return;
        };
        let mut known = match self.acted(number) {
            Some(rows) => rows.to_vec(),
            None => {
                self.acted = Loadable::Idle;
                Vec::new()
            }
        };
        let keys = self
            .account(number)
            .map_or(&[][..], |account| account.keys());
        let signed = |tx: &&TxRow| keys.iter().any(|key| key.key == tx.signer);
        let windowed = self.chain.txs.iter().filter(signed);
        known.extend(windowed.cloned());
        let work = queries::acted(cx.host(), number, known);
        cx.load(self, work, |view| &mut view.acted);
    }

    pub(crate) fn pull(&mut self, cx: &mut Context<Self>) {
        if self.pulling {
            return;
        }
        let head = self
            .status
            .ready()
            .map(|status| status.height.max(self.head));
        let (before, limit) = match (self.chain.top(), head) {
            (None, _) => (None, u32::MAX),
            (Some(top), Some(head)) if head > top => {
                (None, u32::try_from(head - top).unwrap_or(u32::MAX))
            }
            _ if !self.chain.complete && self.chain.blocks.len() < WINDOW => {
                (self.chain.blocks.last().map(|block| block.height), u32::MAX)
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
        let Some(payload) = &tx.payload else {
            return;
        };
        if tx.op.get().is_some() || tx.asked.replace(true) {
            return;
        }
        let ask = cx
            .host()
            .ask::<ProgramDescribe>((tx.target.clone(), payload.clone()));
        let (hash, target, payload) = (tx.hash, tx.target.clone(), payload.clone());
        cx.land(ask, move |view, described, cx| {
            let op = worded(&target, &payload, described, cx);
            view.described(hash, op, cx);
        })
        .detach();
    }

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
        let (hash, target, size) = (tx.hash, tx.target.clone(), u64::from(tx.size));
        cx.land(read, move |view, read, cx| {
            let op = match read {
                Ok(Some((payload, described))) => worded(&target, &payload, described, cx),
                Ok(None) => unread(&target, size),
                Err(refusal) => {
                    cx.log_refused("a transaction's payload", &refusal);
                    unread(&target, size)
                }
            };
            view.described(hash, op, cx);
        })
        .detach();
    }

    fn described(&mut self, hash: [u8; 32], op: Description, cx: &mut Context<Self>) {
        // every row of the hash: a frame the node landed again is
        // the same op, drawn once per landing
        let rows = self
            .chain
            .txs
            .iter()
            .chain(self.opened_txs())
            .chain(self.acted_txs());
        for tx in rows.filter(|tx| tx.hash == hash) {
            let _ = tx.op.set(op.clone());
        }
        cx.notify();
    }

    pub(crate) fn tx(&self, hash: &[u8; 32]) -> Option<&TxRow> {
        self.chain
            .txs
            .iter()
            .find(|tx| &tx.hash == hash)
            .or_else(|| self.opened_tx(hash))
            .or_else(|| self.acted_txs().iter().find(|tx| &tx.hash == hash))
    }

    fn acted_txs(&self) -> &[TxRow] {
        self.acted.ready().map_or(&[], |acted| &acted.rows)
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
