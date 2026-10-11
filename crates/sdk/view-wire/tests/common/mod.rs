//! The table of a test whose nodes' styles are not what it is about: one
//! empty style, which every such node names as [`PLAIN`].
#![allow(dead_code)]
use view_wire::{Frame, Refused, SanitizeReport, Style, StyleId, Styles};

pub const PLAIN: StyleId = StyleId(0);

pub fn plain() -> Vec<Style> {
    vec![Style::new(&Default::default())]
}

pub fn held() -> Styles {
    let mut styles = Styles::default();
    styles.extend(plain()).unwrap();
    styles
}

/// A table of the plain style and then `styles`: the first of them is
/// `StyleId(1)`.
pub fn table(styles: &[gpui::StyleRefinement]) -> Vec<Style> {
    plain()
        .into_iter()
        .chain(styles.iter().map(Style::new))
        .collect()
}

/// `view_wire::sanitize`, for a frame that did not bring a table of its
/// own: its nodes name [`PLAIN`].
pub fn sanitize(frame: &mut Frame) -> Result<SanitizeReport, Refused> {
    if frame.styles.is_empty() {
        frame.styles = plain();
    }
    view_wire::sanitize(frame, &mut Styles::default())
}
