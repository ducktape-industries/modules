//! What this view says to the host: the chat program by the methods in
//! `view_wire::methods`, and the host's session facts.
pub use ducktape_view_guest::methods::{
    Changes, ClipboardWrite, HostId, HostRoute, HostSession, HostVisible, Session, Submit,
};
