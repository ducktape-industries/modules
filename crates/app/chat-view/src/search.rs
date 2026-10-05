//! Message search: `#tag` pages through the tag index, anything else is a
//! full-text search the program caps.
use ducktape_view_guest::{Context, Loadable};

use crate::queries::search_hits;
use crate::{Chat, Hits, Search};

impl Chat {
    pub(crate) fn search_submit(&mut self, cx: &mut Context<Self>) {
        let query = self.search.draft.text.trim().to_owned();
        if query.is_empty() {
            return;
        }
        self.search.query = query;
        self.search_now(cx);
    }

    pub(crate) fn search_now(&mut self, cx: &mut Context<Self>) {
        let (text, viewer) = (self.search.query.clone(), self.viewer());
        let host = cx.host();
        // a new search: the hits shown answer another question
        self.search.hits = Loadable::Idle;
        cx.load(
            self,
            async move {
                let (rows, capped, next_after) =
                    search_hits(host, text, None, viewer, None).await?;
                Ok(Hits {
                    rows,
                    capped,
                    has_more: next_after.is_some(),
                    next_after,
                })
            },
            |chat| &mut chat.search.hits,
        );
    }

    pub(crate) fn search_more(&mut self, cx: &mut Context<Self>) {
        let Some(after) = self.search.hits.ready().and_then(|h| h.next_after.clone()) else {
            return;
        };
        if self.search.more_loading {
            return;
        }
        self.search.more_loading = true;
        let (text, viewer) = (self.search.query.clone(), self.viewer());
        let more = search_hits(cx.host(), text, None, viewer, Some(after));
        cx.land(more, |chat, result, cx| {
            cx.notify();
            chat.search.more_loading = false;
            let (rows, _, next_after) = match result {
                Ok(page) => page,
                Err(refusal) => {
                    chat.notice = format!("Couldn’t search further: {}", refusal.message);
                    return;
                }
            };
            if let Some(hits) = chat.search.hits.ready_mut() {
                for row in rows {
                    if !hits
                        .rows
                        .iter()
                        .any(|h| h.channel_id == row.channel_id && h.seq == row.seq)
                    {
                        hits.rows.push(row);
                    }
                }
                hits.has_more = next_after.is_some();
                hits.next_after = next_after;
            }
        })
        .detach();
    }

    pub(crate) fn search_clear(&mut self) {
        self.search = Search::default();
    }

    /// A hit opens its room around the message.
    pub(crate) fn open_hit(&mut self, channel_id: String, seq: u64, cx: &mut Context<Self>) {
        self.search_clear();
        self.create = None;
        self.open_at(channel_id, seq, cx);
    }
}
