//! A web page in a view ([`crate::Node::Web`]): what the view asks of it
//! ([`WebCommand`]) and what the host tells back ([`WebState`]). Nothing of
//! the page itself crosses: the host's browser engine draws it and the
//! reader's presses and keys on it are the page's.
use serde::{Deserialize, Serialize};

/// The longest address a web piece loads, in bytes. A longer one is no
/// address: the host loads nothing for it.
pub const MAX_URL_BYTES: usize = 2048;
/// The most commands one frame carries for one web piece.
pub const MAX_WEB_COMMANDS: usize = 16;

/// One navigation the view asks of its page, applied once by the host when
/// the node's `revision` moves.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum WebCommand {
    /// Open this address. The host loads only an `https://` one, or an
    /// `http://` one on this machine (`localhost`, a loopback address), with
    /// no user name or password in it: the host's own check, made before
    /// every load and every navigation it lets the page make.
    Load(String),
    Back,
    Forward,
    Reload,
    Stop,
}

/// The page as the host's browser holds it: what an address bar, a title
/// and the back, forward and stop buttons show. Sent as [`crate::Event::Web`]
/// whenever any of it changes.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct WebState {
    /// The address the page is at now: a link followed in the page or a
    /// redirect moves it.
    pub url: String,
    pub title: String,
    pub loading: bool,
    pub can_go_back: bool,
    pub can_go_forward: bool,
}

pub(crate) fn decode_commands<'de, D: serde::Deserializer<'de>>(
    d: D,
) -> Result<Vec<WebCommand>, D::Error> {
    crate::bounded_vec(
        d,
        MAX_WEB_COMMANDS,
        "more web commands than the host applies",
    )
}
