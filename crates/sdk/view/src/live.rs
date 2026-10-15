//! Live tracks: what a view publishes to, and hears from, the people in its
//! program's rooms, through the connected node and off consensus. Four
//! methods carry it ([`LivePublishTrack`], [`LiveSendObject`],
//! [`LiveSubscribeTrack`], [`LiveTracks`](crate::methods::LiveTracks));
//! this module is the two helpers that earn a place over them: [`Track`],
//! which owns a publication and numbers its groups and objects, and
//! [`subscribe`], which decodes what a track carries. Each takes the
//! method's own request, so a helper and a raw `cx.follow` spell an ask the
//! same way. Who is in a room is `cx.follow::<LiveTracks>(room, ..)` as it
//! is.
//!
//! The node lets an account publish or subscribe in a room only as the
//! view's program granted it, in rows the program writes
//! (`program::live::grant`). A view with no program behind it has no one to
//! write them, and opens no room.
//!
//! The node reads those rows from blocks, not from the preconfirmed state a
//! `Submit<P>` answers with, and holds an ask its rows do not cover yet
//! until a block's do: a view asks once. Such an ask hears `Waiting`
//! ([`Publishing::Waiting`], [`Received::Waiting`],
//! [`Watching::Waiting`](crate::methods::Watching::Waiting)), and the node
//! answers it on the block that writes the row, as if just asked. A view
//! that opens a room sends the op writing its rows on each `Waiting` (a
//! reconnect that still waits tells it again); what it sends onto a
//! waiting track goes out once the track is up.
//! `not_granted` is this account's grant taken back: the request's last
//! item; a subscription whose publisher lost its grant ends
//! `publisher_gone`.
//!
//! A publication and a subscription are tasks: keep them beside the state
//! they feed and build them in [`View::attach`](crate::View::attach), which
//! runs again after a restore; dropping one ends the track or the
//! subscription. A body is the view's own borsh type; a body another
//! version of the view wrote that does not decode reaches the handler as
//! `Err` (`unexpected_reply`).
use std::marker::PhantomData;

use borsh::{BorshDeserialize, BorshSerialize};

use crate::host::{Error, malformed};
use crate::methods::{
    LiveItem, LiveMode, LivePublish, LivePublishTrack, LiveSend, LiveSendObject, LiveSubscribe,
    LiveSubscribeTrack, LiveTrack, Publishing,
};
use crate::{Context, Host, Task};

/// The priority a track is published at unless it says otherwise
/// ([`LivePublish::priority`]): the middle, as MoQ's default.
pub const DEFAULT_PRIORITY: u8 = 128;

/// A track this view publishes. The `Task` inside is the publication: drop
/// the `Track` to unpublish.
pub struct Track<M: BorshSerialize> {
    task: Task<()>,
    track: LiveTrack,
    group: u64,
    object: u32,
    mode: LiveMode,
    _m: PhantomData<M>,
}

impl<M: BorshSerialize> Track<M> {
    /// `live.publish` of `request`, with `on` hearing [`Publishing`]
    /// (`Waiting` while the room's program has not granted the track, then
    /// the subscriber count, and in stream mode `Cut { group }` when the
    /// host abandoned a group it could not queue: call
    /// [`next_group`](Track::next_group) and send a full state), a refused
    /// object (an `Err` that does not end the track) and the refusal that
    /// ends it. `priority` 0 is the
    /// highest: the node sends a subscriber this track's objects ahead of
    /// those of its tracks at a larger number ([`DEFAULT_PRIORITY`] unless
    /// the track says otherwise).
    pub fn publish<V: 'static>(
        cx: &mut Context<V>,
        request: LivePublish,
        on: impl FnMut(&mut V, Result<Publishing, Error>, &mut Context<V>) + 'static,
    ) -> Track<M> {
        let track = request.track.clone();
        let mode = request.mode;
        Track {
            task: cx.follow::<LivePublishTrack>(request, on),
            track,
            group: 0,
            object: 0,
            mode,
            _m: PhantomData,
        }
    }

    /// Datagram mode: one object, the newest wins. The group is this
    /// call's `group`; objects count up from 0 within it.
    pub fn send_latest(&mut self, host: &Host, group: u64, value: &M) {
        debug_assert_eq!(
            self.mode,
            LiveMode::Datagram,
            "send_latest is datagram mode's"
        );
        if group != self.group {
            self.group = group;
            self.object = 0;
        }
        self.put(
            host,
            borsh::to_vec(value).expect("borsh encodes an in-memory value"),
            false,
        );
    }

    /// Stream mode: one object appended to the current group.
    pub fn send(&mut self, host: &Host, value: &M) {
        debug_assert_eq!(self.mode, LiveMode::Stream, "send is stream mode's");
        self.put(
            host,
            borsh::to_vec(value).expect("borsh encodes an in-memory value"),
            false,
        );
    }

    /// Stream mode: close the current group and start the next (a
    /// keyframe, a full state, a message boundary). Subscribers see the
    /// closed group's `GroupEnd { complete: true }` now; a group with no
    /// object sent is not closed, only passed over.
    pub fn next_group(&mut self, host: &Host) {
        if self.object > 0 {
            self.put(host, Vec::new(), true);
        }
        self.group += 1;
        self.object = 0;
    }

    /// The group the next object goes in.
    pub fn group(&self) -> u64 {
        self.group
    }

    fn put(&mut self, host: &Host, body: Vec<u8>, end: bool) {
        host.notify::<LiveSendObject>(LiveSend {
            track: self.track.clone(),
            group: self.group,
            object: self.object,
            body,
            end,
        });
        if !end {
            self.object += 1;
        }
    }
}

impl<M: BorshSerialize> std::fmt::Debug for Track<M> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Track")
            .field("track", &self.track)
            .field("mode", &self.mode)
            .field("group", &self.group)
            .field("publication", &self.task)
            .finish()
    }
}

/// One item of a [`subscribe`]d track, its body decoded.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Received<M> {
    /// The room's program has not granted the track yet: the subscription
    /// waits, and `Opened` follows on the block that grants it.
    Waiting,
    /// The track as the node has it: who publishes it, how, and the group
    /// the node replays first (`None`: nothing is replayed, the next object
    /// is new). Comes again when the host opened the track anew: what was
    /// built from the items before it starts over.
    Opened {
        by: u64,
        mode: LiveMode,
        latest: Option<u64>,
    },
    Object {
        group: u64,
        object: u32,
        value: M,
    },
    /// A stream-mode group closed (`complete`) or was cut short.
    GroupEnd {
        group: u64,
        complete: bool,
    },
}

/// `live.subscribe` of `request`, each body decoded as `M`. A body that
/// does not decode reaches `each` as `Err` (`unexpected_reply`) and the
/// subscription goes on; a refusal the host sends last (`publisher_gone`,
/// `track_ended`, `unknown_track`, `not_granted`, `malformed_request`,
/// `bad_proof`, `live_unavailable`) ends it.
pub fn subscribe<V: 'static, M: BorshDeserialize + 'static>(
    cx: &mut Context<V>,
    request: LiveSubscribe,
    mut each: impl FnMut(&mut V, Result<Received<M>, Error>, &mut Context<V>) + 'static,
) -> Task<()> {
    cx.follow::<LiveSubscribeTrack>(request, move |view, item, cx| {
        each(view, item.and_then(received), cx)
    })
}

fn received<M: BorshDeserialize>(item: LiveItem) -> Result<Received<M>, Error> {
    Ok(match item {
        LiveItem::Waiting => Received::Waiting,
        LiveItem::Opened { by, mode, latest } => Received::Opened { by, mode, latest },
        LiveItem::Object {
            group,
            object,
            body,
        } => Received::Object {
            group,
            object,
            value: borsh::from_slice(&body).map_err(|error| malformed(error.to_string()))?,
        },
        LiveItem::GroupEnd { group, complete } => Received::GroupEnd { group, complete },
    })
}
