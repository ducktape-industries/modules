//! Bounds applied before a guest refinement reaches native layout or painting.
//!
//! The rule for this file: clamp magnitudes, refuse a shape it cannot keep,
//! and never silently delete a valid value. A number is held to a finite
//! range (a margin or an inset to `±MAX_PIXELS`, a size to
//! `0..=MAX_PIXELS`), a list or a string to a length; a background this
//! file cannot read back (a pattern) refuses the frame. A value inside its
//! bound passes unchanged, so a negative margin stays negative and a
//! gradient stays a gradient.
//!
//! The host clips the entire view slot (`render.rs`, the slot's
//! `overflow_hidden`): a negative margin or inset moves an element only
//! inside that clip, and local clipping cannot contain a deferred or
//! anchored element on its own.
use crate::{MAX_PIXELS, MAX_TEXT_PIXELS, truncate_to};
use gpui::{
    AbsoluteLength, Background, DefiniteLength, Fill, GridPlacement, Hsla, Length, StyleRefinement,
    TextStyleRefinement, px,
};

const MAX_REMS: f32 = 256.;
const MAX_GRID: u16 = 64;
const MAX_SHADOWS: usize = 4;

/// Clamps in place; a non-finite value becomes `min`.
pub(crate) fn clamp_finite(value: &mut f32, min: f32, max: f32) {
    *value = if value.is_finite() {
        value.clamp(min, max)
    } else {
        min
    };
}
/// Holds `value` to `0..=max`, or to `-max..=max` when it may point either
/// way (a margin, an inset); a signed NaN reads as 0.
fn bound(value: &mut f32, max: f32, signed: bool) {
    match signed {
        true if value.is_nan() => *value = 0.,
        true => *value = value.clamp(-max, max),
        false => clamp_finite(value, 0., max),
    }
}
fn absolute(value: &mut AbsoluteLength, max: f32, signed: bool) {
    match value {
        AbsoluteLength::Pixels(value) => {
            let mut number = f32::from(*value);
            bound(&mut number, max, signed);
            *value = px(number);
        }
        AbsoluteLength::Rems(value) => bound(&mut value.0, (max / 32.).min(MAX_REMS), signed),
    }
}
fn definite(value: &mut DefiniteLength, max: f32, signed: bool) {
    match value {
        DefiniteLength::Absolute(value) => absolute(value, max, signed),
        DefiniteLength::Fraction(value) => bound(value, 1., signed),
    }
}
fn length(value: &mut Length, signed: bool) {
    if let Length::Definite(value) = value {
        definite(value, MAX_PIXELS, signed);
    }
}
pub(crate) fn sanitize_hsla(value: &mut Hsla) {
    for number in [&mut value.h, &mut value.s, &mut value.l, &mut value.a] {
        clamp_finite(number, 0., 1.);
    }
}
/// A solid keeps its colour and a linear gradient its angle, stops and
/// colour space, each bounded; a pattern's payload has no public read, so
/// it is refused rather than forwarded unchecked to the shader.
fn background(value: Background) -> Result<Background, &'static str> {
    if let Some(mut color) = value.as_solid() {
        sanitize_hsla(&mut color);
        return Ok(color.into());
    }
    let Some((mut angle, mut stops, space)) = value.as_linear_gradient() else {
        return Err("a pattern background does not cross the view wire");
    };
    if !angle.is_finite() {
        angle = 0.;
    }
    for stop in &mut stops {
        sanitize_hsla(&mut stop.color);
        clamp_finite(&mut stop.percentage, 0., 1.);
    }
    Ok(gpui::linear_gradient(angle, stops[0], stops[1]).color_space(space))
}
fn grid(value: &mut GridPlacement) {
    match value {
        GridPlacement::Line(value) => *value = (*value).clamp(-(MAX_GRID as i16), MAX_GRID as i16),
        GridPlacement::Span(value) => *value = (*value).clamp(1, MAX_GRID),
        GridPlacement::Auto => {}
    }
}

/// Apply once to each base and conditional refinement during the tree's
/// existing sanitize walk. This pinned GPUI revision has no z-index field in
/// StyleRefinement. Deferred painting must be bounded separately by the
/// host's slot content mask.
pub(crate) fn sanitize(style: &mut StyleRefinement) -> Result<(), &'static str> {
    for value in [
        &mut style.size.width,
        &mut style.size.height,
        &mut style.min_size.width,
        &mut style.min_size.height,
        &mut style.max_size.width,
        &mut style.max_size.height,
    ]
    .into_iter()
    .flatten()
    {
        length(value, false);
    }
    for value in [
        &mut style.inset.top,
        &mut style.inset.right,
        &mut style.inset.bottom,
        &mut style.inset.left,
        &mut style.margin.top,
        &mut style.margin.right,
        &mut style.margin.bottom,
        &mut style.margin.left,
        &mut style.flex_basis,
    ]
    .into_iter()
    .flatten()
    {
        length(value, true);
    }
    for value in [
        &mut style.padding.top,
        &mut style.padding.right,
        &mut style.padding.bottom,
        &mut style.padding.left,
        &mut style.gap.width,
        &mut style.gap.height,
    ]
    .into_iter()
    .flatten()
    {
        definite(value, MAX_PIXELS, false);
    }
    for value in [
        &mut style.border_widths.top,
        &mut style.border_widths.right,
        &mut style.border_widths.bottom,
        &mut style.border_widths.left,
        &mut style.corner_radii.top_left,
        &mut style.corner_radii.top_right,
        &mut style.corner_radii.bottom_left,
        &mut style.corner_radii.bottom_right,
        &mut style.scrollbar_width,
    ]
    .into_iter()
    .flatten()
    {
        absolute(value, MAX_PIXELS, false);
    }
    for value in [&mut style.flex_grow, &mut style.flex_shrink]
        .into_iter()
        .flatten()
    {
        clamp_finite(value, 0., 1024.);
    }
    if let Some(value) = &mut style.aspect_ratio {
        clamp_finite(value, 1. / 1024., 1024.);
    }
    if let Some(value) = &mut style.opacity {
        clamp_finite(value, 0., 1.);
    }
    if let Some(value) = &mut style.border_color {
        sanitize_hsla(value);
    }
    if let Some(Fill::Color(value)) = &mut style.background {
        *value = background(*value)?;
    }
    if let Some(shadows) = &mut style.box_shadow {
        shadows.truncate(MAX_SHADOWS);
        for shadow in shadows {
            sanitize_hsla(&mut shadow.color);
            for offset in [&mut shadow.offset.x, &mut shadow.offset.y] {
                let mut number = f32::from(*offset);
                clamp_finite(&mut number, -128., 128.);
                *offset = px(number);
            }
            for radius in [&mut shadow.blur_radius, &mut shadow.spread_radius] {
                let mut number = f32::from(*radius);
                clamp_finite(&mut number, 0., 128.);
                *radius = px(number);
            }
        }
    }
    for template in [&mut style.grid_cols, &mut style.grid_rows]
        .into_iter()
        .flatten()
    {
        template.repeat = template.repeat.clamp(1, MAX_GRID);
    }
    if let Some(location) = &mut style.grid_location {
        for value in [
            &mut location.row.start,
            &mut location.row.end,
            &mut location.column.start,
            &mut location.column.end,
        ] {
            grid(value);
        }
    }
    sanitize_text(&mut style.text);
    #[cfg(debug_assertions)]
    {
        style.debug = None;
        style.debug_below = None;
    }
    Ok(())
}

/// A text style's own bounds: colours, sizes, font lookup lists, ellipsis.
pub(crate) fn sanitize_text(text: &mut TextStyleRefinement) {
    if let Some(value) = &mut text.color {
        sanitize_hsla(value);
    }
    if let Some(value) = &mut text.background_color {
        sanitize_hsla(value);
    }
    if let Some(value) = &mut text.font_size {
        absolute(value, MAX_TEXT_PIXELS, false);
    }
    if let Some(value) = &mut text.line_height {
        match value {
            DefiniteLength::Absolute(value) => absolute(value, MAX_TEXT_PIXELS, false),
            DefiniteLength::Fraction(value) => clamp_finite(value, 0., 8.),
        }
    }
    if let Some(value) = &mut text.font_weight {
        clamp_finite(&mut value.0, 1., 1000.);
    }
    if let Some(value) = &mut text.line_clamp {
        *value = (*value).clamp(1, 1024);
    }
    if let Some(value) = &mut text.font_family {
        let mut name = value.to_string();
        truncate_to(&mut name, 256);
        *value = name.into();
    }
    if let Some(fallbacks) = &mut text.font_fallbacks {
        let fonts = std::sync::Arc::make_mut(&mut fallbacks.0);
        fonts.truncate(16);
        for name in fonts {
            truncate_to(name, 256);
        }
    }
    if let Some(features) = &mut text.font_features {
        let features = std::sync::Arc::make_mut(&mut features.0);
        features.truncate(64);
        features.retain(|(tag, _)| tag.len() == 4 && tag.is_ascii());
    }
    if let Some(overflow) = &mut text.text_overflow {
        let (gpui::TextOverflow::Truncate(value)
        | gpui::TextOverflow::TruncateStart(value)
        | gpui::TextOverflow::TruncateMiddle(value)) = overflow;
        let mut string = value.to_string();
        truncate_to(&mut string, 32);
        *value = string.into();
    }
    if let Some(value) = &mut text.underline {
        let mut thickness = f32::from(value.thickness);
        clamp_finite(&mut thickness, 0., 32.);
        value.thickness = px(thickness);
        if let Some(value) = &mut value.color {
            sanitize_hsla(value);
        }
    }
    if let Some(value) = &mut text.strikethrough {
        let mut thickness = f32::from(value.thickness);
        clamp_finite(&mut thickness, 0., 32.);
        value.thickness = px(thickness);
        if let Some(value) = &mut value.color {
            sanitize_hsla(value);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use gpui::{Overflow, Position, Styled, relative, rems, rgb};

    #[test]
    fn negative_margins_and_insets_keep_their_sign_and_only_their_magnitude_is_cut() {
        let mut style = StyleRefinement::default()
            .absolute()
            .left(px(-50.))
            .m(px(-20.))
            .mt(rems(-1e6))
            .mr(relative(-3.))
            .flex_basis(px(-1e9));
        style.margin.bottom = Some(px(f32::NAN).into());
        sanitize(&mut style).unwrap();
        assert_eq!(style.position, Some(Position::Absolute));
        assert_eq!(style.inset.left, Some(px(-50.).into()));
        assert_eq!(style.margin.left, Some(px(-20.).into()));
        assert_eq!(style.margin.top, Some(rems(-MAX_REMS).into()));
        assert_eq!(style.margin.right, Some(relative(-1.).into()));
        assert_eq!(style.margin.bottom, Some(px(0.).into()));
        assert_eq!(style.flex_basis, Some(px(-MAX_PIXELS).into()));
        // a size may not point backwards
        let mut size = StyleRefinement::default().w(px(-5.));
        sanitize(&mut size).unwrap();
        assert_eq!(size.size.width, Some(px(0.).into()));
    }
    #[test]
    fn bounds_pixels_rems_fractions_and_nonfinite_dimensions() {
        let mut style = StyleRefinement::default()
            .w(px(f32::INFINITY))
            .h(rems(10000.))
            .min_w(relative(1000.));
        style.max_size.height = Some(px(1e20).into());
        sanitize(&mut style).unwrap();
        assert_eq!(style.size.width, Some(px(0.).into()));
        assert_eq!(style.size.height, Some(rems(MAX_REMS).into()));
        assert_eq!(style.min_size.width, Some(relative(1.).into()));
        assert_eq!(style.max_size.height, Some(px(MAX_PIXELS).into()));
    }
    #[test]
    fn opacity_and_color_are_finite_and_bounded() {
        let mut style = StyleRefinement::default().opacity(f32::NAN);
        style.border_color = Some(Hsla {
            h: -1.,
            s: 2.,
            l: f32::INFINITY,
            a: 9.,
        });
        sanitize(&mut style).unwrap();
        assert_eq!(style.opacity, Some(0.));
        assert_eq!(
            style.border_color,
            Some(Hsla {
                h: 0.,
                s: 1.,
                l: 0.,
                a: 1.
            })
        );
    }
    #[test]
    fn visible_overflow_is_local_and_requires_host_slot_clip() {
        let mut style = StyleRefinement::default();
        style.overflow.x = Some(Overflow::Visible);
        sanitize(&mut style).unwrap();
        assert_eq!(style.overflow.x, Some(Overflow::Visible));
    }
    #[test]
    fn bounds_shadow_count_and_gpu_radius() {
        let mut style = StyleRefinement {
            box_shadow: Some(vec![
                gpui::BoxShadow {
                    color: rgb(0).into(),
                    offset: gpui::point(px(f32::NAN), px(1000.)),
                    blur_radius: px(1e9),
                    spread_radius: px(-100.),
                    inset: false,
                };
                100
            ]),
            ..Default::default()
        };
        sanitize(&mut style).unwrap();
        let shadows = style.box_shadow.unwrap();
        assert_eq!(shadows.len(), MAX_SHADOWS);
        assert_eq!(shadows[0].blur_radius, px(128.));
        assert_eq!(shadows[0].offset.x, px(-128.));
        assert_eq!(shadows[0].spread_radius, px(0.));
    }
    #[test]
    fn caps_grid_expansion_and_font_rasterization() {
        let mut style = StyleRefinement::default().text_size(px(1e9));
        style.grid_cols = Some(gpui::GridTemplate {
            repeat: u16::MAX,
            ..Default::default()
        });
        style.grid_location = Some(gpui::GridLocation {
            row: GridPlacement::Span(u16::MAX)..GridPlacement::Line(i16::MIN),
            column: GridPlacement::Auto..GridPlacement::Auto,
        });
        sanitize(&mut style).unwrap();
        assert_eq!(style.text.font_size, Some(px(MAX_TEXT_PIXELS).into()));
        assert_eq!(style.grid_cols.unwrap().repeat, MAX_GRID);
        assert_eq!(
            style.grid_location.unwrap().row.start,
            GridPlacement::Span(MAX_GRID)
        );
    }
    #[test]
    fn preserves_normal_line_spacing_and_is_idempotent() {
        let mut style = StyleRefinement::default()
            .text_size(px(16.))
            .line_height(relative(1.5));
        sanitize(&mut style).unwrap();
        assert_eq!(style.text.line_height, Some(relative(1.5)));
        let once = style.clone();
        sanitize(&mut style).unwrap();
        assert_eq!(style, once);
    }
    #[test]
    fn bounds_font_lookup_lists_and_custom_ellipsis_at_utf8_boundaries() {
        let mut style = StyleRefinement::default().font_family("λ".repeat(200));
        style.text.font_fallbacks =
            Some(gpui::FontFallbacks::from_fonts(vec!["λ".repeat(200); 30]));
        style.text.font_features = Some(gpui::FontFeatures(std::sync::Arc::new(vec![
            (
                "liga".into(),
                1
            );
            100
        ])));
        style.text.text_overflow = Some(gpui::TextOverflow::Truncate("λ".repeat(100).into()));
        sanitize(&mut style).unwrap();
        assert_eq!(style.text.font_family.unwrap().len(), 256);
        let fonts = style.text.font_fallbacks.unwrap();
        assert_eq!(fonts.fallback_list().len(), 16);
        assert!(fonts.fallback_list().iter().all(|name| name.len() == 256));
        assert_eq!(style.text.font_features.unwrap().tag_value_list().len(), 64);
        assert_eq!(
            style.text.text_overflow,
            Some(gpui::TextOverflow::Truncate("λ".repeat(16).into()))
        );
    }

    #[test]
    fn a_gradient_keeps_its_stops_bounded_and_a_pattern_is_refused() {
        let gradient = gpui::linear_gradient(
            30.,
            gpui::linear_color_stop(rgb(0xff0000), 0.25),
            gpui::linear_color_stop(rgb(0x0000ff), 1.),
        )
        .color_space(gpui::ColorSpace::Oklab);
        let mut style = StyleRefinement::default().bg(gradient);
        sanitize(&mut style).unwrap();
        assert_eq!(style.background, Some(gradient.into()));
        let hostile = gpui::linear_gradient(
            f32::NAN,
            gpui::linear_color_stop(
                Hsla {
                    h: 2.,
                    s: -1.,
                    l: 0.5,
                    a: f32::INFINITY,
                },
                -4.,
            ),
            gpui::linear_color_stop(rgb(0x0000ff), f32::NAN),
        );
        let mut style = StyleRefinement::default().bg(hostile);
        sanitize(&mut style).unwrap();
        let Some(Fill::Color(kept)) = style.background else {
            panic!("the gradient is kept")
        };
        let (angle, [from, to], _) = kept.as_linear_gradient().expect("still a gradient");
        assert_eq!(angle, 0.);
        assert_eq!(
            (from.color, from.percentage, to.percentage),
            (
                Hsla {
                    h: 1.,
                    s: 0.,
                    l: 0.5,
                    a: 0.
                },
                0.,
                0.
            )
        );
        let mut solid = StyleRefinement::default().bg(rgb(0x123456));
        let before = solid.background.clone();
        sanitize(&mut solid).unwrap();
        assert_eq!(solid.background, before);
        let mut pattern = StyleRefinement::default().bg(gpui::pattern_slash(rgb(0), 1., 4.));
        assert_eq!(
            sanitize(&mut pattern),
            Err("a pattern background does not cross the view wire")
        );
    }
}
