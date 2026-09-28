//! Declarative editor formatting. Document identity is the enclosing Editor node's reference.
use serde::{Deserialize, Serialize};

/// Presentation is bounded independently of the canonical document bytes.
pub const MAX_EDITOR_FORMATS: usize = 256;
pub const MAX_EDITOR_SPANS: usize = 32_768;

/// A presentation interaction is not an edit or a history commit. The editor
/// event envelope supplies the instance and canonical document reference.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum EditorInteraction {
    /// A guest-authored control action ordered after pending native input.
    Action {
        #[serde(deserialize_with = "crate::editor_transaction::decode_name")]
        tag: String,
    },
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct EditorFormat {
    pub style: gpui::StyleRefinement,
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
    /// Local editor box presentation.
    pub style: gpui::StyleRefinement,
}

impl EditorPresentation {
    pub(super) fn sanitize(&mut self) {
        crate::style_sanitize::sanitize(&mut self.style);
        for format in &mut self.formats {
            crate::style_sanitize::sanitize(&mut format.style);
        }
        // Spans are semantic data: never shortened. Over-limit metadata is
        // rejected by the bounded decoder, preserving the previous healthy
        // frame instead of publishing partial formatting.
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
    fn native_span_styles_bound_untrusted_padding() {
        let mut value = EditorPresentation::default();
        value.formats.push(EditorFormat {
            style: gpui::StyleRefinement::default()
                .pt(gpui::px(-4.5))
                .pr(gpui::px(2.0))
                .pb(gpui::px(-1e9))
                .pl(gpui::px(f32::NAN)),
        });
        value.sanitize();
        let style = &value.formats[0].style;
        assert_eq!(style.padding.top, Some(gpui::px(0.).into()));
        assert_eq!(style.padding.right, Some(gpui::px(2.).into()));
        assert_eq!(style.padding.bottom, Some(gpui::px(0.).into()));
        assert_eq!(style.padding.left, Some(gpui::px(0.).into()));
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
