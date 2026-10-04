use crate::*;

/// The most bytes one encoded frame may be: a host refuses a longer one
/// before decoding it, and no raster payload may claim more.
pub const MAX_FRAME_BYTES: usize = 8 << 20;
/// A tree deeper than this is cut off: a guest cannot make the host's
/// layout recurse without bound.
pub const MAX_DEPTH: usize = 64;
/// More nodes than this and the host stops reading: the widget tree of a
/// screen, not of a spreadsheet.
pub const MAX_NODES: usize = 8_192;
/// The longest string a single node may carry (text, placeholder, key).
pub const MAX_STRING_BYTES: usize = 64 << 10;
/// The most SHAPED text one frame may carry in total — every [`Node::Text`]
/// content, field placeholder and label, and plain button label together.
/// A field's `value` is the host engine's copy to adopt, bounded by
/// [`MAX_FIELD_BYTES`](crate::MAX_FIELD_BYTES) alone, and never shaped from
/// the frame.
///
/// The per-string and per-node caps do not bound this: 128 strings of
/// [`MAX_STRING_BYTES`] are a legal 8 MiB frame, and the host reshapes all
/// of it on the window thread every time the guest changes a character.
/// Measured on a desk machine, that frame took 6.2 s and allocated 3.3 GiB;
/// held to one full-length string's worth of text it takes 69 ms and 26 MiB.
/// A screen shows a couple of kilobytes, so this leaves a guest that means
/// well thirty times what it needs while taking two orders of magnitude off
/// what a hostile one can spend.
///
/// It bounds bytes and nothing else. Two other costs live outside it and are
/// the host's to attack: [`MAX_NODES`] nodes cost around a hundred
/// milliseconds to lay out however short their text, and a byte of Hangul,
/// Han or emoji costs some twenty times a byte of ASCII to shape.
pub const MAX_TEXT_BYTES_PER_FRAME: usize = MAX_STRING_BYTES;
/// The most picture bytes one frame may carry in total, over every
/// [`Node::Svg`] or [`Node::Image`] that brings its payload. A picture that does not fit in
/// what is left is dropped whole, not cut: half an SVG is not an SVG, and
/// the host draws an unknown hash as empty space. A guest sends each
/// picture once, so this bounds what a frame can make the host parse, not
/// what an app can show over its life; an icon is a few kilobytes.
pub const MAX_PICTURE_BYTES_PER_FRAME: usize = 1 << 20;
/// A uniform list may describe a large logical list without allocating rows.
pub const MAX_UNIFORM_LIST_COUNT: usize = 65_536;
/// A frame and one host range request carry at most this many uniform rows.
pub const MAX_UNIFORM_LIST_ROWS: usize = 256;

/// Text and spacing sizes are pixels; nothing on a screen needs more.
pub const MAX_PIXELS: f32 = 8192.0;
/// A text size, which is not a length: every glyph at it is rasterized and
/// cached, so a screenful of 8192 px text is an atlas no screen asked for.
pub const MAX_TEXT_PIXELS: f32 = 512.0;

/// Pulls a frame from an untrusted module into what the host is willing to
/// lay out: the tree is truncated past [`MAX_DEPTH`] and [`MAX_NODES`],
/// strings past [`MAX_STRING_BYTES`], shaped text past
/// [`MAX_TEXT_BYTES_PER_FRAME`] in total, picture bytes past
/// [`MAX_PICTURE_BYTES_PER_FRAME`] in total, text sizes to [`MAX_TEXT_PIXELS`],
/// every other size, colour and spacing clamped to a finite range, and typed
/// identity collisions refused ([`crate::identity`]). A frame from a
/// well-behaved guest passes through unchanged. It answers what it cut and
/// where ([`SanitizeReport`]); a clamp is no cut.
///
/// A frame that arrived as bytes has passed [`decode`] first, which refuses
/// one nested deeper than this walk goes. A frame that carries `patches`
/// instead of a tree is bounded by [`apply`], since every bound is on the
/// tree the patches make and only the host holds it.
pub fn sanitize(frame: &mut Frame) -> Result<SanitizeReport, Refused> {
    let mut budgets = Budgets::frame();
    if let Some(root) = &mut frame.root {
        sanitize_tree_with(root, &mut budgets)?;
    }
    frame.tooltip_responses.truncate(MAX_PATCHES);
    let mut responses = Vec::with_capacity(frame.tooltip_responses.len());
    for mut response in frame.tooltip_responses.drain(..) {
        if let Some(content) = &mut response.content {
            if budgets.nodes == 0 {
                budgets.cut(|cuts| &mut cuts.nodes, content.count());
                continue;
            }
            sanitize_tree_with(content, &mut budgets)?;
        }
        responses.push(response);
    }
    frame.tooltip_responses = responses;
    for request in &mut frame.requests {
        truncate_string(&mut request.kind);
    }
    Ok(budgets.cuts)
}

pub(crate) fn sanitize_tree(root: &mut Node) -> Result<SanitizeReport, Refused> {
    let mut budgets = Budgets::frame();
    sanitize_tree_with(root, &mut budgets)?;
    Ok(budgets.cuts)
}

fn sanitize_tree_with(root: &mut Node, budgets: &mut Budgets) -> Result<(), Refused> {
    sanitize_node(root, 0, budgets, &mut identity::Scopes::default(), None)
}

/// Why the host refuses a frame or a patch: a bound or a shape it breaks,
/// or one typed id claimed twice, with the site that claimed it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Refused {
    Invalid(&'static str),
    Duplicate(identity::DuplicateIdentity),
}

impl From<&'static str> for Refused {
    fn from(reason: &'static str) -> Self {
        Self::Invalid(reason)
    }
}

impl From<identity::DuplicateIdentity> for Refused {
    fn from(duplicate: identity::DuplicateIdentity) -> Self {
        Self::Duplicate(duplicate)
    }
}

impl std::fmt::Display for Refused {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Invalid(reason) => f.write_str(reason),
            Self::Duplicate(duplicate) => duplicate.fmt(f),
        }
    }
}

impl std::error::Error for Refused {}

/// What is left of a frame's per-frame budgets while its tree is walked.
pub(crate) struct Budgets {
    pub(crate) nodes: usize,
    pub(crate) canvas_parts: usize,
    pub(crate) text: usize,
    pub(crate) pictures: usize,
    pub(crate) list_items: usize,
    /// A node already claimed the active descendant.
    pub(crate) active_descendant: bool,
    /// The child indices from the root to the node being walked.
    pub(crate) at: Vec<u32>,
    /// What the walk has cut so far.
    pub(crate) cuts: SanitizeReport,
}

impl Budgets {
    pub(crate) fn frame() -> Self {
        Self {
            nodes: MAX_NODES,
            text: MAX_TEXT_BYTES_PER_FRAME,
            pictures: MAX_PICTURE_BYTES_PER_FRAME,
            list_items: MAX_LIST_ITEMS,
            canvas_parts: MAX_CANVAS_PARTS,
            active_descendant: false,
            at: Vec::new(),
            cuts: SanitizeReport::default(),
        }
    }

    /// Counts `count` cuts of one kind, and where the first one fell.
    pub(crate) fn cut(&mut self, kind: fn(&mut SanitizeReport) -> &mut usize, count: usize) {
        if count == 0 {
            return;
        }
        *kind(&mut self.cuts) += count;
        if self.cuts.first.is_none() {
            self.cuts.first = Some(self.at.clone());
        }
    }
}

/// Truncates one shaped string to what is left of the frame's text budget
/// and spends what survives. Nodes are walked in tree order, so a frame past
/// the budget keeps its head and loses its tail.
pub(crate) fn spend_text(text: &mut String, budgets: &mut Budgets) {
    let before = text.len();
    truncate_to(text, budgets.text.min(MAX_STRING_BYTES));
    if text.len() < before {
        match before > MAX_STRING_BYTES {
            true => budgets.cut(|cuts| &mut cuts.strings, 1),
            false => budgets.cut(|cuts| &mut cuts.text, 1),
        }
    }
    budgets.text -= text.len();
}

/// Cuts a node's string at [`MAX_STRING_BYTES`], and says so.
pub(crate) fn cut_string(text: &mut String, budgets: &mut Budgets) {
    let before = text.len();
    truncate_string(text);
    budgets.cut(|cuts| &mut cuts.strings, usize::from(text.len() < before));
}

/// Spends a picture's bytes from the frame's picture budget, or drops them
/// whole when they do not fit.
fn spend_svg(bytes: &mut Option<Vec<u8>>, budgets: &mut Budgets) {
    match bytes {
        Some(picture) if picture.len() <= budgets.pictures => budgets.pictures -= picture.len(),
        Some(_) => {
            *bytes = None;
            budgets.cut(|cuts| &mut cuts.pictures, 1);
        }
        None => {}
    }
}

mod node;
use node::sanitize_node;

mod numbers;
pub use numbers::truncate_string;
pub(crate) use numbers::{bounded, finite, signed_bounded, truncate_to};

mod interactivity;
use interactivity::sanitize_interactivity;
