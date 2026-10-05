//! State stored by the root view and its panes.
use chat::{ChannelInfo, MemberRow, MsgRow};
use ducktape_view_guest::{Entity, ListState, Loadable, TextField, WeakEntity};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

use crate::api::Session;
use crate::composer::Draft;
use crate::message::ChatMessage;
use chat::view::Names;

#[derive(Serialize, Deserialize, Default)]
pub struct Chat {
    pub(crate) session: Session,
    #[serde(skip)]
    pub(crate) names: Loadable<Names>,
    pub(crate) channels: Loadable<Vec<ChannelInfo>>,
    pub(crate) room: Option<Room>,
    pub(crate) drafts: BTreeMap<String, Draft>,
    /// the banner over the room: the last refusal, until the reader moves on
    pub(crate) notice: String,
    pub(crate) search: Search,
    pub(crate) create: Option<ChannelCreate>,
    pub(crate) details: Option<Details>,
    pub(crate) layout: Layout,
    pub(crate) reads: Reads,
    pub(crate) menu: Option<Menu>,
    /// The reaction picker's search and tab while it is open.
    #[serde(skip)]
    pub(crate) picker: Picker,
    /// The reader's reactions, newest first: the picker's frequent row.
    #[serde(default)]
    pub(crate) recent_emoji: Vec<String>,
    /// The sidebar's room list and each pane's messages: entities of their
    /// own, cached in the tree, built in `attach` from the root's state
    /// (nothing of theirs is in the snapshot).
    #[serde(skip)]
    pub(crate) rooms: Option<Entity<Rooms>>,
    #[serde(skip)]
    pub(crate) timeline: Option<Entity<Timeline>>,
    #[serde(skip)]
    pub(crate) thread_timeline: Option<Entity<Timeline>>,
    /// The item the arrows are on in the open message menu.
    #[serde(skip)]
    pub(crate) menu_cursor: usize,
    /// The line over the room saying a copy landed: not a refusal, so not
    /// the `notice` banner.
    #[serde(skip)]
    pub(crate) confirmation: String,
    pub(crate) copy: Option<CopyRange>,
    /// What the view follows (`watch.rs`); dropping them unsubscribes.
    #[serde(skip)]
    pub(crate) followers: Vec<ducktape_view_guest::Task<()>>,
    /// messages meant for the reader in rooms they have not read, by room
    #[serde(skip)]
    pub(crate) attention: BTreeMap<String, i64>,
    /// the tab badge last sent
    #[serde(skip)]
    pub(crate) badge: Option<i64>,
    /// `attention` was counted again from the read cursors since this view
    /// (re)started
    #[serde(skip)]
    pub(crate) recounted: bool,
    /// the channel list read again while the one on screen stays
    #[serde(skip)]
    pub(crate) rereading_channels: Option<ducktape_view_guest::Task<()>>,
}

#[derive(Serialize, Deserialize, Default)]
pub struct Room {
    pub(crate) id: String,
    pub(crate) messages: Loadable<Vec<MsgRow>>,
    pub(crate) members: Loadable<Vec<MemberRow>>,
    pub(crate) thread: Option<Thread>,
    /// sends the module accepted that the index has not shown yet; drawn
    /// after the fetched rows and dropped once a fetched row carries the id
    #[serde(skip)]
    pub(crate) pending: Vec<MsgRow>,
    pub(crate) has_older: bool,
    #[serde(skip)]
    pub(crate) older_loading: bool,
    /// opened around a landing seq: the window may not reach the head
    pub(crate) landed: bool,
    pub(crate) reaches_head: bool,
    pub(crate) at_tail: bool,
    /// the rows read again while the ones on screen stay
    #[serde(skip)]
    pub(crate) rereading: Option<ducktape_view_guest::Task<()>>,
}

#[derive(Serialize, Deserialize, Default)]
pub struct Thread {
    pub(crate) root: u64,
    pub(crate) replies: Loadable<Vec<MsgRow>>,
    pub(crate) has_more: bool,
    pub(crate) next: Option<Vec<u8>>,
    #[serde(skip)]
    pub(crate) more_loading: bool,
    /// the replies read again while the ones on screen stay
    #[serde(skip)]
    pub(crate) rereading: Option<ducktape_view_guest::Task<()>>,
}

#[derive(Serialize, Deserialize, Default)]
pub struct Search {
    pub(crate) draft: TextField,
    /// the query the hits answer; "" while no search stands
    pub(crate) query: String,
    pub(crate) hits: Loadable<Hits>,
    #[serde(skip)]
    pub(crate) more_loading: bool,
}

#[derive(Serialize, Deserialize, Default, Clone)]
pub struct Hits {
    pub(crate) rows: Vec<MsgRow>,
    pub(crate) capped: bool,
    pub(crate) has_more: bool,
    pub(crate) next_after: Option<Vec<u8>>,
}

#[derive(Serialize, Deserialize, Default)]
pub struct ChannelCreate {
    pub(crate) name: TextField,
    pub(crate) members_only: bool,
    pub(crate) error: String,
    #[serde(skip)]
    pub(crate) busy: bool,
}

#[derive(Serialize, Deserialize, Default)]
pub struct Details {
    pub(crate) name_draft: TextField,
    pub(crate) member_draft: TextField,
}

#[derive(Serialize, Deserialize)]
pub struct Layout {
    /// the pane's size, read from the window each render
    #[serde(skip)]
    pub(crate) viewport: (f32, f32),
    pub(crate) sidebar: f32,
    pub(crate) details: f32,
    pub(crate) thread: f32,
    /// where the pointer last pressed: a menu opens there
    pub(crate) press: (f32, f32),
}

impl Default for Layout {
    fn default() -> Self {
        Self {
            viewport: (0., 0.),
            sidebar: 236.,
            details: 320.,
            thread: 330.,
            press: (0., 0.),
        }
    }
}

/// The sidebar's width bounds; it never takes more than half the window.
const SIDEBAR_W: (f32, f32) = (180., 420.);
/// The details pane's width bounds.
const DETAILS_W: (f32, f32) = (260., 520.);
/// The thread pane's width bounds.
const THREAD_W: (f32, f32) = (280., 640.);
/// What a side pane leaves the room: its narrowest column and the dividers.
const ROOM_KEEPS_W: f32 = 320. + 20.;

impl Layout {
    pub(crate) fn clamp(&mut self) {
        let (w, _) = self.viewport;
        let (lo, hi) = SIDEBAR_W;
        self.sidebar = self.sidebar.clamp(lo, (w * 0.5).clamp(lo, hi));
        let side = w - self.sidebar - ROOM_KEEPS_W;
        let (lo, hi) = DETAILS_W;
        self.details = self.details.clamp(lo, side.clamp(lo, hi));
        let (lo, hi) = THREAD_W;
        self.thread = self.thread.clamp(lo, side.clamp(lo, hi));
    }

    /// Whether a side pane `side` wide fits beside the sidebar and the
    /// room's narrowest; else it floats over the room.
    pub(crate) fn docks(&self, side: f32) -> bool {
        ducktape_view_guest::design::docks(self.viewport.0, self.sidebar + ROOM_KEEPS_W, side)
    }
}

/// What the reader has read: per room, the head seq when they last had it on
/// screen. `boundary` is the cursor the open room was entered with — the
/// unread divider's row is the first message past it.
#[derive(Serialize, Deserialize, Default)]
pub struct Reads {
    pub(crate) cursors: BTreeMap<String, u64>,
    #[serde(skip)]
    pub(crate) visible: bool,
    #[serde(skip)]
    pub(crate) entering: bool,
    pub(crate) boundary: u64,
    /// The store key the cursors were loaded from; nothing is written back
    /// until they have been.
    #[serde(default)]
    pub(crate) kept: Option<String>,
    /// The cursors last written to the store.
    #[serde(skip)]
    pub(crate) written: BTreeMap<String, u64>,
}

#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Debug)]
pub enum Pane {
    Timeline,
    Thread,
}

#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Debug)]
pub enum Mode {
    /// chosen: the floating actions stay open
    Toolbar,
    More,
    Reactions,
    Editing,
    Delete,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Menu {
    pub(crate) pane: Pane,
    pub(crate) seq: u64,
    pub(crate) rev: u32,
    pub(crate) mode: Mode,
    pub(crate) at: (f32, f32),
}

/// The message the arrows are on in a pane's grid, and which of its cells:
/// its content (0) or one of its controls, recorded in paint order as the
/// row is drawn so Enter knows what the cell does. The newest message
/// until the arrows move.
#[derive(Default, Debug)]
pub struct Cursor {
    pub(crate) id: Option<String>,
    pub(crate) cell: usize,
    /// the controls of the message `controls_of`, the active row when it
    /// was last drawn; another message's are nobody's
    pub(crate) controls: Vec<Control>,
    pub(crate) controls_of: Option<String>,
}

impl Cursor {
    /// The controls of message `id`, if they are the ones recorded.
    pub(crate) fn controls_of(&self, id: &str) -> &[Control] {
        match self.controls_of.as_deref() == Some(id) {
            true => &self.controls,
            false => &[],
        }
    }
}

/// A control on a message card: what pressing it does.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Control {
    /// the block link in the header: `link`
    Height(String),
    /// a program post's "Open in …": `link`
    ProgramOpen(String),
    Reaction {
        emoji: String,
        add: bool,
    },
    AddReaction,
    Replies,
    Thread,
    ThumbsUp,
    React,
    More,
}

/// A pane's messages as an entity of its own, cached in the tree
/// (`entity.cached(..)`): it renders only when it, or the root pushing it a
/// fact that moved, called `cx.notify()`. It owns what it draws and what
/// its handlers change; what the root knows comes down as [`Shown`], and an
/// intent goes up through `chat`, the root's own handlers.
pub struct Timeline {
    pub(crate) pane: Pane,
    pub(crate) chat: WeakEntity<Chat>,
    /// What the root last showed it.
    pub(crate) shown: Shown,
    /// The message the arrows are on in this pane.
    pub(crate) cursor: Cursor,
    /// The message row under the pointer in this pane: the one that
    /// carries the action strip, beside a chosen one.
    pub(crate) hovered: Option<u64>,
    pub(crate) list: Option<ListState>,
    /// The row keys the list was last spliced to.
    pub(crate) rows: Vec<String>,
}

/// What the root shows a [`Timeline`]: its messages, folded with the names
/// the reader has, and the facts around them. Pushed on every root render
/// and compared there, so the pane renders only when one moved.
#[derive(Clone, Default, PartialEq)]
pub struct Shown {
    pub(crate) messages: Vec<ChatMessage>,
    /// The room's history is all here: the intro leads the rows.
    pub(crate) lead: bool,
    /// The intro's room name and, for a dm, its peer.
    pub(crate) intro: Option<(String, Option<String>)>,
    /// A thread that is only its root: "No replies yet" ends the rows.
    pub(crate) bare: bool,
    /// The unread divider's boundary, in the room.
    pub(crate) boundary: Option<u64>,
    /// The room "Jump to latest" opens, while the room shows one.
    pub(crate) jump: Option<String>,
    /// The copy range, when it is this pane's.
    pub(crate) copy: Option<CopyRange>,
    /// The chosen message (the open menu's), when it is this pane's.
    pub(crate) chosen: Option<u64>,
    pub(crate) writable: bool,
    pub(crate) names: Names,
    /// The link a program's own post opens, in a program's room.
    pub(crate) program_link: Option<String>,
}

/// The sidebar's rooms as an entity of its own, cached in the tree.
pub struct Rooms {
    pub(crate) chat: WeakEntity<Chat>,
    pub(crate) shown: RoomsShown,
    /// The room the arrows are on; the open room until they move.
    pub(crate) cursor: Option<String>,
}

/// What the root shows [`Rooms`]: the channels and the direct messages,
/// each as the row draws it, and the list's state.
#[derive(Clone, Default, PartialEq)]
pub struct RoomsShown {
    pub(crate) channels: Vec<RoomRow>,
    pub(crate) dms: Vec<RoomRow>,
    pub(crate) loading: bool,
    pub(crate) failed: Option<String>,
    pub(crate) create_open: bool,
}

/// One room's row in the sidebar.
#[derive(Clone, PartialEq)]
pub struct RoomRow {
    pub(crate) id: String,
    pub(crate) name: String,
    /// A direct room's peer and whether it is an agent.
    pub(crate) peer: Option<(u64, bool)>,
    pub(crate) unread: bool,
    pub(crate) selected: bool,
    pub(crate) members_only: bool,
    pub(crate) archived: bool,
}

impl Chat {
    pub(crate) fn timeline(&self, pane: Pane) -> &Entity<Timeline> {
        match pane {
            Pane::Timeline => &self.timeline,
            Pane::Thread => &self.thread_timeline,
        }
        .as_ref()
        .expect("attach built it")
    }

    pub(crate) fn rooms(&self) -> &Entity<Rooms> {
        self.rooms.as_ref().expect("attach built it")
    }

    /// Where a popup the keys open sits: a key has no pointer position, so
    /// it takes the top-right of the pane's message list, inside the window.
    pub(crate) fn key_spot(&self, pane: Pane) -> (f32, f32) {
        let (width, _) = self.layout.viewport;
        let side = match pane {
            Pane::Thread => 0.,
            Pane::Timeline if self.details.is_some() && self.room.is_some() => self.layout.details,
            Pane::Timeline if self.room.as_ref().is_some_and(|room| room.thread.is_some()) => {
                self.layout.thread
            }
            Pane::Timeline => 0.,
        };
        let side = if side > 0. && self.layout.docks(side) {
            side
        } else {
            0.
        };
        (width - side - 16., 96.)
    }
}

/// The reaction picker: what the search holds, which tab is open, and the
/// cell the arrows are on in one of its grids (the grid's id, the cell).
#[derive(Default, Debug)]
pub struct Picker {
    pub(crate) query: TextField,
    pub(crate) tab: usize,
    pub(crate) cursor: Option<(&'static str, usize)>,
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
pub struct CopyRange {
    pub(crate) pane: Pane,
    pub(crate) anchor: u64,
    pub(crate) head: u64,
}

impl CopyRange {
    pub(crate) fn holds(&self, pane: Pane, seq: u64) -> bool {
        self.pane == pane && seq >= self.anchor.min(self.head) && seq <= self.anchor.max(self.head)
    }
}
