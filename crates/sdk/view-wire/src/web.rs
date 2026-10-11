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
/// whenever any of it changes. The page writes the address and the title
/// (`history.pushState`, `document.title`), so the host bounds both.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct WebState {
    /// The address the page is at now: a link followed in the page or a
    /// redirect moves it. At most [`MAX_URL_BYTES`]; a longer one arrives
    /// empty, since an address cut short is another address.
    pub url: String,
    /// One line, cleaned as a `host.title` is: no control, bidi embedding,
    /// override or isolate, line or paragraph separator, no space at either
    /// end, at most 80 chars.
    pub title: String,
    pub loading: bool,
    pub can_go_back: bool,
    pub can_go_forward: bool,
}

/// Whether `frame` carries a web piece: in its tree, in a patch or in a
/// tooltip's content. A host refuses such a frame from a view whose
/// manifest does not declare [`Capability::Web`](crate::methods::Capability::Web).
pub fn carries_web(frame: &crate::Frame) -> bool {
    fn has_web(node: &crate::Node) -> bool {
        matches!(node, crate::Node::Web { .. }) || node.children().iter().any(has_web)
    }
    let patched = frame.patches.iter().filter_map(|patch| match patch {
        crate::Patch::Replace { node, .. }
        | crate::Patch::Props { node, .. }
        | crate::Patch::Insert { node, .. } => Some(node),
        crate::Patch::Remove { .. } | crate::Patch::Move { .. } => None,
    });
    let tooltips = frame
        .tooltip_responses
        .iter()
        .filter_map(|response| response.content.as_deref());
    frame
        .root
        .iter()
        .chain(patched)
        .chain(tooltips)
        .any(has_web)
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

/// A web piece's address and commands inside the frame's bounds: at most
/// [`MAX_WEB_COMMANDS`], and an address past [`MAX_URL_BYTES`] emptied,
/// since one cut short is another address (the host loads nothing for an
/// empty one).
pub(crate) fn sanitize(
    url: &mut String,
    commands: &mut Vec<WebCommand>,
    budgets: &mut crate::Budgets,
) {
    commands.truncate(MAX_WEB_COMMANDS);
    let loads = commands.iter_mut().filter_map(|command| match command {
        WebCommand::Load(url) => Some(url),
        _ => None,
    });
    for url in std::iter::once(url).chain(loads) {
        if url.len() > MAX_URL_BYTES {
            url.clear();
            budgets.cut(|cuts| &mut cuts.strings, 1);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{ElementIdWire, Node, StyleId};

    /// An address past the bound is emptied, never cut short, and a frame
    /// carries at most `MAX_WEB_COMMANDS` commands.
    #[test]
    fn a_long_address_is_none_and_commands_are_bounded() {
        let long = format!("https://example.com/{}", "a".repeat(MAX_URL_BYTES));
        let mut node = Node::Web {
            id: ElementIdWire::Name("page".into()),
            url: long.clone(),
            style: StyleId(0),
            revision: 1,
            commands: std::iter::repeat_n(WebCommand::Load(long), MAX_WEB_COMMANDS + 1).collect(),
            on_state: None,
        };
        let report = crate::sanitize_tree(&mut node, &crate::styles::testing::held()).unwrap();
        let Node::Web { url, commands, .. } = node else {
            unreachable!()
        };
        assert!(url.is_empty());
        assert_eq!(commands.len(), MAX_WEB_COMMANDS);
        assert!(
            commands
                .iter()
                .all(|c| *c == WebCommand::Load(String::new()))
        );
        assert_eq!(report.strings, MAX_WEB_COMMANDS + 1);
    }
}
