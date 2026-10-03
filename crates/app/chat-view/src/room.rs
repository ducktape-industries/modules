//! The open room: opening, landing, paging history, the thread beside it and
//! what the reader has read.
use chat::{ChannelInfo, MsgRow, PageRequest, Principal, ask};
use ducktape_view_guest::Context;
use ducktape_view_guest::Loadable;
use ducktape_view_guest::host::Error;
use ducktape_view_guest::methods::Change;

use crate::composer::Target;
use crate::queries;
use crate::{Chat, PAGE, Pane, Room, Thread, WINDOW, links};

impl Chat {
    /// A room the reader chose: opened here, and named to the host so its
    /// tray, links and notices follow.
    pub(crate) fn choose(&mut self, id: String, cx: &mut Context<Self>) {
        self.create = None;
        self.search_clear();
        match links::channel_link(&self.session.chain_id, &id, None) {
            Some(link) => cx.host().open_link(&link),
            None => cx.host().log("no room link: the session names no chain"),
        }
        self.open(id, cx);
        // read to the head this view knows now, not at the next list: a
        // reader who opens a room and quits has read it
        if let Some(list) = self.channels.ready().cloned() {
            self.channels_arrived(list, cx);
            self.save_reads(cx);
        }
        self.settle_badge(cx);
    }

    pub(crate) fn open(&mut self, id: String, cx: &mut Context<Self>) {
        self.open_at(id, 0, cx);
    }

    /// The room at its live tail (`land` 0) or around a landing seq.
    pub(crate) fn open_at(&mut self, id: String, land: u64, cx: &mut Context<Self>) {
        let viewer = self.viewer();
        let same = self.room.as_ref().is_some_and(|room| room.id == id);
        if !same {
            self.reads.boundary = 0;
            self.reads.entering = self.reads.visible;
            self.menu = None;
            self.copy = None;
            self.details = None;
        }
        let room = self.room.get_or_insert_default();
        if !same {
            *room = Room {
                id: id.clone(),
                at_tail: true,
                reaches_head: true,
                ..Room::default()
            };
        }
        room.landed = land > 0;
        room.at_tail = land == 0;
        room.messages = if land > 0 {
            let host = cx.host();
            cx.load(
                async move {
                    let rows = queries::around(host, id, land, viewer).await?;
                    Ok(rows)
                },
                |chat| &mut room_of(chat).messages,
            )
        } else {
            let handle = cx.spawn(async move |this, cx| {
                let host = cx.host();
                let result = queries::roots(host, id, viewer, None, WINDOW).await;
                let _ = this.update(cx, |chat, cx| {
                    cx.notify();
                    chat.rows_arrived(result)
                });
            });
            Loadable::Loading(handle)
        };
        let members_id = self.room.as_ref().map(|r| r.id.clone()).unwrap_or_default();
        let room = room_of(self);
        room.members = cx.load(queries::members(cx.host(), members_id), |chat| {
            &mut room_of(chat).members
        });
    }

    /// The newest window landed: the rows, and whether older ones remain.
    fn rows_arrived(&mut self, result: Result<(Vec<MsgRow>, bool), Error>) {
        let Some(room) = &mut self.room else { return };
        match result {
            Ok((rows, has_older)) => {
                room.has_older = has_older;
                room.reaches_head = true;
                room.messages = Loadable::Ready(rows);
                room.settle();
            }
            Err(refusal) => room.messages = Loadable::Failed(refusal),
        }
    }

    /// Everything chat shows, read again: the channel list and the open
    /// room, after this reader's own op or a session change.
    pub(crate) fn refresh(&mut self, cx: &mut Context<Self>) {
        self.changed(None, cx);
    }

    /// A chat block landed: the channel list and what the open room shows
    /// are read again where the block wrote to what they read, as the
    /// program declares it; `None`, a reopened node link, reads everything
    /// again.
    pub(crate) fn changed(&mut self, change: Option<&Change>, cx: &mut Context<Self>) {
        let list = ask::Channels {
            page: PageRequest::default(),
        };
        if change.is_none_or(|change| change.touches::<chat::Chat, _>(&list)) {
            self.reread_channels(cx);
        }
        self.refresh_room(change, cx);
    }

    pub(crate) fn reread_channels(&mut self, cx: &mut Context<Self>) {
        let list = queries::channels(cx.host());
        self.rereading_channels = Some(cx.refresh(list, |chat, rooms, cx| match rooms {
            Ok(rooms) => chat.channels_landed(rooms, cx),
            Err(refusal) => cx.host().log_refused("chat", "the channel list", &refusal),
        }));
    }

    /// Re-read what the room shows that `change` wrote to (`None`: all of
    /// it), keeping the rows there until fresh land; rows that land as they
    /// were draw nothing. Each read is held by what it reads, so leaving the
    /// room or the thread drops it.
    fn refresh_room(&mut self, change: Option<&Change>, cx: &mut Context<Self>) {
        let touched = |read: &dyn program::Reads| {
            change.is_none_or(|change| change.touches::<chat::Chat, _>(read))
        };
        let viewer = self.viewer();
        let Some(room) = &mut self.room else { return };
        let id = room.id.clone();
        let page = PageRequest::default();
        // the window at the tail is a page of roots; a landed one is read
        // around its middle row
        let window = if room.landed {
            room.messages
                .ready()
                .and_then(|rows| rows.get(rows.len() / 2))
                .map(|row| row.seq)
                .filter(|seq| {
                    touched(&ask::MessagesAround {
                        channel_id: id.clone(),
                        seq: *seq,
                        viewer: viewer.clone(),
                        page: page.clone(),
                    })
                })
                .map(Some)
        } else {
            touched(&ask::Roots {
                channel_id: id.clone(),
                viewer: viewer.clone(),
                page: page.clone(),
            })
            .then_some(None)
        };
        if window == Some(None) {
            let shown = room
                .messages
                .ready()
                .map_or(WINDOW, |rows| rows.len().max(WINDOW));
            let rows = queries::roots(cx.host(), id.clone(), viewer.clone(), None, shown);
            room.rereading = Some(cx.refresh(rows, |chat, rows, cx| {
                let room = room_of(chat);
                match rows {
                    Ok((rows, has_older)) => {
                        let moved =
                            room.messages.ready() != Some(&rows) || room.has_older != has_older;
                        if moved {
                            room.has_older = has_older;
                            room.messages = Loadable::Ready(rows);
                        }
                        room.landed_rows(moved, cx);
                    }
                    Err(refusal) => cx.host().log_refused("chat", "the room", &refusal),
                }
            }));
        } else if let Some(Some(seq)) = window {
            // a landed window re-reads around its middle row, so reactions,
            // edits and reply counts land there too
            let rows = queries::around(cx.host(), id.clone(), seq, viewer.clone());
            room.rereading = Some(cx.refresh(rows, |chat, rows, cx| {
                let room = room_of(chat);
                match rows {
                    Ok(rows) => {
                        let moved = room.messages.ready() != Some(&rows);
                        if moved {
                            room.messages = Loadable::Ready(rows);
                        }
                        room.landed_rows(moved, cx);
                    }
                    Err(refusal) => cx.host().log_refused("chat", "the room", &refusal),
                }
            }));
        }
        if touched(&ask::Members {
            channel_id: id.clone(),
            page: page.clone(),
        }) {
            let roster = queries::members(cx.host(), id.clone());
            cx.reload(&mut room.members, roster, |chat| &mut room_of(chat).members);
        }
        if let Some(thread) = room.thread.as_mut()
            && touched(&ask::Thread {
                channel_id: id.clone(),
                root_seq: thread.root,
                viewer: viewer.clone(),
                page,
            })
        {
            let root = thread.root;
            let replies = queries::thread(cx.host(), id, root, viewer, None);
            thread.rereading = Some(cx.refresh(replies, move |chat, replies, cx| {
                let room = room_of(chat);
                let Some(thread) = room.thread.as_mut().filter(|thread| thread.root == root) else {
                    return;
                };
                match replies {
                    Ok((replies, next)) => {
                        let moved = thread.replies.ready() != Some(&replies) || thread.next != next;
                        if moved {
                            thread.replies = Loadable::Ready(replies);
                            thread.has_more = next.is_some();
                            thread.next = next;
                        }
                        room.landed_rows(moved, cx);
                    }
                    Err(refusal) => cx.host().log_refused("chat", "the thread", &refusal),
                }
            }));
        }
    }

    /// One older page before the oldest row on screen.
    pub(crate) fn load_older(&mut self, cx: &mut Context<Self>) {
        let viewer = self.viewer();
        let Some(room) = &mut self.room else { return };
        let Some(oldest) = room
            .messages
            .ready()
            .and_then(|rows| rows.first())
            .map(|r| r.seq)
        else {
            return;
        };
        if !room.has_older || room.older_loading || room.landed {
            return;
        }
        room.older_loading = true;
        let id = room.id.clone();
        cx.spawn(async move |this, cx| {
            let host = cx.host();
            let below = chat::roots_below(&id, oldest);
            let result = queries::roots(host, id, viewer, Some(below), PAGE).await;
            let _ = this.update(cx, |chat, cx| {
                cx.notify();
                let Some(room) = &mut chat.room else { return };
                room.older_loading = false;
                match result {
                    Ok((older, has_older)) => {
                        room.has_older = has_older;
                        if let Some(rows) = room.messages.ready_mut() {
                            let mut all = older;
                            all.append(rows);
                            *rows = all;
                        }
                    }
                    Err(refusal) => {
                        chat.notice = format!("Couldn’t read this room: {}", refusal.message)
                    }
                }
            });
        })
        .detach();
    }

    /// Settled native list geometry drives paging and tail state. Wheel deltas
    /// are intentionally not used: remeasurement and programmatic scrolling
    /// can move the viewport without one.
    pub(crate) fn list_scrolled(
        &mut self,
        pane: Pane,
        event: &ducktape_view_guest::ListScrollEvent,
        cx: &mut Context<Self>,
    ) {
        match pane {
            Pane::Timeline => {
                let Some(room) = &mut self.room else { return };
                room.at_tail = event.is_following_tail || event.visible_range.end >= event.count;
                if event.visible_range.start <= 4 {
                    self.load_older(cx);
                }
            }
            Pane::Thread => {
                if event.visible_range.end.saturating_add(4) >= event.count {
                    self.load_more_replies(cx);
                }
            }
        }
    }

    pub(crate) fn open_thread(&mut self, root: u64, cx: &mut Context<Self>) {
        let viewer = self.viewer();
        self.details = None;
        self.menu = None;
        self.copy = None;
        let Some(room) = &mut self.room else { return };
        let id = room.id.clone();
        let thread = room.thread.get_or_insert_default();
        *thread = Thread {
            root,
            ..Thread::default()
        };
        let handle = cx.spawn(async move |this, cx| {
            let host = cx.host();
            let result = queries::thread(host, id.clone(), root, viewer, None).await;
            let _ = this.update_in(cx, |chat, window, cx| {
                cx.notify();
                let Some(thread) = chat.room.as_mut().and_then(|r| r.thread.as_mut()) else {
                    return;
                };
                if thread.root != root {
                    return;
                }
                match result {
                    Ok((replies, next)) => {
                        thread.replies = Loadable::Ready(replies);
                        thread.has_more = next.is_some();
                        thread.next = next;
                        room_of(chat).settle();
                        // the reply field takes the keys once it can be typed in
                        let key = Target::Post {
                            channel: id,
                            thread: Some(root),
                        }
                        .key();
                        crate::composer::focus(window, &key);
                    }
                    Err(refusal) => thread.replies = Loadable::Failed(refusal),
                }
            });
        });
        let Some(thread) = self.room.as_mut().and_then(|r| r.thread.as_mut()) else {
            return;
        };
        thread.replies = Loadable::Loading(handle);
    }

    pub(crate) fn load_more_replies(&mut self, cx: &mut Context<Self>) {
        let viewer = self.viewer();
        let Some(room) = &mut self.room else { return };
        let id = room.id.clone();
        let Some(thread) = &mut room.thread else {
            return;
        };
        if !thread.has_more || thread.more_loading {
            return;
        }
        thread.more_loading = true;
        let (root, after) = (thread.root, thread.next.clone());
        cx.spawn(async move |this, cx| {
            let host = cx.host();
            let result = queries::thread(host, id, root, viewer, after).await;
            let _ = this.update(cx, |chat, cx| {
                cx.notify();
                let Some(thread) = chat.room.as_mut().and_then(|r| r.thread.as_mut()) else {
                    return;
                };
                thread.more_loading = false;
                match result {
                    Ok(_) if thread.root != root => {}
                    Ok((more, next)) => {
                        thread.has_more = next.is_some();
                        thread.next = next;
                        if let Some(rows) = thread.replies.ready_mut() {
                            rows.extend(more);
                        }
                    }
                    Err(refusal) => {
                        chat.notice = format!("Couldn’t read this thread: {}", refusal.message)
                    }
                }
            });
        })
        .detach();
    }

    pub(crate) fn close_thread(&mut self) {
        if let Some(room) = &mut self.room {
            room.thread = None;
        }
        if self
            .menu
            .as_ref()
            .is_some_and(|menu| menu.pane == Pane::Thread)
        {
            self.menu = None;
        }
        if self.copy.is_some_and(|copy| copy.pane == Pane::Thread) {
            self.copy = None;
        }
    }

    /// The channel list landed: the rooms, and what each one's head says
    /// about what the reader has read. The room on screen is read to its
    /// head, and the host's notices for it with it.
    pub(crate) fn channels_arrived(&mut self, channels: Vec<ChannelInfo>, cx: &mut Context<Self>) {
        let reading = self
            .room
            .as_ref()
            .filter(|room| self.reads.visible && !room.landed)
            .map(|room| room.id.clone());
        let mut read = None;
        // what the list, its unread marks and the divider draw
        let mut moved = self.channels.ready() != Some(&channels);
        for info in &channels {
            let cursor = match self.reads.cursors.entry(info.channel.id.clone()) {
                std::collections::btree_map::Entry::Occupied(cursor) => cursor.into_mut(),
                std::collections::btree_map::Entry::Vacant(cursor) => {
                    moved = true;
                    cursor.insert(info.head_seq)
                }
            };
            if reading.as_deref() == Some(info.channel.id.as_str()) {
                if self.reads.entering && info.head_seq > *cursor {
                    self.reads.boundary = *cursor;
                }
                self.reads.entering = false;
                if info.head_seq > *cursor {
                    *cursor = info.head_seq;
                    read = Some(info.channel.id.clone());
                    moved = true;
                }
            }
        }
        // a room gone from the list takes its cursor with it: the kept map
        // is written whole, and must not grow with every room ever seen.
        let listed: std::collections::HashSet<&str> = channels
            .iter()
            .map(|info| info.channel.id.as_str())
            .collect();
        let kept = self.reads.cursors.len();
        self.reads
            .cursors
            .retain(|room, _| listed.contains(room.as_str()));
        if moved || self.reads.cursors.len() != kept {
            self.channels = Loadable::Ready(channels);
            cx.notify();
        }
        if let Some(room) = read {
            self.read_notices(&room, cx);
        }
    }

    pub(crate) fn unread(&self, info: &ChannelInfo) -> bool {
        self.reads
            .cursors
            .get(&info.channel.id)
            .is_some_and(|cursor| info.head_seq > *cursor)
    }

    pub(crate) fn room_id(&self) -> String {
        self.room
            .as_ref()
            .map(|room| room.id.clone())
            .unwrap_or_default()
    }

    pub(crate) fn info(&self, id: &str) -> Option<&ChannelInfo> {
        self.channels
            .ready()?
            .iter()
            .find(|info| info.channel.id == id)
    }

    pub(crate) fn room_info(&self) -> Option<&ChannelInfo> {
        self.info(&self.room.as_ref()?.id)
    }

    /// The open room's members, as the roster names them.
    pub(crate) fn roster(&self) -> Vec<(Principal, String)> {
        let (Some(names), Some(members)) = (
            self.names.ready(),
            self.room.as_ref().and_then(|room| room.members.ready()),
        ) else {
            return Vec::new();
        };
        members
            .iter()
            .map(|row| (row.principal.clone(), names.member(&row.principal)))
            .collect()
    }
}

impl Room {
    /// Fresh rows landed: a pending send the index now serves leaves.
    pub(crate) fn settle(&mut self) {
        let shown = |id: &str, rows: Option<&Vec<MsgRow>>| {
            rows.is_some_and(|rows| rows.iter().any(|row| row.message_id == id))
        };
        let replies = self.thread.as_ref().and_then(|t| t.replies.ready());
        let messages = self.messages.ready();
        self.pending
            .retain(|p| !shown(&p.message_id, messages) && !shown(&p.message_id, replies));
    }

    /// A re-read landed: drops the sends it shows, and draws if the rows
    /// `moved` or a send left.
    fn landed_rows(&mut self, moved: bool, cx: &mut Context<Chat>) {
        let pending = self.pending.len();
        self.settle();
        if moved || self.pending.len() != pending {
            cx.notify();
        }
    }
}

pub(crate) fn room_of(chat: &mut Chat) -> &mut Room {
    chat.room.get_or_insert_default()
}
