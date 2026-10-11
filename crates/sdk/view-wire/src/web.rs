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
    /// Open this address. The host loads only an `https://` one (and
    /// `http://` on `127.0.0.1` or `localhost`); [`is_loadable`].
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

/// Whether a web piece may load `url`: `https://` anywhere, `http://` only
/// on this machine (`127.0.0.1`, `localhost`), at most [`MAX_URL_BYTES`].
/// The one rule a host holds every navigation to, the page's own included.
pub fn is_loadable(url: &str) -> bool {
    if url.len() > MAX_URL_BYTES || url.contains(char::is_control) {
        return false;
    }
    if let Some(rest) = url.strip_prefix("https://") {
        return !rest.is_empty();
    }
    let Some(rest) = url.strip_prefix("http://") else {
        return false;
    };
    let host = rest.split(['/', '?', '#']).next().unwrap_or_default();
    let host = host.rsplit_once(':').map_or(host, |(name, _port)| name);
    matches!(host, "127.0.0.1" | "localhost")
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_https_and_this_machines_http_load() {
        for url in [
            "https://example.com",
            "https://ko.wikipedia.org/wiki/한글",
            "http://127.0.0.1:8765/page.html",
            "http://localhost/",
            "http://localhost",
        ] {
            assert!(is_loadable(url), "{url}");
        }
        for url in [
            "",
            "https://",
            "http://example.com",
            "http://127.0.0.1.evil.com/",
            "http://localhost.evil.com",
            "file:///etc/passwd",
            "javascript:alert(1)",
            "data:text/html,hi",
            "chrome://settings",
            "https://example.com/\n",
        ] {
            assert!(!is_loadable(url), "{url}");
        }
        assert!(is_loadable(&format!(
            "https://{}",
            "a".repeat(MAX_URL_BYTES - 8)
        )));
        assert!(!is_loadable(&format!(
            "https://{}",
            "a".repeat(MAX_URL_BYTES - 7)
        )));
    }
}
