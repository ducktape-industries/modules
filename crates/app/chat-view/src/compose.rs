//! The composer's events for one target, run through its draft: the
//! sends it makes, and which drafts are kept.
use crate::composer::{Draft, Event, MentionChoice, Outcome, Send, Target, pending_row};
use crate::names::mention_token;
use crate::{Chat, Mode};
use ducktape_view_guest::prelude::*;

impl Chat {
    /// Who the composer offers after `@`: the roster, and the room's members.
    pub(crate) fn mention_choices(&self) -> Vec<MentionChoice> {
        let Some(names) = self.names.ready() else {
            return Vec::new();
        };
        let members: Vec<_> = self
            .roster()
            .into_iter()
            .map(|(principal, _)| principal)
            .collect();
        crate::names::mention_choices(names, &members)
            .into_iter()
            .map(|choice| MentionChoice {
                token: mention_token(&choice.principal),
                label: choice.label,
            })
            .collect()
    }

    pub(crate) fn composer(
        &mut self,
        target: Target,
        event: Event,
        window: &mut ducktape_view_guest::Window,
        cx: &mut Context<Self>,
    ) {
        let choices = self.mention_choices();
        let key = target.key();
        // the draft the frame drew; none is an event that came the tick
        // after its composer left the screen
        let Some(draft) = self.drafts.get_mut(&key) else {
            return;
        };
        cx.notify();
        if let Outcome::Action(tag) = draft.handle(event, &key, &choices, window)
            && tag == "send"
            && let Some(send) = draft.submitted.take()
        {
            draft.in_flight.push(send.clone());
            self.send(key, send, target, cx);
        }
    }

    /// Makes the drafts agree with the screen, before every frame: each
    /// composer the frame draws has its stored draft, whatever wrote the
    /// room, the thread or the menu, so the draft a frame lowers is the one
    /// that hears what is typed into it. (An edit's is seeded with its
    /// message where its menu opens.) A draft whose composer is off screen
    /// stays while it holds something: the map is saved whole, and must
    /// not grow with every room opened or every edit begun.
    pub(crate) fn seat_drafts(&mut self) {
        let mut shown = Vec::new();
        if let Some(room) = &self.room {
            let post = |thread| {
                let channel = room.id.clone();
                Target::Post { channel, thread }.key()
            };
            shown.push(post(None));
            shown.extend(room.thread.as_ref().map(|thread| post(Some(thread.root))));
        }
        shown.extend(self.editing().map(|target| target.key()));
        self.drafts
            .retain(|key, draft| shown.contains(key) || !spent(key, draft));
        for key in shown {
            self.drafts.entry(key).or_default();
        }
    }

    /// The message the open menu edits, as its composer's target.
    pub(crate) fn editing(&self) -> Option<Target> {
        let menu = self.menu.as_ref().filter(|m| m.mode == Mode::Editing)?;
        Some(Target::Edit {
            channel: self.room_id(),
            seq: menu.seq,
            base_rev: menu.rev,
        })
    }

    fn send(&mut self, key: String, send: Send, target: Target, cx: &mut Context<Self>) {
        let me = self.me();
        let open_dm = self.dm_to_open(&target);
        cx.spawn(async move |this, cx| {
            let host = cx.host();
            let result = async {
                if let Some(open) = open_dm {
                    host.ask::<Submit<::chat::Chat>>(open).await?;
                }
                let id = host.ask::<HostId>("message".into()).await?;
                let op = crate::composer::op(id, &send, &target)?;
                let pending = me.and_then(|me| pending_row(&op, me));
                host.ask::<Submit<::chat::Chat>>(op).await.map(|_| pending)
            }
            .await;
            let _ = this.update(cx, |chat, cx| {
                cx.notify();
                // kept since the send, in flight; were it gone, a refused
                // send still needs a draft to wait in
                let draft = chat.drafts.entry(key).or_default();
                draft.complete_send(&send);
                match result {
                    Ok(pending) => {
                        draft.note.clear();
                        if let (Some(row), Some(room)) = (pending, chat.room.as_mut())
                            && room.id == target.channel()
                        {
                            room.pending.push(row);
                        }
                        if let Target::Edit { seq, .. } = target
                            && chat
                                .menu
                                .as_ref()
                                .is_some_and(|m| m.mode == Mode::Editing && m.seq == seq)
                        {
                            chat.menu = None;
                        }
                        chat.refresh(cx);
                    }
                    Err(refusal) => {
                        draft.note = refusal.message;
                        draft.failed(send);
                    }
                }
            });
        })
        .detach();
    }

    /// A post into a dm room the channel list doesn't hold yet: the op that
    /// opens it first. A dm id opens only through `CreateDmChannel`, named
    /// for the other person (a no-op once it is open).
    pub(crate) fn dm_to_open(&self, target: &Target) -> Option<chat::Op> {
        let Target::Post { channel, .. } = target else {
            return None;
        };
        if self.info(channel).is_some() {
            return None;
        }
        let peer = crate::names::dm_peer_of(self.my_account()?, channel)?;
        let name = self
            .names
            .ready()
            .and_then(|names| names.name(&chat::Principal::Account(peer)))
            .map(str::to_owned)
            .unwrap_or_else(|| format!("account {peer}"));
        Some(chat::Op::CreateDmChannel {
            counterpart: peer,
            name,
        })
    }
}

/// Nothing a reader would miss in the draft kept under `key`: it holds no
/// send, and shows what a new one would. For an edit that is whatever it
/// holds: opening the edit seeds it with the message again.
fn spent(key: &str, draft: &Draft) -> bool {
    let unwritten = draft.field.text().is_empty() && draft.note.is_empty();
    (unwritten || Target::edits(key))
        && draft.failed_send.is_none()
        && draft.submitted.is_none()
        && draft.in_flight.is_empty()
}
