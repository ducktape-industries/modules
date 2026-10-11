//! What chat follows while it is open: the session, routes opened into
//! it, whether it is on screen, and the live heads of chat and identity.
//! Every follower says what a refusal means to it; none ends on one.
use ducktape_view_guest::prelude::*;

use crate::{Chat, links};
use program::role::Identity;

impl Chat {
    /// Follows each of them for as long as the view runs.
    pub(crate) fn watch(&mut self, cx: &mut Context<Self>) {
        cx.follow::<HostSession>((), |chat, props, cx| match props {
            Ok(next) => chat.session_changed(next, cx),
            Err(refusal) => {
                chat.notice = format!("Couldn’t read the session: {}", refusal.message);
                cx.notify();
            }
        })
        .detach();
        // a block re-reads what it wrote to, as the program declares it
        cx.follow::<Changes<::chat::Chat>>((), |chat, change, cx| match change {
            Ok(change) => chat.changed(change.as_ref(), cx),
            Err(refusal) => cx.log_refused("chat's live heads", &refusal),
        })
        .detach();
        // `duck://<chain>/chat/<channel>[/<seq>]`: a link opened into
        // this view (a notice's, say) names the room and the message
        cx.follow::<HostRoute>((), |chat, route, cx| match route {
            Ok(route) => {
                if let Some((channel, seq)) = links::route_target(&route) {
                    chat.search_clear();
                    chat.open_at(channel, seq, cx);
                    chat.settle_badge(cx);
                    cx.notify();
                }
            }
            Err(refusal) => cx.log_refused("the route", &refusal),
        })
        .detach();
        cx.follow::<HostVisible>((), |chat, visible, cx| match visible {
            Ok(visible) => chat.visibility_changed(visible, cx),
            Err(refusal) => cx.log_refused("visibility", &refusal),
        })
        .detach();
        // re-read the roster on identity's heads, so a name another
        // signer claims replaces its "account N" fallback
        cx.follow::<Changes<Identity>>((), |chat, head, cx| match head {
            Ok(_) => chat.load_names(cx),
            Err(refusal) => cx.log_refused("identity's live heads", &refusal),
        })
        .detach();
    }
}
