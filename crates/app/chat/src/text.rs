//! What search and tags read out of a message: its flat text, the words
//! search indexes, and its `#tags`; and the same line as a person reads it.
use std::borrow::Cow;
use std::collections::BTreeSet;

use unicode_normalization::UnicodeNormalization;

use crate::view::Names;
use crate::{Block, MAX_TAG_CHARS, MAX_TAGS_PER_MESSAGE, Mark, Span};

/// The message as one line of text, blocks joined by a space. A block's
/// spans are runs of one text split where a mark starts or ends, so they
/// join as they are. A mention is its `<@7>` token: this is what search
/// indexes, not what a person reads ([`read_text`]).
pub fn plain_text(blocks: &[Block]) -> String {
    line(blocks, |span| Cow::Borrowed(&span.text))
}

/// [`plain_text`] as a person reads it: a mention spelled as `names` calls
/// the account (`@Ada`, or `@account-7` for one it does not name).
pub fn read_text(blocks: &[Block], names: &Names) -> String {
    line(blocks, |span| {
        let mention = span.marks.iter().find_map(|mark| match mark {
            Mark::Mention(principal) => Some(principal),
            _ => None,
        });
        match mention {
            Some(principal) => Cow::Owned(names.mention(principal)),
            None => Cow::Borrowed(&span.text),
        }
    })
}

/// The blocks on one line, each span as `span_text` spells it.
fn line<'a>(blocks: &'a [Block], span_text: impl Fn(&'a Span) -> Cow<'a, str>) -> String {
    let mut out = String::new();
    for block in blocks {
        let piece = match block {
            Block::Paragraph(spans) | Block::Quote(spans) => {
                spans.iter().map(&span_text).collect::<String>()
            }
            Block::Code { text, .. } => text.clone(),
            Block::Divider => continue,
        };
        if !out.is_empty() {
            out.push(' ');
        }
        out.push_str(&piece);
    }
    out
}

/// NFC lowercase alphanumeric runs of two or more chars.
pub fn tokens(text: &str) -> BTreeSet<String> {
    text.nfc()
        .collect::<String>()
        .to_lowercase()
        .split(|c: char| !c.is_alphanumeric())
        .filter(|t| t.chars().count() >= 2)
        .map(str::to_string)
        .collect()
}

/// `#tag` labels in appearance order, at most [`MAX_TAGS_PER_MESSAGE`]:
/// outside code and links, each one `tag_label`-normalized.
pub fn tags(blocks: &[Block]) -> Vec<String> {
    let spans = blocks.iter().flat_map(|block| match block {
        Block::Paragraph(spans) | Block::Quote(spans) => spans.as_slice(),
        Block::Code { .. } | Block::Divider => &[],
    });
    let mut out: Vec<String> = Vec::new();
    for span in spans.filter(|span| {
        !span
            .marks
            .iter()
            .any(|m| matches!(m, Mark::Link(_) | Mark::Code))
    }) {
        for label in span_tags(&span.text) {
            if !out.contains(&label) {
                out.push(label);
            }
        }
    }
    out.truncate(MAX_TAGS_PER_MESSAGE);
    out
}

/// A tag as the index keys it: NFC, lowercase, no leading `#`.
pub(crate) fn tag_label(text: &str) -> String {
    text.trim_start_matches('#')
        .nfc()
        .collect::<String>()
        .to_lowercase()
}

/// The `#tags` in one run of text: `[alnum_-]` after a `#` that opens at a
/// word boundary (never `##`, `/#`, `&#`).
fn span_tags(text: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut prev: Option<char> = None;
    let mut rest = text;
    while let Some(at) = rest.find('#') {
        let before = rest[..at].chars().next_back().or(prev);
        let opens = before.is_none_or(|p| !p.is_alphanumeric() && !matches!(p, '#' | '/' | '&'));
        let body: String = rest[at + 1..]
            .chars()
            .take_while(|c| c.is_alphanumeric() || *c == '_' || *c == '-')
            .collect();
        if opens && (1..=MAX_TAG_CHARS).contains(&body.chars().count()) {
            out.push(tag_label(&body));
        }
        prev = Some(body.chars().next_back().unwrap_or('#'));
        rest = &rest[at + 1 + body.len()..];
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parse_message;

    #[test]
    fn tags_open_at_a_word_boundary_outside_code_and_links() {
        let blocks = parse_message("#Ship it, #ship ##no a#no /#no &#no #café\n```\n#code\n```");
        assert_eq!(tags(&blocks), ["ship", "café"]);
        let linked = parse_message("[#x](https://x) #y");
        assert_eq!(tags(&linked), ["y"]);
        assert_eq!(
            tokens("Hello, héllo — a HELLO"),
            ["hello", "héllo"].map(String::from).into()
        );
    }
}
