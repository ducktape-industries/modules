//! What chat follows while it is open: the session, routes opened into
//! it, whether it is on screen, and the live heads of chat and identity.
//! Every follower says what a refusal means to it; none ends on one.
use ducktape_view_guest::host::Error;
use ducktape_view_guest::{Context, Task};
use futures::{Stream, StreamExt};

use crate::api::{Changes, HostOffset, HostRoute, HostSession, HostVisible};
use crate::{Chat, links};
use ducktape_view_guest::design;
use program::role::Identity;

impl Chat {
    /// Subscribes every follower; the ones before are dropped with them.
    pub(crate) fn watch(&mut self, cx: &mut Context<Self>) {
        let host = cx.host();
        let props = host.subscribe::<HostSession>(());
        let changes = host.subscribe::<Changes<::chat::Chat>>(());
        let routes = host.subscribe::<HostRoute>(());
        let visible = host.subscribe::<HostVisible>(());
        let identity = host.subscribe::<Changes<Identity>>(());
        let offset = host.subscribe::<HostOffset>(());
        self.followers = vec![
            cx.for_each(props, |chat, props, _, cx| match props {
                Ok(next) => chat.session_changed(next, cx),
                Err(refusal) => {
                    chat.notice = format!("Couldn’t read the session: {}", refusal.message)
                }
            }),
            // a head only starts the re-reads; the room is drawn with what
            // they bring, not before
            Self::follow(changes, cx, |chat, head, cx| match head {
                Ok(_) => chat.refresh(cx),
                Err(refusal) => cx.host().log_refused("chat", "chat's live heads", &refusal),
            }),
            // `duck://<chain>/chat/<channel>[/<seq>]`: a link opened into
            // this view (a notice's, say) names the room and the message
            cx.for_each(routes, |chat, route, _, cx| match route {
                Ok(route) => {
                    if let Some((channel, seq)) = links::route_target(&route) {
                        chat.search_clear();
                        chat.open_at(channel, seq, cx);
                        chat.settle_badge(cx);
                    }
                }
                Err(refusal) => cx.host().log_refused("chat", "the route", &refusal),
            }),
            cx.for_each(visible, |chat, visible, _, cx| match visible {
                Ok(visible) => chat.visibility_changed(visible, cx),
                Err(refusal) => cx.host().log_refused("chat", "visibility", &refusal),
            }),
            // re-read the roster on identity's heads, so a name another
            // signer claims replaces its "account N" fallback
            Self::follow(identity, cx, |chat, head, cx| match head {
                Ok(_) => chat.load_names(cx),
                Err(refusal) => cx
                    .host()
                    .log_refused("chat", "identity's live heads", &refusal),
            }),
            // the reader's zone, for the day and clock a message reads
            cx.for_each(offset, |_, offset, _, cx| match offset {
                Ok(minutes) => design::set_utc_offset(minutes),
                Err(refusal) => cx.host().log_refused("chat", "the UTC offset", &refusal),
            }),
        ];
    }

    /// `cx.for_each` without the redraw after every item, for a follower
    /// whose `each` only starts reads: what it starts draws when it lands,
    /// so a head drawn here would be a full render of the room that
    /// changes nothing. `each` leaves the snapshot alone (the SDK's notify
    /// contract): what it writes is drawn by the landing.
    fn follow<T: 'static>(
        mut stream: impl Stream<Item = Result<T, Error>> + Unpin + 'static,
        cx: &mut Context<Self>,
        mut each: impl FnMut(&mut Self, Result<T, Error>, &mut Context<Self>) + 'static,
    ) -> Task<()> {
        cx.spawn(async move |this, cx| {
            while let Some(item) = stream.next().await {
                if this.update(cx, |chat, cx| each(chat, item, cx)).is_err() {
                    break;
                }
            }
        })
    }
}
