//! What Explorer follows while it is open: the chain's heads, the session,
//! links opened into it, and the live heads of the programs whose lists it
//! shows. Every follower says what a refusal means to it; none ends on one.
use ducktape_view_guest::Context;
use ducktape_view_guest::design;
use ducktape_view_guest::methods::{ChainHeads, Changes, HostOffset, HostRoute, HostSession};
use futures::StreamExt;
use identity::Identity;
use module_registry::Modules;
use valset::Valset;

use crate::Explorer;

impl Explorer {
    /// Subscribes every follower; the ones before are dropped with them.
    pub(crate) fn watch(&mut self, cx: &mut Context<Self>) {
        let host = cx.host();
        // `None` once the stream ends: the head is polled from then on
        let heads = host
            .subscribe::<ChainHeads>(())
            .map(Some)
            .chain(futures::stream::iter([None]));
        let session = host.subscribe::<HostSession>(());
        let routes = host.subscribe::<HostRoute>(());
        let identity = host.subscribe::<Changes<Identity>>(());
        let valset = host.subscribe::<Changes<Valset>>(());
        let registry = host.subscribe::<Changes<Modules>>(());
        let offset = host.subscribe::<HostOffset>(());
        // Not `cx.for_each`, which redraws the view after every item: a head
        // is drawn with the page it brings (`at_head`). A refused or ended
        // stream falls back to the clock, whose reads draw themselves.
        let mut heads = heads;
        let followed = cx.spawn(async move |this, cx| {
            while let Some(head) = heads.next().await {
                let landed = this.update(cx, |view, cx| match head {
                    Some(Ok(head)) => view.at_head(head, cx),
                    Some(Err(refusal)) => {
                        cx.host()
                            .log_refused("explorer", "the chain's heads", &refusal);
                        view.poll_head(cx);
                    }
                    None => view.poll_head(cx),
                });
                if landed.is_err() {
                    break;
                }
            }
        });
        self.followers = vec![
            followed,
            cx.for_each(session, |view, session, _, cx| match session {
                Ok(session) => view.session_chain = session.chain_id,
                Err(refusal) => cx.host().log_refused("explorer", "the session", &refusal),
            }),
            // `duck://<chain>/explorer/<route>`
            cx.for_each(routes, |view, route, _, cx| match route {
                Ok(route) => view.open_route(&route, cx),
                Err(refusal) => cx.host().log_refused("explorer", "the route", &refusal),
            }),
            // an account made, renamed or re-keyed, by anyone: a module's
            // `RegisterModule` included, which no transaction targets
            cx.for_each(identity, |view, head, _, cx| match head {
                Ok(_) => view.read_accounts(cx),
                Err(refusal) => {
                    cx.host()
                        .log_refused("explorer", "identity's live heads", &refusal)
                }
            }),
            cx.for_each(valset, |view, head, _, cx| match head {
                Ok(_) => view.read_validators(cx),
                Err(refusal) => cx
                    .host()
                    .log_refused("explorer", "valset's live heads", &refusal),
            }),
            cx.for_each(registry, |view, head, _, cx| match head {
                Ok(_) => view.read_network(cx),
                Err(refusal) => {
                    cx.host()
                        .log_refused("explorer", "the registry's live heads", &refusal)
                }
            }),
            // the reader's zone, for the dates a block and a tx read
            cx.for_each(offset, |_, offset, _, cx| match offset {
                Ok(minutes) => design::set_utc_offset(minutes),
                Err(refusal) => cx
                    .host()
                    .log_refused("explorer", "the UTC offset", &refusal),
            }),
        ];
    }
}
