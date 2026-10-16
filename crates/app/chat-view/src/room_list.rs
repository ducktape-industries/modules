//! The rooms the reader can open, in the order both designs list them:
//! the channels, then the direct rooms already open, then everyone else
//! the reader could write to, by name (their room opens on the first
//! message, [`Chat::dm_to_open`]). A program's own rooms (`forge:…` review
//! threads) are its to show.
use chat::{ChannelInfo, Principal};

use crate::Chat;
use crate::names::dm_peer_of;

/// A direct room in the list: the room, once it is open; the other
/// person; the room's id.
pub(crate) struct DmEntry<'a> {
    pub info: Option<&'a ChannelInfo>,
    pub peer: u64,
    pub id: String,
}

pub(crate) struct RoomList<'a> {
    pub channels: Vec<&'a ChannelInfo>,
    pub dms: Vec<DmEntry<'a>>,
}

impl RoomList<'_> {
    /// Every room's id, in list order: what the arrows walk.
    pub(crate) fn ids(&self) -> Vec<String> {
        let channels = self.channels.iter().map(|info| info.channel.id.clone());
        channels
            .chain(self.dms.iter().map(|dm| dm.id.clone()))
            .collect()
    }

    /// The direct rooms already open (they lead the direct messages).
    pub(crate) fn opened(&self) -> usize {
        self.dms.iter().filter(|dm| dm.info.is_some()).count()
    }
}

impl Chat {
    pub(crate) fn room_list(&self) -> RoomList<'_> {
        let mine = self.my_account();
        let mut channels = Vec::new();
        let mut dms = Vec::new();
        for info in self.channels.ready().into_iter().flatten() {
            let id = info.channel.id.as_str();
            if chat::namespace::program(id).is_some() {
                continue;
            }
            if chat::dm_peers(id).is_none() {
                channels.push(info);
                continue;
            }
            dms.extend(
                mine.and_then(|mine| dm_peer_of(mine, id))
                    .map(|peer| DmEntry {
                        info: Some(info),
                        peer,
                        id: info.channel.id.clone(),
                    }),
            );
        }
        if let (Some(mine), Some(names)) = (mine, self.names.ready()) {
            let mut rest: Vec<(String, u64)> = names
                .people()
                .filter(|peer| *peer != mine && !dms.iter().any(|dm| dm.peer == *peer))
                .map(|peer| (names.member(&Principal::Account(peer)).to_lowercase(), peer))
                .collect();
            rest.sort();
            dms.extend(rest.into_iter().map(|(_, peer)| DmEntry {
                info: None,
                peer,
                id: chat::dm_channel_id(mine, peer),
            }));
        }
        RoomList { channels, dms }
    }

    /// The row the room list's arrows are on: where they were left, else
    /// the open room, else the first.
    pub(crate) fn rooms_at(&self, ids: &[String]) -> usize {
        let open = self.room.as_ref().map(|room| room.id.as_str());
        self.rooms_cursor
            .as_deref()
            .or(open)
            .and_then(|id| ids.iter().position(|room| room == id))
            .unwrap_or(0)
    }
}
