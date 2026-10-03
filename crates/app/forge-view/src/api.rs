//! The programs this view speaks to, by the methods in `view_wire::methods`:
//! forge (its own) and chat (the discussion threads).
use ducktape_view_guest::methods::{Query, Submit};

pub use ducktape_view_guest::methods::{HostSession, Session};

/// The forge program's two methods (`forge::Forge` the program, apart from
/// this view's `Forge`). Chat is read by its typed asks (`chat::ask`).
pub type Ask = Query<forge::Forge>;
pub type SubmitForge = Submit<forge::Forge>;
