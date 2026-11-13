//! 幾何データ構造の定義

use serde::{Deserialize, Serialize};

/// 2次元座標点
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Point2 {
    pub x: f64,
    pub y: f64,
}

impl Point2 {
    pub fn new(x: f64, y: f64) -> Self {
        Self { x, y }
    }

    pub fn distance_to(&self, other: &Point2) -> f64 {
        let dx = other.x - self.x;
        let dy = other.y - self.y;
        (dx * dx + dy * dy).sqrt()
    }
}

/// ガーバー極性（Dark: 銅箔あり, Clear: 銅箔抜き）
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Polarity {
    Dark,
    Clear,
}

impl Default for Polarity {
    fn default() -> Self {
        Self::Dark
    }
}

/// 幅を持つ線分
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Segment {
    pub start: Point2,
    pub end: Point2,
    pub width: f64,
    pub polarity: Polarity,
}

/// 幅を持つ円弧
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ArcSegment {
    pub start: Point2,
    pub end: Point2,
    pub center: Point2,
    pub clockwise: bool,
    pub width: f64,
    pub polarity: Polarity,
}

/// 塗りつぶし円（フラッシュパッドなど）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CircleShape {
    pub center: Point2,
    pub radius: f64,
    pub polarity: Polarity,
}

/// 塗りつぶし多角形（ベタ塗りリージョン、矩形パッド、マクロパッドなど）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PolygonShape {
    pub points: Vec<Point2>,
    pub polarity: Polarity,
}

/// バウンディングボックス
#[derive(Debug, Clone, Copy)]
pub struct Bounds {
    pub min_x: f64,
    pub min_y: f64,
    pub max_x: f64,
    pub max_y: f64,
}

impl Bounds {
    pub fn empty() -> Self {
        Self {
            min_x: f64::INFINITY,
            min_y: f64::INFINITY,
            max_x: f64::NEG_INFINITY,
            max_y: f64::NEG_INFINITY,
        }
    }

    pub fn is_valid(&self) -> bool {
        self.min_x.is_finite()
            && self.min_y.is_finite()
            && self.max_x.is_finite()
            && self.max_y.is_finite()
    }

    pub fn expand_point(&mut self, p: Point2) {
        self.min_x = self.min_x.min(p.x);
        self.min_y = self.min_y.min(p.y);
        self.max_x = self.max_x.max(p.x);
        self.max_y = self.max_y.max(p.y);
    }

    pub fn expand_circle(&mut self, c: Point2, r: f64) {
        self.min_x = self.min_x.min(c.x - r);
        self.min_y = self.min_y.min(c.y - r);
        self.max_x = self.max_x.max(c.x + r);
        self.max_y = self.max_y.max(c.y + r);
    }

    pub fn merge(&mut self, other: &Bounds) {
        if other.is_valid() {
            self.min_x = self.min_x.min(other.min_x);
            self.min_y = self.min_y.min(other.min_y);
            self.max_x = self.max_x.max(other.max_x);
            self.max_y = self.max_y.max(other.max_y);
        }
    }

    pub fn width(&self) -> f64 {
        if self.is_valid() {
            self.max_x - self.min_x
        } else {
            0.0
        }
    }

    pub fn height(&self) -> f64 {
        if self.is_valid() {
            self.max_y - self.min_y
        } else {
            0.0
        }
    }

    pub fn center(&self) -> Point2 {
        if self.is_valid() {
            Point2::new((self.min_x + self.max_x) * 0.5, (self.min_y + self.max_y) * 0.5)
        } else {
            Point2::new(0.0, 0.0)
        }
    }
}

/// ガーバー幾何データの集合
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Geometry {
    pub segments: Vec<Segment>,
    pub arcs: Vec<ArcSegment>,
    pub circles: Vec<CircleShape>,
    pub polygons: Vec<PolygonShape>,
}

impl Geometry {
    pub fn new() -> Self {
        Self::default()
    }

    #[allow(dead_code)]
    pub fn is_empty(&self) -> bool {
        self.segments.is_empty()
            && self.arcs.is_empty()
            && self.circles.is_empty()
            && self.polygons.is_empty()
    }

    /// 全体のバウンディングボックスを取得
    pub fn bounds(&self) -> Bounds {
        let mut b = Bounds::empty();
        for s in &self.segments {
            let r = s.width * 0.5;
            b.expand_circle(s.start, r);
            b.expand_circle(s.end, r);
        }
        for a in &self.arcs {
            let r = a.width * 0.5;
            b.expand_circle(a.start, r);
            b.expand_circle(a.end, r);
            // 円弧の正確な最大最小も考慮（中心±半径）
            let arc_r = a.center.distance_to(&a.start);
            b.expand_circle(a.center, arc_r + r);
        }
        for c in &self.circles {
            b.expand_circle(c.center, c.radius);
        }
        for p in &self.polygons {
            for pt in &p.points {
                b.expand_point(*pt);
            }
        }
        b
    }

    /// 全要素を指定オフセットだけ平行移動する
    pub fn translate(&mut self, dx: f64, dy: f64) {
        for s in &mut self.segments {
            s.start.x += dx;
            s.start.y += dy;
            s.end.x += dx;
            s.end.y += dy;
        }
        for a in &mut self.arcs {
            a.start.x += dx;
            a.start.y += dy;
            a.end.x += dx;
            a.end.y += dy;
            a.center.x += dx;
            a.center.y += dy;
        }
        for c in &mut self.circles {
            c.center.x += dx;
            c.center.y += dy;
        }
        for p in &mut self.polygons {
            for pt in &mut p.points {
                pt.x += dx;
                pt.y += dy;
            }
        }
    }

    pub fn summary(&self) -> String {
        format!(
            "線分: {}, 円弧: {}, 円: {}, ポリゴン: {}",
            self.segments.len(),
            self.arcs.len(),
            self.circles.len(),
            self.polygons.len()
        )
    }
}

/// ドリル穴
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DrillHole {
    pub position: Point2,
    pub diameter: f64,
}

/// 長穴（スロット）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DrillSlot {
    pub start: Point2,
    pub end: Point2,
    pub diameter: f64,
}

/// ドリルデータ全体
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct DrillData {
    pub holes: Vec<DrillHole>,
    pub slots: Vec<DrillSlot>,
}

impl DrillData {
    pub fn new() -> Self {
        Self::default()
    }

    #[allow(dead_code)]
    pub fn is_empty(&self) -> bool {
        self.holes.is_empty() && self.slots.is_empty()
    }

    pub fn bounds(&self) -> Bounds {
        let mut b = Bounds::empty();
        for h in &self.holes {
            b.expand_circle(h.position, h.diameter * 0.5);
        }
        for s in &self.slots {
            let r = s.diameter * 0.5;
            b.expand_circle(s.start, r);
            b.expand_circle(s.end, r);
        }
        b
    }

    pub fn translate(&mut self, dx: f64, dy: f64) {
        for h in &mut self.holes {
            h.position.x += dx;
            h.position.y += dy;
        }
        for s in &mut self.slots {
            s.start.x += dx;
            s.start.y += dy;
            s.end.x += dx;
            s.end.y += dy;
        }
    }

    pub fn summary(&self) -> String {
        if self.slots.is_empty() {
            format!("穴数: {}", self.holes.len())
        } else {
            format!("穴数: {}, 長穴: {}", self.holes.len(), self.slots.len())
        }
    }
}

/// ツールパス（連続した点の列、閉ループまたは開パス）
pub type Toolpath = Vec<Point2>;
