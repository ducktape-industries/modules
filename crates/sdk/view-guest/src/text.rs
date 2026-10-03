//! The guest's copy of a text field. The host's editing engine owns the
//! text: this copy follows every [`TextChange`] the host sends and asks for
//! edits against the host's `revision`, so no key waits on the guest and
//! nothing typed is lost.
use crate::wire::{self, TextChange, TextRange, TextToken};
use gpui::ElementId;
use serde::{Deserialize, Serialize};
use std::cell::Cell;
use std::ops::Range;

thread_local! {
    /// Above every generation made or lowered so far: a fresh field is a
    /// generation the host has not seen, whatever it replaces at its path.
    static GENERATIONS: Cell<u64> = const { Cell::new(0) };
}

fn fresh_generation() -> u64 {
    GENERATIONS.with(|generations| {
        let next = generations.get() + 1;
        generations.set(next);
        next
    })
}

/// Lowering saw `generation`: nothing made from now on is older than it.
pub(crate) fn lowered_generation(generation: u64) {
    GENERATIONS.with(|generations| generations.set(generations.get().max(generation)));
}

/// A field's text as the guest knows it. [`apply`](Self::apply) follows the
/// host; [`replace`](Self::replace) and [`replace_all`](Self::replace_all)
/// ask the host for an edit the writer can undo; [`reset`](Self::reset) is
/// a new document, adopted whole, which is the one way the writer's undo is
/// cleared.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct TextField {
    pub text: String,
    /// Bytes selected; a caret when empty.
    pub cursor: TextRange,
    /// The atomic spans (mentions) and what they mean, where they now lie.
    pub tokens: Vec<TextToken>,
    /// The span an IME is still composing in, part of `text` already.
    pub preedit: Option<TextRange>,
    /// Moves on `reset`: the host adopts the text whole when it does.
    pub generation: u64,
    /// The host's, as of the last change applied here.
    pub revision: u64,
}

impl Default for TextField {
    fn default() -> Self {
        Self::new("")
    }
}

impl TextField {
    pub fn new(text: impl Into<String>) -> Self {
        let text = text.into();
        Self {
            cursor: TextRange::caret(text.len()),
            text,
            tokens: Vec::new(),
            preedit: None,
            generation: fresh_generation(),
            revision: 0,
        }
    }

    /// A new document in the field: `text`, caret at its end, no tokens.
    pub fn reset(&mut self, text: impl Into<String>) {
        *self = Self {
            revision: self.revision,
            ..Self::new(text)
        };
    }

    /// The host's word on the text, taken as is — when it is on this
    /// document. A change to a generation this field has left (one the host
    /// echoed before it adopted the reset) is not: the reset's text stands
    /// until the host adopts it. Answers whether the change was taken.
    pub fn apply(&mut self, change: &TextChange) -> bool {
        if change.generation != self.generation {
            return false;
        }
        self.text.clone_from(&change.text);
        self.cursor = change.cursor;
        self.preedit = change.preedit;
        self.tokens.clone_from(&change.tokens);
        self.revision = change.revision;
        true
    }

    pub fn is_blank(&self) -> bool {
        self.text.trim().is_empty()
    }

    pub fn selection(&self) -> Range<usize> {
        self.cursor.range()
    }

    /// Asks the host to put `text` in place of `range` of the text as
    /// known here, the caret at `cursor` after it — an offset into the new
    /// text, or one before `range`. With `token`, the inserted text is one
    /// atomic span meaning `token`. The ask names this document's
    /// generation: after a `reset`, `revision` is the host's count of the
    /// old one, and only the generation says which text `range` is bytes of.
    pub fn replace(
        &self,
        target: impl Into<ElementId>,
        range: Range<usize>,
        text: impl Into<String>,
        token: Option<String>,
        cursor: usize,
    ) -> wire::WidgetCommand {
        wire::WidgetCommand::Replace {
            target: vec![crate::element::wire_id(target.into())],
            generation: self.generation,
            revision: self.revision,
            range: TextRange::from(range),
            text: text.into(),
            token,
            cursor: TextRange::caret(cursor),
        }
    }

    /// Asks the host to put `text` in place of the whole text as known
    /// here, the caret after it.
    pub fn replace_all(
        &self,
        target: impl Into<ElementId>,
        text: impl Into<String>,
    ) -> wire::WidgetCommand {
        let text = text.into();
        let caret = text.len();
        self.replace(target, 0..self.text.len(), text, None, caret)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_fresh_field_is_a_generation_the_host_has_not_seen() {
        let first = TextField::default();
        let second = TextField::new("x");
        assert!(second.generation > first.generation);
        lowered_generation(1_000);
        let mut later = TextField::default();
        assert!(later.generation > 1_000);
        let before = later.generation;
        later.reset("again");
        assert!(later.generation > before);
        assert_eq!(later.cursor, TextRange::caret(5));
    }

    #[test]
    fn a_replace_speaks_at_the_revision_the_field_knows() {
        let mut field = TextField::new("say word now");
        let change = |generation, revision, text: &str| TextChange {
            generation,
            revision,
            edit: None,
            text: text.into(),
            cursor: TextRange::from(4..8),
            preedit: None,
            tokens: Vec::new(),
        };
        assert!(field.apply(&change(field.generation, 7, "say word now")));
        let wire::WidgetCommand::Replace {
            generation,
            revision,
            range,
            text,
            cursor,
            ..
        } = field.replace_all("f", "")
        else {
            unreachable!()
        };
        assert_eq!(
            (generation, revision, range, text, cursor),
            (
                field.generation,
                7,
                TextRange::from(0..12),
                String::new(),
                TextRange::caret(0)
            )
        );
        // a reset is a new document; the host's word on the old one, echoed
        // before it adopted the reset, is not a word on this one
        let left = field.generation;
        field.reset("fresh");
        assert!(!field.apply(&change(left, 8, "say word now!")));
        assert_eq!((field.text.as_str(), field.revision), ("fresh", 7));
        assert!(field.apply(&change(field.generation, 9, "fresh")));
        assert_eq!(field.revision, 9);
    }
}
