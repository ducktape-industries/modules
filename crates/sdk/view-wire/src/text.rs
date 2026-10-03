//! One text model. The host's editing engine owns a field's text; the guest
//! holds a copy that follows every [`TextChange`] and asks for edits with
//! [`WidgetCommand::Replace`](crate::WidgetCommand::Replace) against the
//! `revision` it has seen. The host rebases a stale ask over what was typed
//! since, so no key waits on the guest and nothing typed is lost.
use serde::{Deserialize, Serialize};
use std::ops::Range;

/// The most bytes one field holds.
pub const MAX_FIELD_BYTES: usize = 1 << 20;
/// The most atomic spans one field carries.
pub const MAX_FIELD_TOKENS: usize = 256;
/// The most keys one field claims.
pub const MAX_FIELD_CLAIMS: usize = 32;

/// Bytes `start..end` of a field's text; a caret when empty.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct TextRange {
    pub start: u32,
    pub end: u32,
}

impl TextRange {
    pub fn caret(at: usize) -> Self {
        Self {
            start: at as u32,
            end: at as u32,
        }
    }

    pub fn range(self) -> Range<usize> {
        self.start as usize..self.end as usize
    }

    pub fn is_empty(self) -> bool {
        self.start == self.end
    }
}

impl From<Range<usize>> for TextRange {
    fn from(range: Range<usize>) -> Self {
        Self {
            start: range.start as u32,
            end: range.end as u32,
        }
    }
}

/// An atomic span of a field's text (a mention): the engine moves over,
/// selects and deletes it whole. `id` is what the span means to the guest.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct TextToken {
    pub range: TextRange,
    pub id: String,
}

/// A key the guest hears (`on_key`) instead of the engine, matched after
/// IME processing against the key's state.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct KeyClaim {
    pub key: crate::keyboard::Key,
    pub modifiers: gpui::Modifiers,
    /// Add the host platform's command modifier (platform on macOS, control elsewhere).
    pub command: bool,
}

impl KeyClaim {
    pub fn matches(&self, key: &crate::keyboard::KeyState, macos: bool) -> bool {
        let mut modifiers = self.modifiers;
        if self.command {
            if macos {
                modifiers.platform = true;
            } else {
                modifiers.control = true;
            }
        }
        self.key == key.key && modifiers == key.modifiers
    }

    /// The engine's own keys, which no guest claims: Backspace, Delete, Tab
    /// and the undo and redo chords. Editing and history are the engine's.
    pub fn engine_owned(&self) -> bool {
        use crate::keyboard::{Key, Named};
        match &self.key {
            Key::Named(Named::Backspace | Named::Delete | Named::Tab) => true,
            Key::Character(character) => {
                self.command && matches!(character.to_ascii_lowercase().as_str(), "z" | "y")
            }
            _ => false,
        }
    }
}

/// The host's word on a field's text: the whole text as the engine holds it
/// at `revision`, its cursor, the span an IME is still composing in, and
/// the atomic spans where they now lie.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct TextChange {
    pub revision: u64,
    pub text: String,
    pub cursor: TextRange,
    pub preedit: Option<TextRange>,
    #[serde(deserialize_with = "decode_tokens")]
    pub tokens: Vec<TextToken>,
}

pub(crate) fn decode_tokens<'de, D: serde::Deserializer<'de>>(
    d: D,
) -> Result<Vec<TextToken>, D::Error> {
    crate::bounded_vec(d, MAX_FIELD_TOKENS, "field token limit")
}

pub(crate) fn decode_claims<'de, D: serde::Deserializer<'de>>(
    d: D,
) -> Result<Vec<KeyClaim>, D::Error> {
    crate::bounded_vec(d, MAX_FIELD_CLAIMS, "field claim limit")
}

/// What is wrong with a field as the guest sent it, if anything: text over
/// the cap, a cursor or token off the text's character boundaries, tokens
/// out of order or overlapping, a claim on a key the engine owns.
pub fn validate_field(
    value: &str,
    cursor: TextRange,
    tokens: &[TextToken],
    claims: &[KeyClaim],
) -> Result<(), &'static str> {
    if value.len() > MAX_FIELD_BYTES {
        return Err("field text exceeds its cap");
    }
    let on_text = |range: TextRange| {
        range.start <= range.end
            && value.is_char_boundary(range.start as usize)
            && value.is_char_boundary(range.end as usize)
    };
    if !on_text(cursor) {
        return Err("field cursor is off its text");
    }
    let mut last = 0;
    for token in tokens {
        let range = token.range;
        if range.start < last || range.is_empty() || !on_text(range) {
            return Err("field token is off its text");
        }
        if token.id.is_empty() || token.id.len() > crate::MAX_STRING_BYTES {
            return Err("field token id exceeds bounds");
        }
        last = range.end;
    }
    if claims.iter().any(KeyClaim::engine_owned) {
        return Err("field claims a key the engine owns");
    }
    Ok(())
}

/// The one span of `before` that `after` rewrote, and what it reads now:
/// the host's edit log is built from it. Equal blocks are compared first,
/// so a letter typed into a long text costs its neighbourhood, not the text.
pub fn changed_span(before: &str, after: &str) -> Option<(TextRange, String)> {
    if before == after {
        return None;
    }
    let (b, a) = (before.as_bytes(), after.as_bytes());
    let limit = b.len().min(a.len());
    let mut start = 0;
    while start + 64 <= limit && b[start..start + 64] == a[start..start + 64] {
        start += 64;
    }
    while start < limit && b[start] == a[start] {
        start += 1;
    }
    while !(before.is_char_boundary(start) && after.is_char_boundary(start)) {
        start -= 1;
    }
    let limit = limit - start;
    let mut suffix = 0;
    while suffix + 64 <= limit
        && b[b.len() - suffix - 64..b.len() - suffix] == a[a.len() - suffix - 64..a.len() - suffix]
    {
        suffix += 64;
    }
    while suffix < limit && b[b.len() - suffix - 1] == a[a.len() - suffix - 1] {
        suffix += 1;
    }
    while !(before.is_char_boundary(b.len() - suffix) && after.is_char_boundary(a.len() - suffix)) {
        suffix -= 1;
    }
    Some((
        TextRange::from(start..b.len() - suffix),
        after[start..a.len() - suffix].to_owned(),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn span(before: &str, after: &str) -> Option<(Range<usize>, String)> {
        changed_span(before, after).map(|(range, text)| (range.range(), text))
    }

    #[test]
    fn the_changed_span_is_the_smallest_on_character_boundaries() {
        assert_eq!(span("say word now", "say x now"), Some((4..8, "x".into())));
        assert_eq!(span("abc", "abc"), None);
        assert_eq!(span("", "abc"), Some((0..0, "abc".into())));
        assert_eq!(span("abc", ""), Some((0..3, String::new())));
        // a shared lead byte never splits a character
        assert_eq!(span("aé", "aè"), Some((1..3, "è".into())));
        assert_eq!(span("é", "e"), Some((0..2, "e".into())));
        // a long text pays for its neighbourhood only
        let long = "x".repeat(10_000);
        assert_eq!(
            span(&long, &format!("{long}y")),
            Some((10_000..10_000, "y".into()))
        );
        // a repeated letter typed inside a run lands at the run's end,
        // the one place both texts still agree on
        assert_eq!(span("aaa", "aaaa"), Some((3..3, "a".into())));
    }

    #[test]
    fn a_field_is_valid_on_its_own_text_and_claims_no_engine_key() {
        use crate::keyboard::{Key, Named};
        let claim = |key| KeyClaim {
            key,
            modifiers: Default::default(),
            command: false,
        };
        let token = |start, end| TextToken {
            range: TextRange::from(start..end),
            id: "<@1>".into(),
        };
        assert_eq!(
            validate_field("hi @al", TextRange::caret(6), &[token(3, 6)], &[]),
            Ok(())
        );
        assert_eq!(
            validate_field("é", TextRange::caret(1), &[], &[]),
            Err("field cursor is off its text")
        );
        assert_eq!(
            validate_field(
                "abcd",
                TextRange::caret(0),
                &[token(2, 4), token(0, 2)],
                &[]
            ),
            Err("field token is off its text")
        );
        assert_eq!(
            validate_field(
                "abcd",
                TextRange::caret(0),
                &[token(0, 3), token(2, 4)],
                &[]
            ),
            Err("field token is off its text")
        );
        for owned in [Named::Backspace, Named::Delete, Named::Tab] {
            assert_eq!(
                validate_field("", TextRange::default(), &[], &[claim(Key::Named(owned))]),
                Err("field claims a key the engine owns"),
                "{owned:?}"
            );
        }
        let undo = KeyClaim {
            command: true,
            ..claim(Key::Character("z".into()))
        };
        assert!(undo.engine_owned());
        assert!(!claim(Key::Character("z".into())).engine_owned());
        assert!(!claim(Key::Named(Named::Enter)).engine_owned());
        assert_eq!(
            validate_field(
                &"x".repeat(MAX_FIELD_BYTES + 1),
                TextRange::default(),
                &[],
                &[]
            ),
            Err("field text exceeds its cap")
        );
    }
}
