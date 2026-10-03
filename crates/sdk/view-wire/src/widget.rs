//! Commands applied only to the requesting guest's mounted widget tree.
use crate::TextRange;
use serde::{Deserialize, Serialize};

/// Full authored typed ancestry of one mounted element, including the target.
pub type WidgetTarget = Vec<crate::ElementIdWire>;

/// Payload of `host.widget`: a mutation of the mounted tree, answered with
/// an encoded unit. Targets are the tree's qualified keys.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum WidgetCommand {
    /// Replace `range` of a field's text, as the guest read it at the
    /// host's `revision`, with `text` — one atomic span when `token` names
    /// it — and put the cursor at `cursor`. Edits the engine made since
    /// `revision` are rebased over, never lost, and the edit is one the
    /// writer can undo.
    Replace {
        target: WidgetTarget,
        revision: u64,
        range: TextRange,
        text: String,
        token: Option<String>,
        cursor: TextRange,
    },
    FocusPrevious,
    FocusNext,
    Focus {
        target: WidgetTarget,
    },
    FocusHandle {
        handle: u64,
    },
    CursorFront {
        target: WidgetTarget,
    },
    CursorEnd {
        target: WidgetTarget,
    },
    Cursor {
        target: WidgetTarget,
        position: u32,
    },
    SelectAll {
        target: WidgetTarget,
    },
    Select {
        target: WidgetTarget,
        start: u32,
        end: u32,
    },
    Snap {
        target: WidgetTarget,
        x: f32,
        y: f32,
    },
    SnapEnd {
        target: WidgetTarget,
    },
    ScrollTo {
        target: WidgetTarget,
        x: f32,
        y: f32,
    },
    ScrollBy {
        target: WidgetTarget,
        x: f32,
        y: f32,
    },
}

impl WidgetCommand {
    /// Bound a decoded request without changing its target identity.
    pub fn validate(&mut self) -> Result<(), String> {
        let target = match self {
            Self::FocusPrevious | Self::FocusNext | Self::FocusHandle { .. } => return Ok(()),
            Self::Replace { target, .. }
            | Self::Focus { target }
            | Self::CursorFront { target }
            | Self::CursorEnd { target }
            | Self::Cursor { target, .. }
            | Self::SelectAll { target }
            | Self::Select { target, .. }
            | Self::Snap { target, .. }
            | Self::SnapEnd { target }
            | Self::ScrollTo { target, .. }
            | Self::ScrollBy { target, .. } => target,
        };
        if target.is_empty() || target.len() > crate::MAX_DEPTH {
            return Err("widget target path is empty or too deep".into());
        }
        for id in target {
            id.validate_host()?;
        }
        if let Self::Replace {
            range,
            text,
            token,
            cursor,
            ..
        } = self
        {
            // the range is bytes of a text under the cap; the cursor is bytes
            // of the text the replace can leave, at most the cap plus its own
            let off = |range: &TextRange| range.start > range.end;
            if off(range)
                || off(cursor)
                || text.len() > crate::text::MAX_FIELD_BYTES
                || range.end as usize > crate::text::MAX_FIELD_BYTES
                || cursor.end as usize > crate::text::MAX_FIELD_BYTES + text.len()
            {
                return Err("replace range or text exceeds bounds".into());
            }
            if token
                .as_ref()
                .is_some_and(|id| id.is_empty() || id.len() > crate::MAX_STRING_BYTES)
            {
                return Err("replace token id exceeds bounds".into());
            }
        }
        match self {
            Self::Snap { x, y, .. } | Self::ScrollTo { x, y, .. } | Self::ScrollBy { x, y, .. }
                if !x.is_finite() || !y.is_finite() =>
            {
                return Err("widget offsets must be finite".into());
            }
            _ => {}
        }
        if let Self::Snap { x, y, .. } = self {
            *x = x.clamp(0.0, 1.0);
            *y = y.clamp(0.0, 1.0);
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn target(id: crate::ElementIdWire) -> WidgetTarget {
        vec![id]
    }

    #[test]
    fn a_replace_keeps_its_target_and_rejects_unbounded_text() {
        let mut command = WidgetCommand::Replace {
            target: target(crate::ElementIdWire::Name("Other/body".into())),
            revision: 3,
            range: TextRange { start: 0, end: 2 },
            text: "@alice".into(),
            token: Some("<@1>".into()),
            cursor: TextRange::caret(6),
        };
        assert!(command.validate().is_ok());
        assert_eq!(
            crate::decode::<WidgetCommand>(&crate::encode(&command)).unwrap(),
            command
        );
        let WidgetCommand::Replace { text, .. } = &mut command else {
            unreachable!()
        };
        *text = "x".repeat(crate::text::MAX_FIELD_BYTES + 1);
        assert!(command.validate().is_err());
        let WidgetCommand::Replace { text, range, .. } = &mut command else {
            unreachable!()
        };
        *text = String::new();
        *range = TextRange { start: 2, end: 0 };
        assert!(command.validate().is_err());
        // a range or cursor off any text a field can hold is refused, not
        // carried into the host's arithmetic
        let WidgetCommand::Replace { range, .. } = &mut command else {
            unreachable!()
        };
        *range = TextRange::from(0..crate::text::MAX_FIELD_BYTES + 1);
        assert!(command.validate().is_err());
        let WidgetCommand::Replace { range, cursor, .. } = &mut command else {
            unreachable!()
        };
        *range = TextRange::from(0..2);
        *cursor = TextRange::caret(crate::text::MAX_FIELD_BYTES + 1);
        assert!(command.validate().is_err());
        let WidgetCommand::Replace { text, cursor, .. } = &mut command else {
            unreachable!()
        };
        *text = "ab".into();
        *cursor = TextRange::caret(crate::text::MAX_FIELD_BYTES + 2);
        assert!(
            command.validate().is_ok(),
            "the end of the text the replace can leave"
        );
    }

    #[test]
    fn widget_targets_are_rejected_whole_instead_of_redirected() {
        let key = crate::ElementIdWire::Name("가".repeat(crate::MAX_STRING_BYTES / 3 + 1).into());
        let mut command = WidgetCommand::Focus {
            target: target(key.clone()),
        };
        assert!(
            command.validate().is_err(),
            "oversized target must be refused"
        );
        assert_eq!(
            command,
            WidgetCommand::Focus {
                target: target(key)
            }
        );
        let expected = WidgetCommand::Focus {
            target: vec![
                crate::ElementIdWire::Integer(1),
                crate::ElementIdWire::Name("draft".into()),
            ],
        };
        let mut decoded: WidgetCommand = crate::decode(&crate::encode(&expected)).unwrap();
        decoded.validate().unwrap();
        assert_eq!(decoded, expected);
    }

    #[test]
    fn widget_offsets_reject_nonfinite_values_and_preserve_scroll_direction() {
        for value in [f32::NAN, f32::INFINITY, f32::NEG_INFINITY] {
            for mut command in [
                WidgetCommand::Snap {
                    target: target(crate::ElementIdWire::Name("list".into())),
                    x: value,
                    y: 0.0,
                },
                WidgetCommand::ScrollTo {
                    target: target(crate::ElementIdWire::Name("list".into())),
                    x: 0.0,
                    y: value,
                },
                WidgetCommand::ScrollBy {
                    target: target(crate::ElementIdWire::Name("list".into())),
                    x: value,
                    y: 0.0,
                },
            ] {
                assert!(
                    command.validate().is_err(),
                    "nonfinite offset must be refused"
                );
            }
        }
        let mut snap = WidgetCommand::Snap {
            target: target(crate::ElementIdWire::Name("list".into())),
            x: -1.0,
            y: 2.0,
        };
        snap.validate().unwrap();
        assert_eq!(
            snap,
            WidgetCommand::Snap {
                target: target(crate::ElementIdWire::Name("list".into())),
                x: 0.0,
                y: 1.0
            }
        );
        let expected = WidgetCommand::ScrollBy {
            target: target(crate::ElementIdWire::Name("list".into())),
            x: -24.0,
            y: 10.0,
        };
        let mut command = expected.clone();
        command.validate().unwrap();
        assert_eq!(command, expected);
    }
}
