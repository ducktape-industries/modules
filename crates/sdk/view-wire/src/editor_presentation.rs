//! Declarative editor formatting. Document identity is the enclosing Editor node's reference.
use serde::{Deserialize, Serialize};

/// Presentation is bounded independently of the canonical document bytes.
pub const MAX_EDITOR_FORMATS: usize = 256;
pub const MAX_EDITOR_SPANS: usize = 32_768;
pub const MAX_EDITOR_MENU_ITEMS: usize = 64;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum EditorMenuAnchor {
    Caret,
    Line(u32),
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct EditorMenuItem {
    #[serde(deserialize_with = "crate::editor_transaction::decode_document")]
    pub tag: String,
    #[serde(deserialize_with = "crate::editor_transaction::decode_document")]
    pub label: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct EditorMenu {
    pub anchor: EditorMenuAnchor,
    #[serde(deserialize_with = "decode_menu_items")]
    pub items: Vec<EditorMenuItem>,
    pub selected: u32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum EditorGutterButton {
    Plus,
    Handle,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct EditorGutter {
    pub line: u32,
    pub plus: bool,
    pub handle: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct EditorMargin {
    pub line: u32,
    pub count: u32,
}

/// Only these source ranges consume a line press; all other clicks remain native.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct EditorHit {
    pub line: u32,
    pub start: u32,
    pub end: u32,
    pub tag: u32,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct EditorAffordances {
    pub menu: Option<EditorMenu>,
    #[serde(deserialize_with = "decode_gutters")]
    pub gutters: Vec<EditorGutter>,
    #[serde(deserialize_with = "decode_boundaries")]
    pub drop_boundaries: Vec<u32>,
    #[serde(deserialize_with = "decode_margins")]
    pub margins: Vec<EditorMargin>,
    #[serde(deserialize_with = "crate::editor_transaction::decode_document")]
    pub margin_label: String,
    #[serde(deserialize_with = "decode_hits")]
    pub hits: Vec<EditorHit>,
}

/// A presentation interaction is not an edit or a history commit. The editor
/// event envelope supplies the instance and canonical document reference.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum EditorInteraction {
    /// A guest-authored control action ordered after pending native input.
    Action {
        #[serde(deserialize_with = "crate::editor_transaction::decode_document")]
        tag: String,
    },
    LinePress {
        tag: u32,
        position: crate::EditorPosition,
    },
    Gutter {
        line: u32,
        button: EditorGutterButton,
    },
    GutterDrop {
        from: u32,
        boundary: u32,
    },
    Margin {
        line: u32,
    },
    MenuSelect {
        index: u32,
    },
    MenuPick {
        #[serde(deserialize_with = "crate::editor_transaction::decode_document")]
        tag: String,
    },
    MenuDismiss,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct EditorFormat {
    pub style: gpui::StyleRefinement,
    pub line_style: gpui::StyleRefinement,
    pub line_rule: Option<gpui::Hsla>,
    pub strikethrough: Option<gpui::Hsla>,
    /// Underline color, drawn along the span's baseline.
    pub underline: Option<gpui::Hsla>,
}

/// A source range, in UTF-8 byte offsets within a logical line.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct EditorSpan {
    pub line: u32,
    pub start: u32,
    pub end: u32,
    pub format: u16,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct EditorPresentation {
    #[serde(deserialize_with = "decode_formats")]
    pub formats: Vec<EditorFormat>,
    /// Ordered by line, then start; overlapping ranges are rejected.
    #[serde(deserialize_with = "decode_spans")]
    pub spans: Vec<EditorSpan>,
    /// Local editor box presentation, including gutter and margin insets.
    pub style: gpui::StyleRefinement,
    pub affordances: EditorAffordances,
}

impl EditorPresentation {
    pub(super) fn sanitize(&mut self) {
        crate::style_sanitize::sanitize(&mut self.style);
        for format in &mut self.formats {
            crate::style_sanitize::sanitize(&mut format.style);
            crate::style_sanitize::sanitize(&mut format.line_style);
            for color in [
                &mut format.line_rule,
                &mut format.strikethrough,
                &mut format.underline,
            ]
            .into_iter()
            .flatten()
            {
                crate::style_sanitize::sanitize_hsla(color);
            }
        }
        // Interaction tags/ranges are semantic data: never shorten them.
        // Over-limit metadata is rejected by the bounded decoder, preserving
        // the previous healthy frame instead of publishing partial controls.
    }
}

/// Presentation entries share the decoder's node budget: a frame cannot
/// spend on spans what it may not spend on nodes.
fn decode_budgeted<'de, D, T>(d: D, limit: usize, message: &'static str) -> Result<Vec<T>, D::Error>
where
    D: serde::Deserializer<'de>,
    T: Deserialize<'de>,
{
    let values = crate::bounded_vec(d, limit, message)?;
    crate::budget::spend(values.len()).map_err(serde::de::Error::custom)?;
    Ok(values)
}

fn decode_formats<'de, D: serde::Deserializer<'de>>(d: D) -> Result<Vec<EditorFormat>, D::Error> {
    decode_budgeted(d, MAX_EDITOR_FORMATS, "editor format limit")
}

fn decode_spans<'de, D: serde::Deserializer<'de>>(d: D) -> Result<Vec<EditorSpan>, D::Error> {
    decode_budgeted(d, MAX_EDITOR_SPANS, "editor span limit")
}

fn decode_menu_items<'de, D: serde::Deserializer<'de>>(
    d: D,
) -> Result<Vec<EditorMenuItem>, D::Error> {
    decode_budgeted(d, MAX_EDITOR_MENU_ITEMS, "editor menu item limit")
}
fn decode_gutters<'de, D: serde::Deserializer<'de>>(d: D) -> Result<Vec<EditorGutter>, D::Error> {
    decode_budgeted(d, MAX_EDITOR_SPANS, "editor gutter limit")
}
fn decode_boundaries<'de, D: serde::Deserializer<'de>>(d: D) -> Result<Vec<u32>, D::Error> {
    decode_budgeted(d, MAX_EDITOR_SPANS, "editor drop boundary limit")
}
fn decode_margins<'de, D: serde::Deserializer<'de>>(d: D) -> Result<Vec<EditorMargin>, D::Error> {
    decode_budgeted(d, MAX_EDITOR_SPANS, "editor margin limit")
}
fn decode_hits<'de, D: serde::Deserializer<'de>>(d: D) -> Result<Vec<EditorHit>, D::Error> {
    decode_budgeted(d, MAX_EDITOR_SPANS, "editor hit limit")
}

#[cfg(test)]
mod tests {
    use super::*;
    use gpui::Styled as _;

    fn presentation(spans: Vec<EditorSpan>) -> EditorPresentation {
        EditorPresentation {
            formats: vec![EditorFormat::default()],
            spans,
            ..Default::default()
        }
    }

    #[test]
    fn overflow_is_rejected_instead_of_silently_truncating_interactions() {
        let mut value = EditorPresentation::default();
        value.affordances.menu = Some(EditorMenu {
            anchor: EditorMenuAnchor::Caret,
            items: (0..=MAX_EDITOR_MENU_ITEMS)
                .map(|index| EditorMenuItem {
                    tag: index.to_string(),
                    label: format!("Action {index}"),
                })
                .collect(),
            selected: 0,
        });
        value.sanitize();
        assert!(
            crate::decode::<EditorPresentation>(&crate::encode(&value)).is_err(),
            "over-budget action lists must reject the frame, not publish a different menu"
        );
    }

    #[test]
    fn native_span_and_line_styles_bound_untrusted_padding() {
        let mut value = EditorPresentation::default();
        value.formats.push(EditorFormat {
            style: gpui::StyleRefinement::default()
                .pt(gpui::px(-4.5))
                .pr(gpui::px(2.0))
                .pb(gpui::px(-1e9))
                .pl(gpui::px(f32::NAN)),
            line_style: gpui::StyleRefinement::default()
                .pt(gpui::px(-4.5))
                .pr(gpui::px(2.0))
                .pb(gpui::px(-1e9))
                .pl(gpui::px(f32::NAN)),
            ..Default::default()
        });
        value.sanitize();
        let format = &value.formats[0];
        for style in [&format.style, &format.line_style] {
            assert_eq!(style.padding.top, Some(gpui::px(0.).into()));
            assert_eq!(style.padding.right, Some(gpui::px(2.).into()));
            assert_eq!(style.padding.bottom, Some(gpui::px(0.).into()));
            assert_eq!(style.padding.left, Some(gpui::px(0.).into()));
        }
    }

    #[test]
    fn decoder_rejects_oversized_collections() {
        let valid = EditorPresentation::default();
        assert!(crate::decode::<EditorPresentation>(&crate::encode(&valid)).is_ok());
        let oversized = EditorPresentation {
            formats: vec![EditorFormat::default(); MAX_EDITOR_FORMATS + 1],
            spans: vec![],
            ..Default::default()
        };
        let bytes = crate::encode(&oversized);
        assert!(crate::decode::<EditorPresentation>(&bytes).is_err());
        let span = EditorSpan {
            line: 0,
            start: 0,
            end: 0,
            format: 0,
        };
        let bytes = crate::encode(&presentation(vec![span; MAX_EDITOR_SPANS + 1]));
        assert!(crate::decode::<EditorPresentation>(&bytes).is_err());
    }
}
