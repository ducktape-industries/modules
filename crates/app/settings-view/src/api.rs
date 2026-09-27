//! The programs this view reads, by the methods in `view_wire::methods`.

pub use ducktape_view_guest::methods::{
    CreateInvite, HostSession, Invite, InviteCreate, Session, Submit,
};

pub use identity::view::Identity;
pub use valset::view::Valset;
