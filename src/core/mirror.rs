//! 反転（ミラーリング）処理

use serde::{Deserialize, Serialize};

use crate::core::geometry::{
    ArcSegment, CircleShape, DrillData, DrillHole, DrillSlot, Geometry, Point2, PolygonShape,
    Segment,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum MirrorAxis {
    None,
    X, // X軸を対称軸（Y座標を反転: y' = 2*ref - y）
    Y, // Y軸を対称軸（X座標を反転: x' = 2*ref - x）
}

impl Default for MirrorAxis {
    fn default() -> Self {
        Self::None
    }
}

fn mirror_point(p: Point2, axis: MirrorAxis, reference: f64) -> Point2 {
    match axis {
        MirrorAxis::None => p,
        MirrorAxis::X => Point2::new(p.x, 2.0 * reference - p.y),
        MirrorAxis::Y => Point2::new(2.0 * reference - p.x, p.y),
    }
}

pub fn mirror_geometry(geo: &Geometry, axis: MirrorAxis, reference: f64) -> Geometry {
    if axis == MirrorAxis::None {
        return geo.clone();
    }

    let segments = geo
        .segments
        .iter()
        .map(|s| Segment {
            start: mirror_point(s.start, axis, reference),
            end: mirror_point(s.end, axis, reference),
            width: s.width,
            polarity: s.polarity,
        })
        .collect();

    let arcs = geo
        .arcs
        .iter()
        .map(|a| ArcSegment {
            start: mirror_point(a.start, axis, reference),
            end: mirror_point(a.end, axis, reference),
            center: mirror_point(a.center, axis, reference),
            clockwise: !a.clockwise,
            width: a.width,
            polarity: a.polarity,
        })
        .collect();

    let circles = geo
        .circles
        .iter()
        .map(|c| CircleShape {
            center: mirror_point(c.center, axis, reference),
            radius: c.radius,
            polarity: c.polarity,
        })
        .collect();

    let polygons = geo
        .polygons
        .iter()
        .map(|p| PolygonShape {
            points: p
                .points
                .iter()
                .map(|pt| mirror_point(*pt, axis, reference))
                .collect(),
            polarity: p.polarity,
        })
        .collect();

    Geometry {
        segments,
        arcs,
        circles,
        polygons,
    }
}

pub fn mirror_drill_data(drill: &DrillData, axis: MirrorAxis, reference: f64) -> DrillData {
    if axis == MirrorAxis::None {
        return drill.clone();
    }

    let holes = drill
        .holes
        .iter()
        .map(|h| DrillHole {
            position: mirror_point(h.position, axis, reference),
            diameter: h.diameter,
        })
        .collect();

    let slots = drill
        .slots
        .iter()
        .map(|s| DrillSlot {
            start: mirror_point(s.start, axis, reference),
            end: mirror_point(s.end, axis, reference),
            diameter: s.diameter,
        })
        .collect();

    DrillData { holes, slots }
}
