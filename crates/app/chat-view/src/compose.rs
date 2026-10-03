//! The composer's events for one target, run through its draft: sends,
//! edits, pastes and copies.
use ducktape_view_guest::Context;
use ducktape_view_guest::methods::Submit;

use crate::api::{ClipboardRead, ClipboardWrite, HostId};
use crate::composer::{Event, MentionChoice, Outcome, Send, Target, pending_row};
use crate::names::mention_token;
use crate::{Chat, Mode};

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
        event: Event<Self>,
        window: &mut ducktape_view_guest::Window,
        cx: &mut Context<Self>,
    ) {
        cx.notify();
        let choices = self.mention_choices();
        let key = target.key();
        let draft = self.drafts.entry(key.clone()).or_default();
        match draft.handle(event, &choices, cx) {
            Outcome::Updated => {}
            Outcome::Run(run) => run(self, window, cx),
            Outcome::Enqueue(tag) => crate::composer::act(window, &key, tag),
            Outcome::Action(tag) => self.composer_action(target, &key, &tag, cx),
        }
    }

    fn composer_action(&mut self, target: Target, key: &str, tag: &str, cx: &mut Context<Self>) {
        match tag {
            "send" => {
                let draft = self.drafts.entry(key.to_owned()).or_default();
                if let Some(send) = draft.submitted.take() {
                    draft.in_flight.push(send.clone());
                    self.send(key.to_owned(), send, target, cx);
                }
            }
            "restore" => {
                let choices = self.mention_choices();
                let draft = self.drafts.entry(key.to_owned()).or_default();
                if let Some(send) = draft.failed_send.take() {
                    draft.seed(&send.body, &choices);
                }
            }
            "paste" => {
                let key = key.to_owned();
                cx.spawn(async move |this, cx| {
                    let host = cx.host();
                    let result = host.ask::<ClipboardRead>(()).await;
                    let _ = this.update_in(cx, |chat, window, cx| {
                        cx.notify();
                        match result {
                            Ok(clipboard) => {
                                chat.drafts.entry(key.clone()).or_default().paste =
                                    Some(clipboard.text);
                                crate::composer::act(window, &key, "paste-ready");
                            }
                            Err(refusal) => {
                                chat.drafts.entry(key).or_default().note = refusal.message
                            }
                        }
                    });
                })
                .detach();
            }
            "copy" | "cut" => {
                let Some(text) = self
                    .drafts
                    .entry(key.to_owned())
                    .or_default()
                    .clipboard
                    .take()
                else {
                    return;
                };
                let key = key.to_owned();
                cx.spawn(async move |this, cx| {
                    let host = cx.host();
                    let result = host.ask::<ClipboardWrite>(text).await;
                    let _ = this.update(cx, |chat, cx| {
                        cx.notify();
                        if let Err(refusal) = result {
                            chat.drafts.entry(key).or_default().note = refusal.message;
                        }
                    });
                })
                .detach();
            }
            _ => {}
        }
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
