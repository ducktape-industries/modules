//! Native editor box presentation and guest-authored editor interactions.
//! Document identity is the enclosing Editor node's reference.
use serde::{Deserialize, Serialize};

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
pub struct EditorPresentation {
    /// Local editor box presentation.
    pub style: gpui::StyleRefinement,
}

impl EditorPresentation {
    pub(super) fn sanitize(&mut self) {
        crate::style_sanitize::sanitize(&mut self.style);
    }
}
