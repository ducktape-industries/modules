//! What this view says to the host: the chat program by the methods in
//! `view_wire::methods`, and the host's session facts.
pub use ducktape_view_guest::methods::{
    Changes, ClipboardRead, ClipboardWrite, HostId, HostOffset, HostRoute, HostSession,
    HostVisible, Query as Ask, Session, Submit,
};
