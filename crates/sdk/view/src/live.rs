//! Live tracks: what a view publishes to, and hears from, the people in its
//! program's rooms, through the connected node and off consensus. Four
//! methods carry it ([`LivePublishTrack`], [`LiveSendObject`],
//! [`LiveSubscribeTrack`], [`LiveTracks`]); this module is the two helpers
//! that earn a place over them: [`Track`], which owns a publication and
//! numbers its groups and objects, and [`subscribe`], which decodes what a
//! track carries. Who is in a room is `cx.follow::<LiveTracks>(room, ..)`
//! as it is.
//!
//! The node lets an account publish or subscribe in a room only as the
//! view's program granted it, in rows the program writes
//! (`program::live::grant`). A view with no program behind it has no one to
//! write them, and opens no room.
//!
//! The node reads those rows from blocks, not from the preconfirmed state a
//! `Submit<P>` answers with: a row an op writes counts from the block that
//! carries it, and what the node refused before that block stays refused.
//! A view that sends the op opening a room asks again on its program's
//! `Changes<P>` item whose keys hold the row's (`abi::live::key`), never on
//! the op's answer: the node publishes that item once its state holds the
//! row.
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

/// The priority a [`Track`] publishes at: the middle, as MoQ's default.
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
    /// `live.publish` at [`DEFAULT_PRIORITY`], with `on` hearing the
    /// subscriber count, a refused object (an `Err` that does not end the
    /// track) and the refusal that ends it.
    pub fn publish<V: 'static>(
        cx: &mut Context<V>,
        track: LiveTrack,
        mode: LiveMode,
        on: impl FnMut(&mut V, Result<Publishing, Error>, &mut Context<V>) + 'static,
    ) -> Track<M> {
        let request = LivePublish {
            track: track.clone(),
            mode,
            priority: DEFAULT_PRIORITY,
        };
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
    pub fn send_latest(&mut self, cx: &Host, group: u64, value: &M) {
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
            cx,
            borsh::to_vec(value).expect("borsh encodes an in-memory value"),
            false,
        );
    }

    /// Stream mode: one object appended to the current group.
    pub fn send(&mut self, cx: &Host, value: &M) {
        debug_assert_eq!(self.mode, LiveMode::Stream, "send is stream mode's");
        self.put(
            cx,
            borsh::to_vec(value).expect("borsh encodes an in-memory value"),
            false,
        );
    }

    /// Stream mode: close the current group and start the next (a
    /// keyframe, a full state, a message boundary). Subscribers see the
    /// closed group's `GroupEnd { complete: true }` now; a group with no
    /// object sent is not closed, only passed over.
    pub fn next_group(&mut self, cx: &Host) {
        if self.object > 0 {
            self.put(cx, Vec::new(), true);
        }
        self.group += 1;
        self.object = 0;
    }

    /// The group the next object goes in.
    pub fn group(&self) -> u64 {
        self.group
    }

    fn put(&mut self, cx: &Host, body: Vec<u8>, end: bool) {
        cx.notify::<LiveSendObject>(LiveSend {
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
    /// The track as the node has it, who publishes it and how. Comes again
    /// when the host opened the track anew: what was built from the items
    /// before it starts over.
    Opened {
        by: u64,
        mode: LiveMode,
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

/// `live.subscribe` with each body decoded as `M`: `latest` replays the
/// group the node holds first. A body that does not decode reaches `each`
/// as `Err` (`unexpected_reply`) and the subscription goes on; a refusal
/// the host sends last (`publisher_gone`, `track_ended`, `unknown_track`,
/// `not_granted`) ends it.
pub fn subscribe<V: 'static, M: BorshDeserialize + 'static>(
    cx: &mut Context<V>,
    track: LiveTrack,
    latest: bool,
    mut each: impl FnMut(&mut V, Result<Received<M>, Error>, &mut Context<V>) + 'static,
) -> Task<()> {
    cx.follow::<LiveSubscribeTrack>(LiveSubscribe { track, latest }, move |view, item, cx| {
        each(view, item.and_then(received), cx)
    })
}

fn received<M: BorshDeserialize>(item: LiveItem) -> Result<Received<M>, Error> {
    Ok(match item {
        LiveItem::Opened { by, mode, .. } => Received::Opened { by, mode },
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
