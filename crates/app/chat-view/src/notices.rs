//! What chat tells the host about the reader's notices, and the tab badge.
//! Chat's program writes its notices on chain and the host's inbox carries
//! them; this view only says how far the reader has been shown each tag
//! (`notify.seen`), so the host marks what is under it read. The tab badge
//! counts the messages meant for the reader in rooms still unread.
use std::collections::BTreeMap;

use chat::{Block, ChannelInfo, Mark, MsgRow, Principal};
use ducktape_view_guest::methods::Seen;
use ducktape_view_guest::prelude::*;

use crate::Chat;
use crate::names::dm_peer_of;

/// The most new messages read out of one room per change.
const MAX_NEW: u64 = 20;

impl Chat {
    /// The channel list landed: the rooms, then what moved in a room the
    /// reader is not looking at is counted on the badge.
    pub(crate) fn channels_landed(&mut self, channels: Vec<ChannelInfo>, cx: &mut Context<Self>) {
        // before the first list there is nothing to compare against: what
        // was already there is history, not news
        let before: Option<BTreeMap<String, u64>> = self.channels.ready().map(|list| {
            list.iter()
                .map(|info| (info.channel.id.clone(), info.head_seq))
                .collect()
        });
        self.channels_arrived(channels, cx);
        if let Some(me) = self.my_account() {
            // the count is not kept, the read cursors are: the first list
            // a view that started over (a reload carries its state, not the
            // count) sees counts again what is meant for them in each room
            // still unread
            let recount = !std::mem::replace(&mut self.recounted, true);
            let viewing = self.viewing();
            let news: Vec<(String, u64, u64)> = self
                .channels
                .ready()
                .into_iter()
                .flatten()
                .filter(|info| Some(info.channel.id.as_str()) != viewing.as_deref())
                .filter_map(|info| {
                    let (id, head) = (&info.channel.id, info.head_seq);
                    let seen = before
                        .as_ref()
                        .map_or(head, |before| before.get(id).copied().unwrap_or(0));
                    let cursor = match recount {
                        true => self.reads.cursors.get(id).copied().unwrap_or(head),
                        false => head,
                    };
                    let from = seen.min(cursor);
                    (head > from).then(|| (id.clone(), from, head))
                })
                .collect();
            for (channel, from, head) in news {
                self.read_news(channel, from, head, me, cx);
            }
        }
        self.settle_badge(cx);
        self.save_reads(cx);
    }

    /// The room on screen, if the reader can see it.
    pub(crate) fn viewing(&self) -> Option<String> {
        self.room
            .as_ref()
            .filter(|_| self.reads.visible)
            .map(|room| room.id.clone())
    }

    /// Reads the messages past `was` in `channel` and counts the ones meant
    /// for the reader.
    fn read_news(&mut self, channel: String, was: u64, head: u64, me: u64, cx: &mut Context<Self>) {
        let fresh = (head - was).min(MAX_NEW);
        let viewer = self.viewer();
        let asked = cx.host().query(chat::ask::MessagesAround {
            channel_id: channel.clone(),
            seq: head,
            viewer,
            page: chat::PageRequest {
                after: None,
                limit: Some(fresh * 2),
            },
        });
        cx.land(asked, move |chat, asked, cx| {
            cx.notify();
            let rows = match asked {
                Ok(rows) => rows,
                Err(refusal) => return cx.log_refused("news", &refusal),
            };
            let meant = rows
                .iter()
                .filter(|row| row.seq > was && row.seq <= head && meant_for(row, me))
                .count();
            if meant == 0 || chat.viewing().as_deref() == Some(channel.as_str()) {
                return;
            }
            *chat.attention.entry(channel).or_default() += meant as i64;
            chat.settle_badge(cx);
        })
        .detach();
    }

    /// How far the reader has been shown each tag, to the host
    /// (`notify.seen`): each pane's newest row under its own tag
    /// (`chat::notify::tag`), the timeline's roots under `<channel>` and an
    /// open thread's replies under `<channel>/<root>`, so reading a room
    /// leaves its threads' notices unread and the reverse. Only while the
    /// room is on screen, only for a pane that holds its newest rows (the
    /// timeline opened at its head, not around a landing; a thread read to
    /// its last page), and only past what was sent.
    pub(crate) fn settle_seen(&mut self, cx: &mut Context<Self>) {
        let Some(room) = self.room.as_ref().filter(|_| self.reads.visible) else {
            return;
        };
        let timeline = room.messages.ready().filter(|_| !room.landed);
        let thread = room
            .thread
            .as_ref()
            .filter(|thread| !thread.has_more)
            .and_then(|thread| thread.replies.ready());
        let shown: Vec<(String, u64)> = [timeline, thread]
            .into_iter()
            .filter_map(|rows| rows?.last())
            .map(|row| (chat::notify::tag(row), row.height))
            .collect();
        for (tag, through_height) in shown {
            if self
                .seen
                .get(&tag)
                .is_some_and(|sent| *sent >= through_height)
            {
                continue;
            }
            self.seen.insert(tag.clone(), through_height);
            cx.host().notify::<NotifySeen>(Seen {
                tag,
                through_height,
            });
        }
    }

    /// The tab badge: messages meant for the reader in rooms still unread.
    /// A room read, or on screen, drops out.
    pub(crate) fn settle_badge(&mut self, cx: &mut Context<Self>) {
        let viewing = self.viewing();
        let read: Vec<String> = self
            .attention
            .keys()
            .filter(|id| {
                Some(id.as_str()) == viewing.as_deref()
                    || self.info(id).is_none_or(|info| !self.unread(info))
            })
            .cloned()
            .collect();
        for id in read {
            self.attention.remove(&id);
        }
        let count: i64 = self.attention.values().sum();
        if self.badge != Some(count) {
            self.badge = Some(count);
            cx.host().notify::<HostBadge>(count);
        }
    }
}

/// Whether `row` is meant for account `me`, as the badge counts it: it
/// mentions them, or it is in a direct room they are in. Their own never
/// is, nor a deleted one.
fn meant_for(row: &MsgRow, me: u64) -> bool {
    !row.deleted
        && row.author != Principal::Account(me)
        && (dm_peer_of(me, &row.channel_id).is_some() || mentions(&row.blocks, me))
}

fn mentions(blocks: &[Block], me: u64) -> bool {
    blocks.iter().any(|block| match block {
        Block::Paragraph(spans) | Block::Quote(spans) => spans
            .iter()
            .any(|span| span.marks.contains(&Mark::Mention(Principal::Account(me)))),
        Block::Code { .. } | Block::Divider => false,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use chat::Span;

    fn row(channel: &str, author: u64, blocks: Vec<Block>) -> MsgRow {
        MsgRow {
            channel_id: channel.into(),
            seq: 7,
            blocks,
            ..MsgRow::by(Principal::Account(author))
        }
    }

    fn said(text: &str, marks: Vec<Mark>) -> Vec<Block> {
        vec![Block::Paragraph(vec![Span {
            text: text.into(),
            marks,
        }])]
    }

    /// A mention of me, or any word in my direct room, from someone else:
    /// meant for me. A mention of another, my own words, a plain message in
    /// a channel, or a deleted one: not.
    #[test]
    fn only_a_mention_or_a_direct_message_to_me_is_meant_for_me() {
        let me = Mark::Mention(Principal::Account(3));
        assert!(meant_for(
            &row("design", 5, said("@me", vec![me.clone()])),
            3
        ));
        let other = Mark::Mention(Principal::Account(4));
        assert!(!meant_for(&row("design", 5, said("@x", vec![other])), 3));
        assert!(!meant_for(&row("design", 5, said("hi", vec![])), 3));
        assert!(!meant_for(&row("design", 3, said("@me", vec![me])), 3));
        assert!(meant_for(&row("dm-3-5", 5, said("hi", vec![])), 3));
        assert!(!meant_for(&row("dm-4-5", 5, said("hi", vec![])), 3));
        let gone = MsgRow {
            deleted: true,
            ..row("dm-3-5", 5, said("hi", vec![]))
        };
        assert!(!meant_for(&gone, 3));
    }
}
