//! Where the reader goes: a tab or a row, a search, a link opened into the
//! explorer, and a page's link copied out of it.
use view::prelude::*;

use crate::Explorer;
use crate::chain::rows;
use crate::state::{Note, Route, hash_of};

enum Found {
    Block,
    Tx,
}

impl Explorer {
    pub(crate) fn go(&mut self, route: Route, cx: &mut Context<Self>) {
        if let Route::Block(height) = route {
            let held = self.chain.block(height).is_some();
            let opened =
                matches!(self.opened.ready(), Some(Some((row, _))) if row.height == height);
            if !held && !opened {
                let ask = cx.host().ask::<ChainBlock>(BlockRef::Height(height));
                // another block: the one opened before is not shown under it
                self.opened = Loadable::Idle;
                cx.load(
                    self,
                    async move { ask.await.map(|block| block.map(rows)) },
                    |view| &mut view.opened,
                );
            }
        }
        self.left[route.tab()] = Some(route.clone());
        self.route = route;
        self.read_acted(cx);
        self.note = None;
        self.cursor = None;
        self.scroll.scroll_to_item(0, ScrollStrategy::Top);
        // the field holds only what is being typed: a search that lands, a
        // tab, prev/next and a row all leave it empty
        self.search.reset("");
        cx.notify();
    }

    /// A height, a block or transaction hash, an account (`#3` or a name)
    /// or a program name.
    pub(crate) fn search(&mut self, cx: &mut Context<Self>) {
        let query = self.search.text().trim().to_string();
        self.note = None;
        if query.is_empty() {
            return;
        }
        let digits: String = query.chars().filter(|c| *c != ',').collect();
        if let Ok(height) = digits.parse::<u64>() {
            return self.go(Route::Block(height), cx);
        }
        if let Some(hash) = hash_of(&query) {
            return self.find_hash(hash, cx);
        }
        let number = query
            .strip_prefix('#')
            .and_then(|n| n.trim().parse::<u64>().ok());
        let lower = query.to_lowercase();
        let accounts = self.account_list();
        let named = |account: &&identity::Account| account.card.name.to_lowercase() == lower;
        let account = accounts
            .iter()
            .find(|account| Some(account.number) == number || named(account))
            .or_else(|| {
                accounts
                    .iter()
                    .find(|account| account.card.name.to_lowercase().contains(&lower))
            });
        if let Some(account) = account {
            return self.go(Route::Account(account.number), cx);
        }
        if let Some(entry) = self.runs(&lower) {
            return self.go(Route::Transactions(Some(entry.program.clone())), cx);
        }
        self.note = Some(Note::NotFound(query));
        cx.notify();
    }

    /// A hash, as search and a link read it: a transaction or a block in the
    /// window, else a block or a transaction the node finds by it.
    fn find_hash(&mut self, hash: [u8; 32], cx: &mut Context<Self>) {
        if self.tx(&hash).is_some() {
            return self.go(Route::Tx(hash), cx);
        }
        if let Some(block) = self.chain.blocks.iter().find(|block| block.id == hash) {
            return self.go(Route::Block(block.height), cx);
        }
        let host = cx.host();
        let found = async move {
            if let Some(block) = host.ask::<ChainBlock>(BlockRef::Id(hash)).await? {
                return Ok::<_, Error>(Some((block, Found::Block)));
            }
            let by_hash = TxAsk {
                tx: TxRef::Hash(hash),
                most: 0,
            };
            let Some(located) = host.ask::<ChainTx>(by_hash).await? else {
                return Ok(None);
            };
            let block = host
                .ask::<ChainBlock>(BlockRef::Height(located.height))
                .await?;
            Ok(block.map(|block| (block, Found::Tx)))
        };
        let listed_from = self.chain.listed_from;
        cx.land(found, move |view, found, cx| {
            match found {
                Ok(Some((block, found))) => {
                    let (row, txs) = rows(block);
                    let route = match found {
                        Found::Block => Route::Block(row.height),
                        Found::Tx => Route::Tx(hash),
                    };
                    view.opened = Loadable::Ready(Some((row, txs)));
                    view.go(route, cx);
                }
                Ok(None) => view.note = Some(Note::NoSuchHash { listed_from }),
                Err(refusal) => view.note = Some(Note::Refused(refusal.message)),
            }
            cx.notify();
        })
        .detach();
    }

    /// A `host.route` item, the path of a `duck://…/explorer/<route>` link:
    /// a [`Route::path`], or `block/<hash>`, found like a searched hash.
    pub(crate) fn open_route(&mut self, route: &str, cx: &mut Context<Self>) {
        if let Some(opened) = Route::from_path(route) {
            return match opened {
                Route::Tx(hash) if self.tx(&hash).is_none() => self.find_hash(hash, cx),
                opened => self.go(opened, cx),
            };
        }
        if let Some(hash) = route.strip_prefix("block/").and_then(hash_of) {
            return self.find_hash(hash, cx);
        }
        self.note = Some(Note::Unlinked(route.to_owned()));
        cx.notify();
    }

    /// `duck://<chain>/explorer/<route>`, while the session names a chain.
    pub(crate) fn link(&self, route: &Route) -> Option<String> {
        let path = route.path();
        let tail: Vec<&str> = path
            .split('/')
            .filter(|segment| !segment.is_empty())
            .collect();
        ducklink::mint(&self.session_chain, "explorer", &tail)
    }

    pub(crate) fn copy_link(&mut self, link: String, cx: &mut Context<Self>) {
        let ask = cx.host().ask::<ClipboardWrite>(link);
        cx.land(ask, move |view, copied, cx| {
            view.note = Some(match copied {
                Ok(()) => Note::Copied,
                Err(refusal) => Note::Refused(refusal.message),
            });
            cx.notify();
        })
        .detach();
    }
}
