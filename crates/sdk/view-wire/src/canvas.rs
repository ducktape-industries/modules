//! Declarative geometry. Commands contain copied values, never host callbacks.
use crate::style_sanitize::{clamp_finite, sanitize_hsla};
use gpui::Hsla;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum CanvasCommand {
    Draw {
        shape: CanvasShape,
        fill: Option<Hsla>,
        even_odd: bool,
        stroke: Option<CanvasStroke>,
    },
    Push {
        translate: [f32; 2],
        rotate: f32,
        scale: [f32; 2],
        clip: Option<[f32; 4]>,
    },
    Pop,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum CanvasShape {
    Rectangle {
        position: [f32; 2],
        size: [f32; 2],
        radius: [f32; 4],
    },
    Circle {
        center: [f32; 2],
        radius: f32,
    },
    Line {
        from: [f32; 2],
        to: [f32; 2],
    },
    Path(Vec<CanvasSegment>),
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum CanvasSegment {
    Rectangle {
        position: [f32; 2],
        size: [f32; 2],
        radius: [f32; 4],
    },
    Circle {
        center: [f32; 2],
        radius: f32,
    },
    Move([f32; 2]),
    Line([f32; 2]),
    Arc {
        center: [f32; 2],
        radius: f32,
        start: f32,
        end: f32,
    },
    ArcTo {
        a: [f32; 2],
        b: [f32; 2],
        radius: f32,
    },
    Ellipse {
        center: [f32; 2],
        radius: [f32; 2],
        rotation: f32,
        start: f32,
        end: f32,
    },
    Bezier {
        a: [f32; 2],
        b: [f32; 2],
        end: [f32; 2],
    },
    Quadratic {
        control: [f32; 2],
        end: [f32; 2],
    },
    Close,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct CanvasStroke {
    pub color: Hsla,
    pub width: f32,
    pub cap: CanvasLineCap,
    pub join: CanvasLineJoin,
    pub dash: Vec<f32>,
    pub dash_offset: u32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum CanvasLineCap {
    Butt,
    Square,
    Round,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum CanvasLineJoin {
    Miter,
    Round,
    Bevel,
}

pub(super) fn sanitize(commands: &mut Vec<CanvasCommand>) {
    let mut open = 0usize;
    commands.retain_mut(|command| {
        match command {
            CanvasCommand::Push {
                translate,
                rotate,
                scale,
                clip,
            } => {
                point(translate);
                angle(rotate);
                scale.iter_mut().for_each(size);
                open += 1;
                if let Some([x, y, width, height]) = clip {
                    coordinate(x);
                    coordinate(y);
                    size(width);
                    size(height);
                }
            }
            CanvasCommand::Pop => {
                if open == 0 {
                    return false;
                }
                open -= 1;
            }
            CanvasCommand::Draw {
                shape,
                fill,
                stroke,
                ..
            } => {
                if let Some(color) = fill {
                    sanitize_hsla(color);
                }
                if let Some(stroke) = stroke {
                    sanitize_hsla(&mut stroke.color);
                    size(&mut stroke.width);
                    for value in &mut stroke.dash {
                        *value = finite_or_zero(*value).max(0.01);
                    }
                    if !stroke.dash.is_empty() {
                        stroke.dash_offset %= stroke.dash.len() as u32;
                    }
                }
                match shape {
                    CanvasShape::Rectangle {
                        position,
                        size: dimensions,
                        radius,
                    } => {
                        point(position);
                        dimensions.iter_mut().for_each(size);
                        radius.iter_mut().for_each(size);
                    }
                    CanvasShape::Circle { center, radius } => {
                        point(center);
                        size(radius);
                    }
                    CanvasShape::Line { from, to } => {
                        point(from);
                        point(to);
                    }
                    CanvasShape::Path(segments) => {
                        for segment in segments {
                            match segment {
                                CanvasSegment::Rectangle {
                                    position,
                                    size: dimensions,
                                    radius,
                                } => {
                                    point(position);
                                    dimensions.iter_mut().for_each(size);
                                    radius.iter_mut().for_each(size);
                                }
                                CanvasSegment::Circle { center, radius } => {
                                    point(center);
                                    size(radius);
                                }
                                CanvasSegment::Move(p) | CanvasSegment::Line(p) => point(p),
                                CanvasSegment::Arc {
                                    center,
                                    radius,
                                    start,
                                    end,
                                } => {
                                    point(center);
                                    size(radius);
                                    angle(start);
                                    angle(end);
                                }
                                CanvasSegment::ArcTo { a, b, radius } => {
                                    point(a);
                                    point(b);
                                    size(radius);
                                }
                                CanvasSegment::Ellipse {
                                    center,
                                    radius,
                                    rotation,
                                    start,
                                    end,
                                } => {
                                    point(center);
                                    radius.iter_mut().for_each(size);
                                    angle(rotation);
                                    angle(start);
                                    angle(end);
                                }
                                CanvasSegment::Bezier { a, b, end } => {
                                    point(a);
                                    point(b);
                                    point(end);
                                }
                                CanvasSegment::Quadratic { control, end } => {
                                    point(control);
                                    point(end);
                                }
                                CanvasSegment::Close => {}
                            }
                        }
                    }
                }
            }
        }
        true
    });
}
fn finite_or_zero(value: f32) -> f32 {
    if value.is_finite() { value } else { 0.0 }
}
fn coordinate(value: &mut f32) {
    *value = finite_or_zero(*value);
}
fn size(value: &mut f32) {
    clamp_finite(value, 0.0, f32::MAX);
}
fn angle(value: &mut f32) {
    *value = finite_or_zero(*value);
}
fn point(value: &mut [f32; 2]) {
    value.iter_mut().for_each(coordinate);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn path(parts: usize) -> CanvasCommand {
        CanvasCommand::Draw {
            shape: CanvasShape::Path(vec![CanvasSegment::Close; parts]),
            fill: Some(gpui::rgb(0x00ffff).into()),
            even_odd: false,
            stroke: None,
        }
    }

    #[test]
    fn combined_native_scale_keeps_sub_epsilon_products() {
        let combined = f32::EPSILON * f32::EPSILON;
        let mut commands = vec![
            CanvasCommand::Push {
                translate: [0.0; 2],
                rotate: 0.0,
                scale: [combined; 2],
                clip: None,
            },
            CanvasCommand::Pop,
        ];
        let original = commands.clone();
        sanitize(&mut commands);
        assert_eq!(
            commands, original,
            "a combined native transform must not be enlarged"
        );
    }

    #[test]
    fn sanitizer_makes_numbers_finite() {
        let mut commands = vec![
            CanvasCommand::Draw {
                shape: CanvasShape::Circle {
                    center: [f32::NAN, f32::MAX],
                    radius: -1.0,
                },
                fill: Some(Hsla {
                    h: f32::INFINITY,
                    s: -1.,
                    l: 2.,
                    a: 1.,
                }),
                even_odd: false,
                stroke: None,
            },
            path(10),
        ];
        sanitize(&mut commands);
        assert_eq!(
            commands,
            vec![
                CanvasCommand::Draw {
                    shape: CanvasShape::Circle {
                        center: [0.0, f32::MAX],
                        radius: 0.0
                    },
                    fill: Some(Hsla {
                        h: 0.,
                        s: 0.,
                        l: 1.,
                        a: 1.
                    }),
                    even_odd: false,
                    stroke: None,
                },
                path(10),
            ]
        );
    }

    #[test]
    fn sanitizer_drops_unmatched_pops() {
        let push = CanvasCommand::Push {
            translate: [0.0; 2],
            rotate: 0.0,
            scale: [8192.0; 2],
            clip: None,
        };
        let mut commands = vec![CanvasCommand::Pop];
        commands.extend(vec![push; 40]);
        commands.push(path(1));
        commands.extend(vec![CanvasCommand::Pop; 41]);
        commands.push(path(1));
        sanitize(&mut commands);
        let mut depth = 0;
        for command in &commands {
            match command {
                CanvasCommand::Push { .. } => depth += 1,
                CanvasCommand::Pop => {
                    assert!(depth > 0);
                    depth -= 1;
                }
                CanvasCommand::Draw { .. } => {}
            }
        }
        assert_eq!(depth, 0);
        assert_eq!(commands.len(), 82);
        let mut again = commands.clone();
        sanitize(&mut again);
        assert_eq!(commands, again);
    }
}
