//! The pane's side of chat's notices: it posts none (chat's notifier,
//! `notifier.rs`, does, whatever panes are open), but reading a room takes
//! that room's rows in the host's centre down, and the room it shows is the
//! one it names to the host (`host.showing`), which keeps the notifier quiet
//! about it.
use chat::ChannelInfo;
use view::prelude::*;

use crate::Chat;
use crate::notifier::tag;
use chat::view::Names;

impl Chat {
    /// The channel list landed: the rooms, and the cursors kept.
    pub(crate) fn channels_landed(&mut self, channels: Vec<ChannelInfo>, cx: &mut Context<Self>) {
        self.channels_arrived(channels, cx);
        self.save_reads(cx);
    }

    /// The room on screen, if the reader can see it.
    pub(crate) fn viewing(&self) -> Option<String> {
        self.room
            .as_ref()
            .filter(|_| self.reads.visible)
            .map(|room| room.id.clone())
    }

    /// `room` is read: the host's rows for it, under the tag its notices
    /// carry, are read too.
    pub(crate) fn read_notices(&self, room: &str, cx: &mut Context<Self>) {
        let (Some(me), Some(info)) = (self.my_account(), self.info(room)) else {
            return;
        };
        let empty = Names::default();
        let names = self.names.ready().unwrap_or(&empty);
        cx.host()
            .notify::<NotifySeen>(tag(me, room, &info.channel.name, names));
    }

    /// Names the open room to the host when it changed; the host knows
    /// whether the pane is on screen.
    pub(crate) fn settle_showing(&mut self, cx: &mut Context<Self>) {
        let showing = self.room_id();
        if self.showing.as_ref() != Some(&showing) {
            self.showing = Some(showing.clone());
            cx.host().notify::<HostShowing>(showing);
        }
    }
}
