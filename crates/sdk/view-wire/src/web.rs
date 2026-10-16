//! A web page in a view ([`crate::Node::Web`]): what the view asks of it
//! ([`WebCommand`]) and what the host tells back ([`WebState`]). Nothing of
//! the page itself crosses: the host's browser engine draws it and the
//! reader's presses and keys on it are the page's.
use serde::{Deserialize, Serialize};

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
/// whenever any of it changes. The page writes the address and the title
/// (`history.pushState`, `document.title`), so the host bounds both.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct WebState {
    pub url: String,
    /// One line, cleaned as a `host.title` is: no control, bidi embedding,
    /// override or isolate, line or paragraph separator, no space at either
    /// end, at most 80 chars.
    pub title: String,
    pub loading: bool,
    pub can_go_back: bool,
    pub can_go_forward: bool,
}
