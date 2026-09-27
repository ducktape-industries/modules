//! The text and caret a view owns for a host editor: commits patch it,
//! transfers mirror it, and a reset fences off what came before.
use crate::wire;
use std::rc::Rc;

/// Editor snapshot record: text, caret, reset fence and the host's revision.
/// Field names and order are the snapshot bytes.
#[derive(Clone, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
struct EditorState {
    text: String,
    cursor: wire::EditorCursor,
    reset: u64,
    revision: u64,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Editor(Rc<EditorState>, u64);
impl Editor {
    pub fn new(text: impl Into<String>) -> Self {
        let mut state = EditorState {
            text: text.into(),
            ..Default::default()
        };
        assert!(
            state.text.len() <= wire::editor_document::MAX_EDITOR_DOCUMENT_BYTES,
            "editor document exceeds text limit"
        );
        state.cursor.clamp(&state.text);
        Self(Rc::new(state), 0)
    }
    pub fn text(&self) -> String {
        self.0.text.clone()
    }
    pub(crate) fn text_ref(&self) -> &str {
        &self.0.text
    }
    /// Borrow the canonical editor state for deterministic presentation.
    pub fn state_view(&self) -> crate::EditorStateView<'_> {
        crate::EditorStateView {
            text: &self.0.text,
            cursor: self.0.cursor,
            reset: self.0.reset,
            text_revision: self.1,
            revision: self.0.revision,
        }
    }
    pub fn document_reference(&self, document: String) -> wire::editor_document::EditorDocumentRef {
        wire::editor_document::EditorDocumentRef {
            document,
            reset: self.0.reset,
            text_revision: self.1,
            revision: self.0.revision,
            cursor: self.0.cursor,
            byte_len: self.0.text.len() as u32,
        }
    }
    pub fn cursor(&self) -> wire::EditorCursor {
        self.0.cursor
    }
    pub fn reset_revision(&self) -> u64 {
        self.0.reset
    }
    /// An authoritative assignment, including an identical-text document
    /// replacement: the reset fence advances past this document's.
    pub fn replace(&mut self, mut next: Self) {
        Rc::make_mut(&mut next.0).reset = self
            .0
            .reset
            .checked_add(1)
            .expect("editor reset revisions exhausted");
        assert!(
            next.0.text.len() <= wire::editor_document::MAX_EDITOR_DOCUMENT_BYTES,
            "editor document exceeds text limit"
        );
        let state = Rc::make_mut(&mut next.0);
        state.cursor.clamp(&state.text);
        next.1 = 0;
        *self = next;
    }
    pub(crate) fn install_mirror(
        &mut self,
        text: String,
        target: &wire::editor_document::EditorDocumentRef,
    ) -> bool {
        if target.reset != self.0.reset
            || target.revision < self.0.revision
            || target.text_revision < self.1
            || target.validate_text(&text).is_err()
        {
            return false;
        }
        let state = Rc::make_mut(&mut self.0);
        state.text = text;
        state.cursor = target.cursor;
        state.revision = target.revision;
        self.1 = target.text_revision;
        true
    }
    pub(crate) fn accept_patch(
        &mut self,
        before: &wire::editor_document::EditorDocumentRef,
        after: &wire::editor_document::EditorDocumentRef,
        patches: &[wire::EditorPatch],
    ) -> Option<Option<String>> {
        if &self.document_reference(before.document.clone()) != before
            || before.document != after.document
            || before.reset != after.reset
            || after.revision <= before.revision
        {
            return None;
        }
        if patches.is_empty() {
            if after.text_revision != before.text_revision
                || after.validate_text(&self.0.text).is_err()
            {
                return None;
            }
            let state = Rc::make_mut(&mut self.0);
            state.cursor = after.cursor;
            state.revision = after.revision;
            return Some(None);
        }
        let text =
            wire::editor_transaction::patched_editor_text(&self.0.text, patches, after.cursor)
                .ok()?;
        let expected = before
            .text_revision
            .checked_add(u64::from(text != self.0.text))?;
        if after.text_revision != expected || after.validate_text(&text).is_err() {
            return None;
        }
        let state = Rc::make_mut(&mut self.0);
        let old = std::mem::replace(&mut state.text, text);
        state.cursor = after.cursor;
        state.revision = after.revision;
        self.1 = after.text_revision;
        Some(Some(old))
    }
    pub fn move_to(&mut self, mut cursor: wire::EditorCursor) {
        cursor.clamp(&self.0.text);
        let state = Rc::make_mut(&mut self.0);
        state.cursor = cursor;
        state.reset = state
            .reset
            .checked_add(1)
            .expect("editor reset revisions exhausted");
        self.1 = 0;
    }
    pub fn snapshot(&self) -> Vec<u8> {
        wire::encode(&(&*self.0, self.1))
    }
    pub fn restore(bytes: &[u8]) -> Option<Self> {
        let (state, text_revision): (EditorState, u64) = wire::decode(bytes).ok()?;
        if state.text.len() > wire::editor_document::MAX_EDITOR_DOCUMENT_BYTES {
            return None;
        }
        let mut cursor = state.cursor;
        cursor.clamp(&state.text);
        (cursor == state.cursor).then_some(Self(Rc::new(state), text_revision))
    }
}

/// A view snapshot carries an editor as its [`Editor::snapshot`] bytes.
impl serde::Serialize for Editor {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serde::Serialize::serialize(&self.snapshot(), serializer)
    }
}

impl<'de> serde::Deserialize<'de> for Editor {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let bytes = <Vec<u8> as serde::Deserialize>::deserialize(deserializer)?;
        Editor::restore(&bytes).ok_or_else(|| serde::de::Error::custom("invalid editor document"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn caret(column: u32) -> wire::EditorCursor {
        wire::EditorCursor {
            position: wire::EditorPosition { line: 0, column },
            selection: None,
        }
    }

    #[test]
    fn a_mirror_installs_at_the_hosts_revisions_and_never_across_a_reset() {
        let mut editor = Editor::new("a");
        let mut target = editor.document_reference("app:draft".into());
        target.revision = 2;
        target.text_revision = 1;
        target.byte_len = 2;
        target.cursor = caret(2);
        assert!(!editor.install_mirror("abc".into(), &target));
        assert!(editor.install_mirror("ab".into(), &target));
        assert_eq!(editor.text(), "ab");
        assert_eq!(editor.cursor(), caret(2));
        let installed = editor.document_reference("app:draft".into());
        assert_eq!((installed.revision, installed.text_revision), (2, 1));
        target.reset += 1;
        assert!(!editor.install_mirror("ab".into(), &target));
    }

    #[test]
    fn frame_snapshot_shares_immutable_text_and_detaches_before_mutation() {
        let mut editor = Editor::new("before");
        let frame = editor.clone();
        assert!(Rc::ptr_eq(&editor.0, &frame.0));

        editor.move_to(caret(3));

        assert_eq!(frame.cursor(), caret(0));
        assert_eq!(editor.cursor(), caret(3));
        assert!(!Rc::ptr_eq(&editor.0, &frame.0));
    }

    #[test]
    fn document_references_separate_text_revisions_from_caret_commits() {
        let mut editor = Editor::new("a");
        let before = editor.document_reference("app:draft".into());
        let mut after = before.clone();
        after.revision = 4;
        after.text_revision = 1;
        after.byte_len = 2;
        let patches = [wire::EditorPatch {
            start_byte: 1,
            end_byte: 1,
            replacement: "b".into(),
        }];
        assert!(editor.accept_patch(&before, &after, &patches).is_some());
        let typed = editor.document_reference("app:draft".into());
        assert_eq!(
            (typed.text_revision, typed.revision, typed.byte_len),
            (1, 4, 2)
        );
        let mut moved = typed.clone();
        moved.revision = 5;
        assert!(editor.accept_patch(&typed, &moved, &[]).is_some());
        let moved = editor.document_reference("app:draft".into());
        assert_eq!((moved.text_revision, moved.revision), (1, 5));
        assert_eq!(Editor::restore(&editor.snapshot()), Some(editor.clone()));
        editor.move_to(wire::EditorCursor::default());
        assert_eq!(
            editor.document_reference("app:draft".into()).text_revision,
            0
        );
    }

    #[test]
    fn an_oversized_document_is_refused() {
        let oversized = "x".repeat(wire::editor_document::MAX_EDITOR_DOCUMENT_BYTES + 1);
        assert!(std::panic::catch_unwind(|| Editor::new(oversized)).is_err());
    }

    #[test]
    fn a_caret_move_and_a_replacement_each_fence_the_document() {
        let mut editor = Editor::new("a");
        editor.move_to(wire::EditorCursor::default());
        assert_eq!(editor.reset_revision(), 1);
        editor.replace(Editor::new("한글"));
        assert_eq!(editor.reset_revision(), 2);
        assert_eq!(editor.cursor(), wire::EditorCursor::default());
        assert_eq!(Editor::restore(&editor.snapshot()), Some(editor.clone()));
    }
}

#[cfg(test)]
mod revision_tests {
    use super::*;

    #[test]
    fn same_text_replacement_preserves_the_verified_text_revision() {
        let mut editor = Editor::new("same");
        let before = editor.document_reference("app:doc".into());
        let mut after = before.clone();
        after.revision += 1;
        let patches = [wire::EditorPatch {
            start_byte: 0,
            end_byte: 4,
            replacement: "same".into(),
        }];
        let mut invalid = after.clone();
        invalid.text_revision += 1;
        assert!(
            editor.accept_patch(&before, &invalid, &patches).is_none(),
            "patch presence alone cannot advance the text revision"
        );
        assert_eq!(editor.document_reference("app:doc".into()), before);
        assert!(editor.accept_patch(&before, &after, &patches).is_some());
        assert_eq!(editor.document_reference("app:doc".into()), after);
    }
}
