pub(crate) fn bounded(value: f32) -> f32 {
    match value.is_nan() {
        true => 0.0,
        false => value.clamp(0.0, f32::MAX),
    }
}

/// A number that is not a size (a list's overdraw, a picture's transform):
/// it is made finite and nothing more. The host clamps it into the range
/// it lays out.
pub(crate) fn finite(value: f32) -> f32 {
    match value.is_nan() {
        true => 0.0,
        false => value.clamp(f32::MIN, f32::MAX),
    }
}
