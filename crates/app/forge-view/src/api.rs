//! The programs this view speaks to, by the methods in `view_wire::methods`:
//! forge (its own) and chat (the discussion threads).
use ducktape_view_guest::methods::{Query, Submit};

pub use ducktape_view_guest::methods::{HostSession, Session};

/// The forge and chat programs, by their own markers (named apart from
/// this view's `Forge` and the chat module's `Chat`).
pub use chat::view::ChatApi;
pub use forge::view::ForgeApi;
pub type Ask = Query<ForgeApi>;
pub type SubmitForge = Submit<ForgeApi>;
