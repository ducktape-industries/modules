//! What Explorer follows while it is open: the chain's heads, the session,
//! links opened into it, and the live heads of the programs whose lists it
//! shows. Every follower says what a refusal means to it; none ends on one.
use ducktape_view_guest::Context;
use ducktape_view_guest::design;
use ducktape_view_guest::methods::{ChainHeads, Changes, HostOffset, HostRoute, HostSession};
use identity::Identity;
use module_registry::Modules;
use valset::Valset;

use crate::Explorer;

impl Explorer {
    /// Subscribes every follower; the ones before are dropped with them.
    pub(crate) fn watch(&mut self, cx: &mut Context<Self>) {
        let host = cx.host();
        let heads = host.subscribe::<ChainHeads>(());
        let session = host.subscribe::<HostSession>(());
        let routes = host.subscribe::<HostRoute>(());
        let identity = host.subscribe::<Changes<Identity>>(());
        let valset = host.subscribe::<Changes<Valset>>(());
        let registry = host.subscribe::<Changes<Modules>>(());
        let offset = host.subscribe::<HostOffset>(());
        self.followers = vec![
            // A head is not drawn on its own: it is drawn with the page it
            // brings (`at_head`). The host keeps the stream across a
            // reconnect, opened again at the new node's tip.
            cx.for_each(heads, |view, head, _, cx| match head {
                Ok(head) => view.at_head(head, cx),
                Err(refusal) => cx
                    .host()
                    .log_refused("explorer", "the chain's heads", &refusal),
            }),
            cx.for_each(session, |view, session, _, cx| match session {
                Ok(session) => {
                    if view.session_chain != session.chain_id {
                        view.session_chain = session.chain_id;
                        cx.notify();
                    }
                }
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
                Ok(minutes) => {
                    design::set_utc_offset(minutes);
                    cx.notify();
                }
                Err(refusal) => cx
                    .host()
                    .log_refused("explorer", "the UTC offset", &refusal),
            }),
        ];
    }
}
