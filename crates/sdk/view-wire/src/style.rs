//! The style refinements and listener routes an element lowers to native GPUI.
use crate::StyleId;
use gpui::SharedString;
use serde::{Deserialize, Serialize};

/// Declarative interactivity lowered into native GPUI's `Interactivity`.
/// Sparse on the wire: a node sets a few of these and leaves the rest out.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields, remote = "Self")]
pub struct Interactivity {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub role: Option<gpui::Role>,
    #[serde(skip_serializing_if = "crate::is_default")]
    pub aria: crate::Aria,
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub focusable: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tab_stop: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tab_index: Option<i32>,
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub tab_group: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub focus: Option<StyleId>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub in_focus: Option<StyleId>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub focus_visible: Option<StyleId>,
    /// Guest-app-local opaque focus allocation. It is never an authored element ID.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub focus_handle: Option<u64>,
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub occlude: bool,
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub block_mouse_except_scroll: bool,
    #[serde(skip_serializing_if = "crate::is_default")]
    pub hover_listener_mode: crate::HoverListenerMode,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub group: Option<SharedString>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hover: Option<StyleId>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub active: Option<StyleId>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub group_hover: Option<GroupRefinement>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub group_active: Option<GroupRefinement>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub on_click: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub on_aux_click: Option<u32>,
    /// The click consumes its press: the host stops the pointer's click at
    /// this node, so nothing under it hears the same press. It needs
    /// `on_click`, where the host stops it; without one the frame is refused.
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub consumes_click: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub on_mouse_down: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub capture_mouse_down: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub on_mouse_down_out: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub on_mouse_up: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub capture_mouse_up: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub on_mouse_up_out: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub on_mouse_pressure: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub capture_mouse_pressure: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub on_mouse_move: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub on_mouse_exit: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub on_scroll_wheel: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub on_pinch: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub capture_pinch: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub on_key_down: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub capture_key_down: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub on_key_up: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub capture_key_up: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub on_modifiers_changed: Option<u32>,
    /// Keystrokes in gpui's words (`"escape"`, `"shift-tab"`) this node
    /// takes, with its key-down listeners or its keyboard click: the host
    /// stops each one here once they have heard it.
    #[serde(
        skip_serializing_if = "Vec::is_empty",
        deserialize_with = "crate::interactivity::decode_consumed_keys"
    )]
    pub consumes_keys: Vec<SharedString>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub on_hover: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub on_file_drop_exit: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tooltip: Option<crate::Tooltip>,
}

crate::codec::sparse!(Interactivity);

/// A node's interactivity as it crosses: a node that declares none holds
/// none, and on the wire that is the sparse struct with no field in it,
/// which is what a node that sets no field writes too. So the reader hands
/// back `None` for both, and the bytes do not say which the writer held.
pub(crate) mod optional {
    use super::Interactivity;
    use serde::ser::SerializeStruct;
    use serde::{Deserializer, Serialize, Serializer};

    struct NoField;
    impl Serialize for NoField {
        fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
            serializer.serialize_struct("Interactivity", 0)?.end()
        }
    }

    pub fn serialize<S: Serializer>(
        value: &Option<Box<Interactivity>>,
        serializer: S,
    ) -> Result<S::Ok, S::Error> {
        match value {
            Some(interactivity) => interactivity.serialize(serializer),
            None => serializer.serialize_newtype_struct(crate::codec::SPARSE, &NoField),
        }
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(
        deserializer: D,
    ) -> Result<Option<Box<Interactivity>>, D::Error> {
        let read = <Interactivity as serde::Deserialize>::deserialize(deserializer)?;
        Ok((read != Interactivity::default()).then(|| Box::new(read)))
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct GroupRefinement {
    pub group: SharedString,
    pub style: StyleId,
}

impl Interactivity {
    /// What a node that declares no interactivity says: nothing, in every
    /// field. For a reader that takes a node's interactivity whether it
    /// holds one or not.
    pub fn none() -> &'static Self {
        static NONE: std::sync::LazyLock<Interactivity> =
            std::sync::LazyLock::new(Default::default);
        &NONE
    }

    /// The seven conditional styles, each where it is set: every style
    /// besides its own that a node names.
    pub(crate) fn style_slots(&mut self) -> impl Iterator<Item = &mut StyleId> {
        [
            &mut self.focus,
            &mut self.in_focus,
            &mut self.focus_visible,
            &mut self.hover,
            &mut self.active,
        ]
        .into_iter()
        .flatten()
        .chain(
            [&mut self.group_hover, &mut self.group_active]
                .into_iter()
                .flatten()
                .map(|group| &mut group.style),
        )
    }

    pub(crate) fn styles(&mut self) -> impl Iterator<Item = StyleId> {
        self.style_slots().map(|style| *style)
    }
}
