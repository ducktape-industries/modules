//! Serializable accessibility declarations for native GPUI interactivity.
use crate::{Action, AriaCurrent, HasPopup, Invalid, Live};
use gpui::SharedString;
use serde::{Deserialize, Serialize};

/// The most advertised actions, after dedupe by [`Action`].
pub const MAX_ARIA_ACTIONS: usize = 32;
pub const MAX_ARIA_CUSTOM_ACTIONS: usize = 8;
/// The longest aria string: a label, a description, a custom action's words.
pub const MAX_ARIA_TEXT_BYTES: usize = 1024;

/// Sparse on the wire, as [`Interactivity`](crate::Interactivity) is.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, remote = "Self")]
pub struct Aria {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub author_id: Option<SharedString>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub label: Option<SharedString>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<SharedString>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub keyshortcuts: Option<SharedString>,
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub active_descendant: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub value: Option<SharedString>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub placeholder: Option<SharedString>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub selected: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub expanded: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub disabled: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub numeric_value: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub numeric_value_step: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub min_numeric_value: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_numeric_value: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub level: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub position_in_set: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub size_of_set: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub row_index: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub column_index: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub row_count: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub column_count: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub toggled: Option<gpui::Toggled>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub orientation: Option<gpui::Orientation>,
    /// How a change inside the node is announced; `Off` is `None`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub live: Option<Live>,
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub busy: bool,
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub required: bool,
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub read_only: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub invalid: Option<Invalid>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub has_popup: Option<HasPopup>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub current: Option<AriaCurrent>,
    /// Actions the node advertises beyond Click/Focus, each with the
    /// handler route [`crate::Event::A11yAction`] answers with.
    #[serde(
        skip_serializing_if = "Vec::is_empty",
        deserialize_with = "decode_actions"
    )]
    pub actions: Vec<(Action, u32)>,
    /// `(id, description)`; requested as [`Action::CustomAction`] with
    /// `ActionData::CustomAction(id)`, routed through that action's handler.
    #[serde(
        skip_serializing_if = "Vec::is_empty",
        deserialize_with = "decode_custom_actions"
    )]
    pub custom_actions: Vec<(i32, String)>,
}

crate::codec::sparse!(Aria);

fn decode_actions<'de, D: serde::Deserializer<'de>>(
    deserializer: D,
) -> Result<Vec<(Action, u32)>, D::Error> {
    crate::bounded_vec(deserializer, MAX_ARIA_ACTIONS, "too many aria actions")
}

fn decode_custom_actions<'de, D: serde::Deserializer<'de>>(
    deserializer: D,
) -> Result<Vec<(i32, String)>, D::Error> {
    crate::bounded_vec(
        deserializer,
        MAX_ARIA_CUSTOM_ACTIONS,
        "too many aria custom actions",
    )
}
/// Actions the host answers itself, whatever a view advertises.
pub(crate) const HOST_ACTIONS: [Action; 6] = [
    Action::Click,
    Action::Focus,
    Action::Blur,
    Action::SetValue,
    Action::ReplaceSelectedText,
    Action::SetTextSelection,
];

/// The role a view may give a node: `GenericContainer` and `Unknown` say
/// nothing (and trip gpui's debug assert), and the window-level roles are
/// the host's.
pub(crate) fn view_role(role: Option<gpui::Role>) -> Option<gpui::Role> {
    use gpui::Role::*;
    role.filter(|role| {
        !matches!(
            role,
            GenericContainer
                | Unknown
                | Window
                | Application
                | RootWebArea
                | Pane
                | Iframe
                | IframePresentational
                | WebView
                | TitleBar
        )
    })
}

impl Aria {
    /// Bounds everything the node's role does not decide; the rest is
    /// `frame_sanitize::sanitize_interactivity`'s.
    pub(crate) fn sanitize(&mut self) {
        for field in [
            &mut self.author_id,
            &mut self.label,
            &mut self.description,
            &mut self.keyshortcuts,
            &mut self.value,
            &mut self.placeholder,
        ]
        .into_iter()
        .flatten()
        {
            let mut text = field.to_string();
            crate::truncate_to(&mut text, MAX_ARIA_TEXT_BYTES);
            *field = text.into();
        }
        for value in [
            &mut self.numeric_value,
            &mut self.numeric_value_step,
            &mut self.min_numeric_value,
            &mut self.max_numeric_value,
        ]
        .into_iter()
        .flatten()
        {
            *value = if value.is_finite() {
                value.clamp(-1e12, 1e12)
            } else {
                0.
            };
        }
        for value in [
            &mut self.level,
            &mut self.position_in_set,
            &mut self.size_of_set,
            &mut self.row_index,
            &mut self.column_index,
            &mut self.row_count,
            &mut self.column_count,
        ]
        .into_iter()
        .flatten()
        {
            *value = (*value).min(1_000_000);
        }
        if self.live == Some(Live::Off) {
            self.live = None;
        }
        let mut seen = std::collections::HashSet::new();
        self.actions
            .retain(|(action, _)| !HOST_ACTIONS.contains(action) && seen.insert(*action));
        self.actions.truncate(MAX_ARIA_ACTIONS);
        // deduped first, as the actions are, so repeats spend no place
        let mut seen = std::collections::HashSet::new();
        self.custom_actions.retain_mut(|(id, description)| {
            crate::truncate_to(description, MAX_ARIA_TEXT_BYTES);
            seen.insert(*id)
        });
        self.custom_actions.truncate(MAX_ARIA_CUSTOM_ACTIONS);
    }
}
