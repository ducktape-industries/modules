//! Serializable accessibility declarations for native GPUI interactivity.
use crate::{Action, AriaCurrent, ElementIdWire, HasPopup, Invalid, Live};
use gpui::SharedString;
use serde::{Deserialize, Serialize};

/// The most targets in one relation list (`labelled_by`, `described_by`, `controls`).
pub const MAX_ARIA_RELATIONS: usize = 16;
/// The most advertised actions, after dedupe by [`Action`].
pub const MAX_ARIA_ACTIONS: usize = 32;
pub const MAX_ARIA_CUSTOM_ACTIONS: usize = 8;
/// The longest aria string: a label, a description, a custom action's words.
pub const MAX_ARIA_TEXT_BYTES: usize = 1024;

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
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
    /// Relation targets, each the target's authored path from the view
    /// root, as [`crate::Node::List`] names its own; the host resolves them.
    #[serde(
        skip_serializing_if = "Vec::is_empty",
        deserialize_with = "decode_relations"
    )]
    pub labelled_by: Vec<Vec<ElementIdWire>>,
    #[serde(
        skip_serializing_if = "Vec::is_empty",
        deserialize_with = "decode_relations"
    )]
    pub described_by: Vec<Vec<ElementIdWire>>,
    #[serde(
        skip_serializing_if = "Vec::is_empty",
        deserialize_with = "decode_relations"
    )]
    pub controls: Vec<Vec<ElementIdWire>>,
    #[serde(
        skip_serializing_if = "Option::is_none",
        deserialize_with = "decode_target"
    )]
    pub error_message: Option<Vec<ElementIdWire>>,
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

/// One relation target, bounded at decode like a list's path.
struct Target(Vec<ElementIdWire>);

impl<'de> Deserialize<'de> for Target {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        crate::bounded_vec(
            deserializer,
            crate::MAX_DEPTH,
            "aria relation target is too deep",
        )
        .map(Target)
    }
}

fn decode_relations<'de, D: serde::Deserializer<'de>>(
    deserializer: D,
) -> Result<Vec<Vec<ElementIdWire>>, D::Error> {
    let targets: Vec<Target> =
        crate::bounded_vec(deserializer, MAX_ARIA_RELATIONS, "too many aria relations")?;
    Ok(targets.into_iter().map(|Target(path)| path).collect())
}

fn decode_target<'de, D: serde::Deserializer<'de>>(
    deserializer: D,
) -> Result<Option<Vec<ElementIdWire>>, D::Error> {
    Ok(Option::<Target>::deserialize(deserializer)?.map(|Target(path)| path))
}

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

/// A relation target is a path a host can resolve, like a list's.
fn check_target(path: &[ElementIdWire]) -> Result<(), &'static str> {
    if path.len() > crate::MAX_DEPTH {
        return Err("aria relation target is too deep");
    }
    path.iter().try_for_each(ElementIdWire::validate_host)
}

impl Aria {
    /// Bounds everything the node's role does not decide; the rest is
    /// `frame_sanitize::sanitize_interactivity`'s.
    pub(crate) fn sanitize(&mut self) -> Result<(), &'static str> {
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
        for relation in [
            &mut self.labelled_by,
            &mut self.described_by,
            &mut self.controls,
        ] {
            relation.truncate(MAX_ARIA_RELATIONS);
            relation.iter().try_for_each(|path| check_target(path))?;
        }
        self.error_message.as_deref().map_or(Ok(()), check_target)?;
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
        Ok(())
    }
}
