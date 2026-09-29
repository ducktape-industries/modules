//! Every unit variant of each enum whose unit variants the samples in place
//! do not spell out: `frame.bin`'s fourth value. A unit variant is its name
//! on the wire, and a serialize-side rename of one no sample writes moves
//! no byte and no line of `schema.txt` (traced from the `Deserialize`
//! side). Each field is filled off the registry, so a unit variant is
//! sampled the moment it is declared. What this does not reach: a
//! field-level `serialize_with` on a field holding one of these enums,
//! which only that field's own sample in place runs.
use serde::{Deserialize, Serialize};
use serde_json::Value as Json;
use serde_reflection::{ContainerFormat, Registry, VariantFormat};

use super::*;

/// One field per enum, named as the registry names it.
#[derive(Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct Units {
    action: Vec<Action>,
    align_content: Vec<gpui::AlignContent>,
    align_items: Vec<gpui::AlignItems>,
    anchor: Vec<Anchor>,
    anchored_fit_mode: Vec<AnchoredFitMode>,
    anchored_position_mode: Vec<AnchoredPositionMode>,
    aria_current: Vec<AriaCurrent>,
    border_style: Vec<gpui::BorderStyle>,
    canvas_line_cap: Vec<CanvasLineCap>,
    canvas_line_join: Vec<CanvasLineJoin>,
    color_space: Vec<gpui::ColorSpace>,
    cursor: Vec<mouse::Cursor>,
    cursor_style: Vec<gpui::CursorStyle>,
    display: Vec<gpui::Display>,
    editor_decision: Vec<EditorDecision>,
    editor_edit_kind: Vec<EditorEditKind>,
    editor_history_effect: Vec<EditorHistoryEffect>,
    editor_transfer_error: Vec<EditorTransferError>,
    flex_direction: Vec<gpui::FlexDirection>,
    flex_wrap: Vec<gpui::FlexWrap>,
    font_style: Vec<gpui::FontStyle>,
    grid_template_min_size: Vec<gpui::GridTemplateMinSize>,
    has_popup: Vec<HasPopup>,
    hover_listener_mode: Vec<interactivity::HoverListenerMode>,
    image_object_fit: Vec<ImageObjectFit>,
    invalid: Vec<Invalid>,
    key: Vec<keyboard::Key>,
    keyboard_button: Vec<click::KeyboardButton>,
    list_alignment: Vec<ListAlignment>,
    list_command: Vec<ListCommand>,
    list_sizing_behavior: Vec<ListSizingBehavior>,
    live: Vec<Live>,
    location: Vec<keyboard::Location>,
    mouse_button: Vec<click::MouseButton>,
    named: Vec<keyboard::Named>,
    native_code: Vec<keyboard::NativeCode>,
    overflow: Vec<gpui::Overflow>,
    position: Vec<gpui::Position>,
    pressure_stage: Vec<interactivity::PressureStage>,
    role: Vec<accesskit::Role>,
    scroll_hint: Vec<accesskit::ScrollHint>,
    scroll_unit: Vec<accesskit::ScrollUnit>,
    svg_source: Vec<SvgSource>,
    text_align: Vec<gpui::TextAlign>,
    toggled: Vec<accesskit::Toggled>,
    touch_phase: Vec<interactivity::TouchPhase>,
    uniform_list_horizontal_sizing: Vec<UniformListHorizontalSizing>,
    uniform_list_scroll_strategy: Vec<UniformListScrollStrategy>,
    uniform_list_sizing: Vec<UniformListSizing>,
    visibility: Vec<gpui::Visibility>,
    white_space: Vec<gpui::WhiteSpace>,
}

/// [`Units`], each field every unit variant `registry` declares for the enum
/// of its name, decoded through that enum's own `Deserialize`. A field no
/// enum answers to fails by name (`missing field`).
pub fn every_unit(registry: &Registry) -> Units {
    let units = registry
        .iter()
        .filter_map(|(name, container)| {
            let ContainerFormat::Enum(variants) = container else {
                return None;
            };
            let units = variants
                .values()
                .filter(|variant| matches!(variant.value, VariantFormat::Unit))
                .map(|variant| Json::String(variant.name.clone()))
                .collect();
            Some((name.clone(), Json::Array(units)))
        })
        .collect();
    serde_json::from_value(Json::Object(units)).unwrap()
}
