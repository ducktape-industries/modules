//! What Account follows while it is open: the session, and the live heads
//! of identity and valset. Every follower says what a refusal means to it;
//! none ends on one.
use ducktape_view_guest::Context;
use ducktape_view_guest::Loadable;
use ducktape_view_guest::methods::Changes;

use crate::Settings;
use crate::api::{HostSession, IdentityApi, ValsetApi};

impl Settings {
    /// Subscribes every follower; the ones before are dropped with them.
    pub(crate) fn watch(&mut self, cx: &mut Context<Self>) {
        let host = cx.host();
        let session = host.subscribe::<HostSession>(());
        let identity = host.subscribe::<Changes<IdentityApi>>(());
        let valset = host.subscribe::<Changes<ValsetApi>>(());
        self.followers = vec![
            cx.for_each(session, |view, session, _, cx| match session {
                Ok(session) => view.session_changed(session, cx),
                Err(refusal) => view.account = Loadable::Failed(refusal),
            }),
            // an account, key or agent changed: the reader's may be among them
            cx.for_each(identity, |view, head, _, cx| match head {
                Ok(_) => view.refresh_account(cx),
                Err(refusal) => {
                    cx.host()
                        .log_refused("settings", "identity's live heads", &refusal)
                }
            }),
            // a key started or stopped validating
            cx.for_each(valset, |view, head, _, cx| match head {
                Ok(_) => view.refresh_account(cx),
                Err(refusal) => cx
                    .host()
                    .log_refused("settings", "valset's live heads", &refusal),
            }),
        ];
    }
}
