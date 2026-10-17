//! Markdown as a document, read once for either design (a README, a
//! markdown file, a change's body); each design draws the blocks its own
//! way (`ui::markdown`, `ui::phone::document`).
//!
//! - [`parse`] turns text into [`Block`]s, once per blob (`BlobCache`).
//! - A pressed link calls the caller's [`OnLink`] with its raw destination;
//!   [`links`] is forge's, which [`target`] resolves: the web through the
//!   host, or a file of the repository, relative to the document's folder,
//!   its `%XX` escapes read by ducklink.
mod links;
mod parse;

use std::rc::Rc;

use view::prelude::*;

use crate::Forge;
pub(crate) use links::{Target, target};
pub(crate) use parse::{Block, Text, parse};

/// What a pressed link does, given its raw destination.
pub(crate) type OnLink = Rc<dyn Fn(&String, &mut Window, &mut App)>;

/// A document's links, a relative one resolved against `dir`, its folder
/// (the root for a change's body).
pub(crate) fn links(dir: Vec<u8>, cx: &mut Context<Forge>) -> OnLink {
    Rc::new(cx.listener(move |forge, dest: &String, _, cx| forge.follow_link(&dir, dest, cx)))
}
