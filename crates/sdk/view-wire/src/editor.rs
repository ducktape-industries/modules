//! Renderer-independent editor positions: zero-based lines and UTF-8 byte columns.
use serde::{Deserialize, Serialize};
use unicode_segmentation::UnicodeSegmentation;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct EditorPosition {
    pub line: u32,
    pub column: u32,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct EditorCursor {
    /// Active end of the selection (the caret).
    pub position: EditorPosition,
    /// Fixed end, without sorting or losing selection direction. None clears selection.
    pub selection: Option<EditorPosition>,
}

/// Logical lines used by native Content, excluding LF/CRLF/CR/LFCR terminators.
/// Empty documents and trailing terminators retain their final empty line.
pub fn editor_lines(text: &str) -> impl Iterator<Item = &str> {
    let mut remaining = Some(text);
    std::iter::from_fn(move || {
        let text = remaining.take()?;
        // ASCII delimiters are UTF-8 boundaries. The portable byte search skips
        // whole words of ordinary prose on repeated line scans.
        let end = memchr::memchr2(b'\r', b'\n', text.as_bytes()).unwrap_or(text.len());
        if end < text.len() {
            let ending = &text[end..];
            let width = if ending.starts_with("\r\n") || ending.starts_with("\n\r") {
                2
            } else {
                1
            };
            remaining = Some(&text[end + width..]);
        }
        Some(&text[..end])
    })
}

/// The byte offset of `position` in `text`: a column past the line's end
/// lands at the end of the line, a line past the last at the end of the text.
pub fn editor_offset(text: &str, position: EditorPosition) -> usize {
    let Some(line) = editor_lines(text).nth(position.line as usize) else {
        return text.len();
    };
    let start = line.as_ptr() as usize - text.as_ptr() as usize;
    start + (position.column as usize).min(line.len())
}

/// The position of byte `at` in `text`, snapped back to a char boundary.
pub fn editor_position(text: &str, mut at: usize) -> EditorPosition {
    at = at.min(text.len());
    while !text.is_char_boundary(at) {
        at -= 1;
    }
    let (line, source) = editor_lines(text)
        .enumerate()
        .take_while(|(_, line)| line.as_ptr() as usize - text.as_ptr() as usize <= at)
        .last()
        .expect("editor has at least one logical line");
    let start = source.as_ptr() as usize - text.as_ptr() as usize;
    EditorPosition {
        line: line as u32,
        column: (at - start).min(source.len()) as u32,
    }
}

impl EditorPosition {
    fn clamp(&mut self, text: &str) {
        let (line, source) = editor_lines(text)
            .enumerate()
            .take((self.line as usize).saturating_add(1))
            .last()
            .unwrap();
        self.line = line as u32;
        let column = (self.column as usize).min(source.len());
        self.column = source
            .grapheme_indices(true)
            .map(|(at, _)| at)
            .chain(std::iter::once(source.len()))
            .take_while(|at| *at <= column)
            .last()
            .unwrap_or(0) as u32;
    }
}
impl EditorCursor {
    /// Clamp both ends backward to extended grapheme boundaries in bounded text.
    pub fn clamp(&mut self, text: &str) {
        self.position.clamp(text);
        if let Some(anchor) = &mut self.selection {
            anchor.clamp(text);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn logical_lines_preserve_mixed_terminators_and_utf8_byte_ranges() {
        let long = "한글 e\u{301} 👩‍💻 ordinary text ".repeat(4096);
        for first in ["\n", "\r", "\r\n", "\n\r"] {
            for second in ["\n", "\r", "\r\n", "\n\r"] {
                let text = format!("{long}{first}middle 한글{second}tail 👩‍💻");
                assert_eq!(
                    editor_lines(&text).collect::<Vec<_>>(),
                    [&long, "middle 한글", "tail 👩‍💻"],
                    "terminators {first:?}, {second:?}"
                );
            }
        }
        for (text, expected) in [
            ("", vec![""]),
            ("\r\n", vec!["", ""]),
            ("\n\r", vec!["", ""]),
            ("\r\r\n\n", vec!["", "", "", ""]),
            ("한\r\n글\n\r", vec!["한", "글", ""]),
        ] {
            assert_eq!(editor_lines(text).collect::<Vec<_>>(), expected);
        }
    }

    #[test]
    fn offsets_and_positions_agree_on_every_terminator() {
        for ending in ["\n", "\r\n", "\r", "\n\r"] {
            let text = format!("a{ending}bc");
            let second = EditorPosition { line: 1, column: 1 };
            let at = 1 + ending.len() + 1;
            assert_eq!(editor_offset(&text, second), at, "{ending:?}");
            assert_eq!(editor_position(&text, at), second, "{ending:?}");
        }
        assert_eq!(
            editor_offset("a\nb", EditorPosition { line: 5, column: 9 }),
            3
        );
        assert_eq!(
            editor_position("한", 1),
            EditorPosition { line: 0, column: 0 }
        );
    }

    #[test]
    fn columns_exclude_native_line_ending_bytes() {
        for ending in ["\n", "\r\n", "\r", "\n\r"] {
            let text = format!("한{ending}글{ending}");
            let mut cursor = EditorCursor {
                position: EditorPosition {
                    line: 0,
                    column: u32::MAX,
                },
                selection: Some(EditorPosition {
                    line: 2,
                    column: 20,
                }),
            };
            cursor.clamp(&text);
            assert_eq!(
                cursor.position.column, 3,
                "line ending {ending:?} is not a column"
            );
            assert_eq!(
                cursor.selection.unwrap(),
                EditorPosition { line: 2, column: 0 }
            );
        }
    }

    #[test]
    fn byte_positions_snap_backward_without_splitting_graphemes() {
        for (text, inside, expected) in [
            ("- 한글", 7, 5),
            ("- 👍🏽", 6, 2),
            ("e\u{301}", 1, 0),
            ("👩‍💻", 7, 0),
        ] {
            let mut cursor = EditorCursor {
                position: EditorPosition {
                    line: 0,
                    column: inside,
                },
                selection: Some(EditorPosition {
                    line: 9,
                    column: u32::MAX,
                }),
            };
            cursor.clamp(text);
            assert_eq!(cursor.position.column, expected, "{text}");
            assert_eq!(
                cursor.selection,
                Some(EditorPosition {
                    line: 0,
                    column: text.len() as u32
                })
            );
        }
        let mut cursor = EditorCursor {
            position: EditorPosition {
                line: u32::MAX,
                column: u32::MAX,
            },
            selection: None,
        };
        cursor.clamp("한글\n");
        assert_eq!(cursor.position, EditorPosition { line: 1, column: 0 });
        assert_eq!(cursor.selection, None);
    }
}
