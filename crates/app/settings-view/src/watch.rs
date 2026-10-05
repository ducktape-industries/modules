//! What Account follows while it is open: the session, and the live heads
//! of identity and valset. Every follower says what a refusal means to it;
//! none ends on one.
use ducktape_view_guest::Context;
use ducktape_view_guest::Loadable;
use ducktape_view_guest::methods::Changes;

use crate::Settings;
use crate::api::HostSession;
use identity::Identity;
use valset::Valset;

impl Settings {
    /// Follows each of them for as long as the view runs.
    pub(crate) fn watch(&mut self, cx: &mut Context<Self>) {
        cx.follow::<HostSession>((), |view, session, cx| match session {
            Ok(session) => view.session_changed(session, cx),
            Err(refusal) => {
                view.account = Loadable::Failed(refusal);
                cx.notify();
            }
        })
        .detach();
        // an account, key or agent changed: the reader's may be among them
        cx.follow::<Changes<Identity>>((), |view, head, cx| match head {
            Ok(_) => view.refresh_account(cx),
            Err(refusal) => cx.log_refused("identity's live heads", &refusal),
        })
        .detach();
        // a key started or stopped validating
        cx.follow::<Changes<Valset>>((), |view, head, cx| match head {
            Ok(_) => view.refresh_account(cx),
            Err(refusal) => cx.log_refused("valset's live heads", &refusal),
        })
        .detach();
    }
}
