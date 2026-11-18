//! ツールパス生成および最適化エンジン (i_overlay ベース)

use std::f64::consts::PI;

use i_overlay::core::fill_rule::FillRule;
use i_overlay::core::overlay_rule::OverlayRule;
use i_overlay::float::single::SingleFloatOverlay;
use i_overlay::mesh::float::outline::offset::OutlineOffset;
use i_overlay::mesh::float::stroke::offset::StrokeOffset;
use i_overlay::mesh::float::style::{LineCap, LineJoin, OutlineStyle, StrokeStyle};

use crate::config::{EdgeCutMode, EdgeGapMode};
use crate::core::geometry::{DrillData, DrillHole, Geometry, Point2, Polarity, Toolpath};
use crate::core::gerber::GerberParser;

type ShapesF64 = Vec<Vec<Vec<[f64; 2]>>>;

/// 輪郭の符号付き面積を計算する（正ならCCW、負ならCW）
fn contour_signed_area(pts: &[[f64; 2]]) -> f64 {
    if pts.len() < 3 {
        return 0.0;
    }
    let mut area = 0.0;
    for i in 0..pts.len() {
        let j = (i + 1) % pts.len();
        area += pts[i][0] * pts[j][1] - pts[j][0] * pts[i][1];
    }
    area * 0.5
}

/// 輪郭を必ず反時計回り(CCW)に正規化する
fn ensure_ccw(mut pts: Vec<[f64; 2]>) -> Vec<[f64; 2]> {
    if contour_signed_area(&pts) < 0.0 {
        pts.reverse();
    }
    pts
}

/// DRC（デザインルール / 加工可能性チェック）の検出箇所
#[derive(Debug, Clone)]
pub struct DrcViolation {
    pub position: Point2,
    pub diameter: f64,
    pub description: String,
}

/// DRC 検査結果サマリー
#[derive(Debug, Clone, Default)]
pub struct DrcReport {
    pub min_trace_width_mm: Option<f64>,
    pub min_clearance_mm: Option<f64>,
    pub uncut_bridges: Vec<DrcViolation>,
    pub narrow_traces: Vec<DrcViolation>,
}

impl DrcReport {
    /// 表示用の全警告リスト（未絶縁ブリッジと細線警告）を返す
    pub fn all_violations(&self) -> Vec<DrcViolation> {
        let mut all = self.uncut_bridges.clone();
        all.extend(self.narrow_traces.clone());
        all
    }
}

pub struct ToolpathGenerator {
    pub tool_diameter: f64,
    pub arc_resolution: f64,
    pub simplify_tolerance: f64,
}

impl Default for ToolpathGenerator {
    fn default() -> Self {
        Self {
            tool_diameter: 0.1,
            arc_resolution: 0.05,
            simplify_tolerance: 0.005,
        }
    }
}

impl ToolpathGenerator {
    pub fn new(tool_diameter: f64) -> Self {
        Self {
            tool_diameter,
            ..Default::default()
        }
    }

    /// ガーバー幾何データから統一銅箔領域（Dark結合 - Clear除外）を構築する
    pub fn build_unified_copper_shapes(&self, geometry: &Geometry) -> ShapesF64 {
        let mut dark_shapes: ShapesF64 = Vec::new();
        let mut clear_shapes: ShapesF64 = Vec::new();

        // 1. 線分を太さを持ったポリゴンにする
        for seg in &geometry.segments {
            let w = if seg.width > 1e-4 { seg.width } else { 0.01 };
            let path = [[seg.start.x, seg.start.y], [seg.end.x, seg.end.y]];
            let style = StrokeStyle::new(w)
                .start_cap(LineCap::Round(0.25))
                .end_cap(LineCap::Round(0.25))
                .line_join(LineJoin::Round(0.25));
            let stroked = path.stroke_as::<i64>(style, false);
            for mut shape in stroked {
                if let Some(outer) = shape.first_mut() {
                    *outer = ensure_ccw(std::mem::take(outer));
                }
                match seg.polarity {
                    Polarity::Dark => dark_shapes.push(shape),
                    Polarity::Clear => clear_shapes.push(shape),
                }
            }
        }

        // 2. 円弧を太さを持ったポリゴンにする
        for arc in &geometry.arcs {
            let pts = GerberParser::sample_arc_points(
                arc.start,
                arc.end,
                arc.center,
                arc.clockwise,
                self.arc_resolution,
            );
            if pts.len() >= 2 {
                let coords: Vec<[f64; 2]> = pts.iter().map(|p| [p.x, p.y]).collect();
                let w = if arc.width > 1e-4 { arc.width } else { 0.01 };
                let style = StrokeStyle::new(w)
                    .start_cap(LineCap::Round(0.25))
                    .end_cap(LineCap::Round(0.25))
                    .line_join(LineJoin::Round(0.25));
                let stroked = coords.stroke_as::<i64>(style, false);
                for mut shape in stroked {
                    if let Some(outer) = shape.first_mut() {
                        *outer = ensure_ccw(std::mem::take(outer));
                    }
                    match arc.polarity {
                        Polarity::Dark => dark_shapes.push(shape),
                        Polarity::Clear => clear_shapes.push(shape),
                    }
                }
            }
        }

        // 3. 円フラッシュを多角形化
        for circle in &geometry.circles {
            if circle.radius <= 1e-5 {
                continue;
            }
            let steps = ((2.0 * PI * circle.radius / self.arc_resolution).ceil() as usize).clamp(16, 64);
            let mut contour = Vec::with_capacity(steps);
            for i in 0..steps {
                let ang = 2.0 * PI * (i as f64) / (steps as f64);
                contour.push([
                    circle.center.x + circle.radius * ang.cos(),
                    circle.center.y + circle.radius * ang.sin(),
                ]);
            }
            let shape = vec![ensure_ccw(contour)];
            match circle.polarity {
                Polarity::Dark => dark_shapes.push(shape),
                Polarity::Clear => clear_shapes.push(shape),
            }
        }

        // 4. ポリゴン（ベタ塗り、矩形、マクロ）
        for poly in &geometry.polygons {
            if poly.points.len() >= 3 {
                let contour: Vec<[f64; 2]> = poly.points.iter().map(|p| [p.x, p.y]).collect();
                let shape = vec![ensure_ccw(contour)];
                match poly.polarity {
                    Polarity::Dark => dark_shapes.push(shape),
                    Polarity::Clear => clear_shapes.push(shape),
                }
            }
        }

        if dark_shapes.is_empty() {
            return Vec::new();
        }

        // 全Dark図形を一括ユニオン（すべてCCWなのでPositiveルールで純粋な和集合になる）
        let empty: ShapesF64 = Vec::new();
        let mut unified = dark_shapes.overlay_as::<i64>(&empty, OverlayRule::Union, FillRule::Positive);

        // Clear図形があれば先にClear同士をユニオンしてから差し引く
        if !clear_shapes.is_empty() && !unified.is_empty() {
            let unified_clear =
                clear_shapes.overlay_as::<i64>(&empty, OverlayRule::Union, FillRule::Positive);
            unified = unified.overlay_as::<i64>(
                &unified_clear,
                OverlayRule::Difference,
                FillRule::Positive,
            );
        }

        unified
    }

    /// 銅箔層のアイソレーションルーティングを生成（複数周回対応）
    pub fn generate_isolation_toolpaths(
        &self,
        geometry: &Geometry,
        passes: usize,
        overlap: f64,
        optimize: bool,
    ) -> Vec<Toolpath> {
        let unified = self.build_unified_copper_shapes(geometry);
        if unified.is_empty() {
            return Vec::new();
        }

        let pass_count = passes.max(1);
        let overlap_clamped = overlap.clamp(0.0, 0.8);
        let step_dist = self.tool_diameter * (1.0 - overlap_clamped);
        let mut toolpaths: Vec<Toolpath> = Vec::new();

        for pass_idx in 0..pass_count {
            let offset = self.tool_diameter * 0.5 + (pass_idx as f64) * step_dist;
            let mut style = OutlineStyle::new(offset);
            style.join = LineJoin::Round(0.2);

            let offset_shapes = unified.outline_as::<i64>(&style);
            for shape in offset_shapes {
                for contour in shape {
                    if contour.len() < 3 {
                        continue;
                    }
                    let mut pts: Vec<Point2> = contour.iter().map(|p| Point2::new(p[0], p[1])).collect();
                    // 閉ループとして最初の点を末尾に追加
                    pts.push(pts[0]);
                    let simplified = self.simplify_closed_path(&pts);
                    if simplified.len() >= 4 {
                        toolpaths.push(simplified);
                    }
                }
            }
        }

        if optimize {
            Self::optimize_path_sequence(&mut toolpaths);
        }

        toolpaths
    }

    /// 基板外形（Edge_Cuts）専用ツールパス生成（既存互換用）
    #[allow(dead_code)]
    pub fn generate_edge_cuts_toolpaths(
        &self,
        geometry: &Geometry,
        mode: EdgeCutMode,
        optimize: bool,
    ) -> Vec<Toolpath> {
        self.generate_edge_cuts_toolpaths_with_options(
            geometry,
            mode,
            0.0,
            EdgeGapMode::None,
            0.0,
            optimize,
        )
    }

    /// 基板外形（Edge_Cuts）専用ツールパス生成（マージン・保持タブ対応）
    pub fn generate_edge_cuts_toolpaths_with_options(
        &self,
        geometry: &Geometry,
        mode: EdgeCutMode,
        margin: f64,
        gap_mode: EdgeGapMode,
        gap_size: f64,
        optimize: bool,
    ) -> Vec<Toolpath> {
        // まず線分・円弧を点列のチェーンに繋ぐ
        let mut chains = self.chain_edge_geometry(geometry, 0.02);

        // ポリゴンとして定義された外形があればそれも追加
        for poly in &geometry.polygons {
            if poly.points.len() >= 3 {
                let mut pts = poly.points.clone();
                if pts.first().unwrap().distance_to(pts.last().unwrap()) > 1e-4 {
                    pts.push(pts[0]);
                }
                chains.push(pts);
            }
        }

        let extra_margin = margin.max(0.0);
        let mut result: Vec<Toolpath> = Vec::new();

        if mode == EdgeCutMode::OnPath && extra_margin <= 1e-6 {
            result = chains
                .into_iter()
                .map(|c| self.simplify_path(&c))
                .filter(|c| c.len() >= 2)
                .collect();
        } else {
            // 閉ループをポリゴンとしてオフセット
            let mut closed_shapes: ShapesF64 = Vec::new();
            let mut open_paths: Vec<Toolpath> = Vec::new();

            for chain in chains {
                if chain.len() >= 3
                    && chain.first().unwrap().distance_to(chain.last().unwrap()) < 0.05
                {
                    let contour: Vec<[f64; 2]> = chain[..chain.len() - 1]
                        .iter()
                        .map(|p| [p.x, p.y])
                        .collect();
                    if contour.len() >= 3 {
                        closed_shapes.push(vec![contour]);
                    }
                } else if chain.len() >= 2 {
                    open_paths.push(self.simplify_path(&chain));
                }
            }

            if !closed_shapes.is_empty() {
                // 複数外形がある場合も一旦整理（Union）
                let empty: ShapesF64 = Vec::new();
                let normalized =
                    closed_shapes.overlay_as::<i64>(&empty, OverlayRule::Union, FillRule::EvenOdd);
                let offset_val = match mode {
                    EdgeCutMode::Outside => self.tool_diameter * 0.5 + extra_margin,
                    EdgeCutMode::Inside => -(self.tool_diameter * 0.5 + extra_margin),
                    EdgeCutMode::OnPath => extra_margin,
                };
                let mut style = OutlineStyle::new(offset_val);
                style.join = LineJoin::Round(0.2);

                let offset_shapes = normalized.outline_as::<i64>(&style);
                for shape in offset_shapes {
                    for contour in shape {
                        if contour.len() < 3 {
                            continue;
                        }
                        let mut pts: Vec<Point2> =
                            contour.iter().map(|p| Point2::new(p[0], p[1])).collect();
                        pts.push(pts[0]);
                        let simplified = self.simplify_closed_path(&pts);
                        if simplified.len() >= 4 {
                            result.push(simplified);
                        }
                    }
                }
            }

            result.extend(open_paths);
        }

        if gap_mode != EdgeGapMode::None && gap_size > 0.0 {
            result = Self::apply_edge_gaps(result, gap_mode, gap_size + self.tool_diameter);
        }

        if optimize {
            Self::optimize_path_sequence(&mut result);
        }

        result
    }

    /// 保持タブ（Gaps）の位置にあるパス区間を除外して複数パスに分割する
    fn apply_edge_gaps(
        paths: Vec<Toolpath>,
        gap_mode: EdgeGapMode,
        cut_gap_width: f64,
    ) -> Vec<Toolpath> {
        if paths.is_empty() || gap_mode == EdgeGapMode::None || cut_gap_width <= 0.0 {
            return paths;
        }

        let mut bounds = crate::core::geometry::Bounds::empty();
        for p in &paths {
            for &pt in p {
                bounds.expand_point(pt);
            }
        }
        if !bounds.is_valid() {
            return paths;
        }

        let cx = (bounds.min_x + bounds.max_x) * 0.5;
        let cy = (bounds.min_y + bounds.max_y) * 0.5;
        let half = cut_gap_width * 0.5;

        let mut out_paths: Vec<Toolpath> = Vec::new();

        for path in paths {
            if path.len() < 2 {
                continue;
            }
            let was_closed = path.len() >= 3 && path.first().unwrap().distance_to(path.last().unwrap()) < 1e-4;
            let mut pieces: Vec<Toolpath> = Vec::new();
            let mut current: Toolpath = Vec::new();

            for seg in path.windows(2) {
                let a = seg[0];
                let b = seg[1];
                let kept_intervals = Self::clip_segment_outside_gaps(a, b, cx, cy, half, gap_mode);

                for (t0, t1) in kept_intervals {
                    let p0 = Point2::new(a.x + (b.x - a.x) * t0, a.y + (b.y - a.y) * t0);
                    let p1 = Point2::new(a.x + (b.x - a.x) * t1, a.y + (b.y - a.y) * t1);
                    if p0.distance_to(&p1) < 1e-5 {
                        continue;
                    }
                    if let Some(last) = current.last() {
                        if last.distance_to(&p0) <= 1e-4 {
                            current.push(p1);
                        } else {
                            if current.len() >= 2 {
                                pieces.push(std::mem::take(&mut current));
                            } else {
                                current.clear();
                            }
                            current.push(p0);
                            current.push(p1);
                        }
                    } else {
                        current.push(p0);
                        current.push(p1);
                    }
                }
            }

            if current.len() >= 2 {
                pieces.push(current);
            }

            // 元のパスが閉ループだった場合、先頭片と末尾片が元の開始点で繋がっていれば結合する
            if was_closed && pieces.len() >= 2 {
                let first_start = pieces[0][0];
                let last_end = *pieces.last().unwrap().last().unwrap();
                if first_start.distance_to(&last_end) <= 1e-4 {
                    let mut last_piece = pieces.pop().unwrap();
                    let first_piece = pieces.remove(0);
                    last_piece.extend(first_piece.into_iter().skip(1));
                    pieces.insert(0, last_piece);
                }
            }

            out_paths.extend(pieces);
        }

        out_paths
    }

    /// 線分上のパラメータ t in [0, 1] からギャップ帯外の有効区間列を返す
    fn clip_segment_outside_gaps(
        a: Point2,
        b: Point2,
        cx: f64,
        cy: f64,
        half: f64,
        mode: EdgeGapMode,
    ) -> Vec<(f64, f64)> {
        let mut forbidden: Vec<(f64, f64)> = Vec::new();
        if matches!(mode, EdgeGapMode::TopBottom | EdgeGapMode::FourSides) {
            if let Some(iv) = Self::axis_band_interval(a.x, b.x, cx - half, cx + half) {
                forbidden.push(iv);
            }
        }
        if matches!(mode, EdgeGapMode::LeftRight | EdgeGapMode::FourSides) {
            if let Some(iv) = Self::axis_band_interval(a.y, b.y, cy - half, cy + half) {
                forbidden.push(iv);
            }
        }
        if forbidden.is_empty() {
            return vec![(0.0, 1.0)];
        }
        forbidden.sort_by(|u, v| u.0.partial_cmp(&v.0).unwrap_or(std::cmp::Ordering::Equal));

        // 禁止区間の和集合を [0, 1] から差し引く
        let mut merged: Vec<(f64, f64)> = Vec::new();
        for (s, e) in forbidden {
            if let Some(last) = merged.last_mut() {
                if s <= last.1 + 1e-9 {
                    last.1 = last.1.max(e);
                    continue;
                }
            }
            merged.push((s, e));
        }

        let mut kept = Vec::new();
        let mut cursor = 0.0_f64;
        for (s, e) in merged {
            if s > cursor + 1e-6 {
                kept.push((cursor, s));
            }
            cursor = cursor.max(e);
        }
        if cursor < 1.0 - 1e-6 {
            kept.push((cursor, 1.0));
        }
        kept
    }

    /// 1次元線分 a + t*(b - a) が [band_min, band_max] に入る t in [0, 1] の区間を求める
    fn axis_band_interval(a: f64, b: f64, band_min: f64, band_max: f64) -> Option<(f64, f64)> {
        let d = b - a;
        if d.abs() < 1e-12 {
            if a >= band_min && a <= band_max {
                return Some((0.0, 1.0));
            }
            return None;
        }
        let t1 = (band_min - a) / d;
        let t2 = (band_max - a) / d;
        let t_lo = t1.min(t2).max(0.0);
        let t_hi = t1.max(t2).min(1.0);
        if t_hi > t_lo + 1e-9 {
            Some((t_lo, t_hi))
        } else {
            None
        }
    }

    /// ばらばらの線分・円弧を端点距離で繋ぎ合わせて連続パスにする
    fn chain_edge_geometry(&self, geometry: &Geometry, tol: f64) -> Vec<Vec<Point2>> {
        let mut raw_segments: Vec<Vec<Point2>> = Vec::new();

        for seg in &geometry.segments {
            if seg.start.distance_to(&seg.end) > 1e-5 {
                raw_segments.push(vec![seg.start, seg.end]);
            }
        }
        for arc in &geometry.arcs {
            let pts = GerberParser::sample_arc_points(
                arc.start,
                arc.end,
                arc.center,
                arc.clockwise,
                self.arc_resolution,
            );
            if pts.len() >= 2 {
                raw_segments.push(pts);
            }
        }

        let mut chains: Vec<Vec<Point2>> = Vec::new();
        while let Some(mut current) = raw_segments.pop() {
            loop {
                let mut merged = false;
                let cur_start = *current.first().unwrap();
                let cur_end = *current.last().unwrap();

                // もしすでに閉じていたら終了
                if current.len() >= 3 && cur_start.distance_to(&cur_end) <= tol {
                    break;
                }

                for i in 0..raw_segments.len() {
                    let cand_start = *raw_segments[i].first().unwrap();
                    let cand_end = *raw_segments[i].last().unwrap();

                    if cur_end.distance_to(&cand_start) <= tol {
                        let seg = raw_segments.remove(i);
                        current.extend(seg.into_iter().skip(1));
                        merged = true;
                        break;
                    } else if cur_end.distance_to(&cand_end) <= tol {
                        let mut seg = raw_segments.remove(i);
                        seg.reverse();
                        current.extend(seg.into_iter().skip(1));
                        merged = true;
                        break;
                    } else if cur_start.distance_to(&cand_end) <= tol {
                        let mut seg = raw_segments.remove(i);
                        seg.extend(current.into_iter().skip(1));
                        current = seg;
                        merged = true;
                        break;
                    } else if cur_start.distance_to(&cand_start) <= tol {
                        let mut seg = raw_segments.remove(i);
                        seg.reverse();
                        seg.extend(current.into_iter().skip(1));
                        current = seg;
                        merged = true;
                        break;
                    }
                }
                if !merged {
                    break;
                }
            }
            chains.push(current);
        }

        chains
    }

    /// Ramer-Douglas-Peucker アルゴリズムで閉ループパスを簡潔化
    fn simplify_closed_path(&self, pts: &[Point2]) -> Vec<Point2> {
        if pts.len() <= 4 || self.simplify_tolerance <= 0.0 {
            return pts.to_vec();
        }
        // 始点から最も遠い点で2分割してからそれぞれRDPにかけることで閉ループが2点に潰れるのを防ぐ
        let start = pts[0];
        let mut far_idx = pts.len() / 2;
        let mut far_dist = -1.0;
        for (i, &p) in pts.iter().enumerate().skip(1).take(pts.len() - 2) {
            let d = start.distance_to(&p);
            if d > far_dist {
                far_dist = d;
                far_idx = i;
            }
        }
        if far_dist < self.simplify_tolerance * 2.0 {
            return Vec::new();
        }
        let mut first_half = Self::rdp_recursive(&pts[..=far_idx], self.simplify_tolerance);
        let second_half = Self::rdp_recursive(&pts[far_idx..], self.simplify_tolerance);
        first_half.pop();
        first_half.extend(second_half);
        first_half
    }

    /// Ramer-Douglas-Peucker アルゴリズムで開パスを簡潔化
    fn simplify_path(&self, pts: &[Point2]) -> Vec<Point2> {
        if pts.len() <= 2 || self.simplify_tolerance <= 0.0 {
            return pts.to_vec();
        }
        Self::rdp_recursive(pts, self.simplify_tolerance)
    }

    fn rdp_recursive(pts: &[Point2], tol: f64) -> Vec<Point2> {
        if pts.len() <= 2 {
            return pts.to_vec();
        }
        let start = pts[0];
        let end = pts[pts.len() - 1];
        let mut max_dist = 0.0;
        let mut max_idx = 0;

        for (i, &p) in pts.iter().enumerate().skip(1).take(pts.len() - 2) {
            let d = Self::point_to_segment_distance(p, start, end);
            if d > max_dist {
                max_dist = d;
                max_idx = i;
            }
        }

        if max_dist > tol {
            let mut left = Self::rdp_recursive(&pts[..=max_idx], tol);
            let right = Self::rdp_recursive(&pts[max_idx..], tol);
            left.pop();
            left.extend(right);
            left
        } else {
            vec![start, end]
        }
    }

    fn point_to_segment_distance(p: Point2, a: Point2, b: Point2) -> f64 {
        let dx = b.x - a.x;
        let dy = b.y - a.y;
        let len_sq = dx * dx + dy * dy;
        if len_sq < 1e-12 {
            return p.distance_to(&a);
        }
        let t = (((p.x - a.x) * dx + (p.y - a.y) * dy) / len_sq).clamp(0.0, 1.0);
        let proj = Point2::new(a.x + t * dx, a.y + t * dy);
        p.distance_to(&proj)
    }

    /// Nearest Neighbor 法でパス順序を最適化（逆方向移動も考慮）
    pub fn optimize_path_sequence(paths: &mut Vec<Toolpath>) {
        if paths.len() <= 1 {
            return;
        }
        let mut remaining = std::mem::take(paths);
        let mut ordered = Vec::with_capacity(remaining.len());

        // 原点(0,0)に最も近いパスから開始
        let origin = Point2::new(0.0, 0.0);
        let mut best_start_idx = 0;
        let mut best_start_dist = f64::INFINITY;
        let mut best_start_rev = false;

        for (i, p) in remaining.iter().enumerate() {
            if p.is_empty() {
                continue;
            }
            let d0 = origin.distance_to(&p[0]);
            if d0 < best_start_dist {
                best_start_dist = d0;
                best_start_idx = i;
                best_start_rev = false;
            }
            let d1 = origin.distance_to(p.last().unwrap());
            if d1 < best_start_dist {
                best_start_dist = d1;
                best_start_idx = i;
                best_start_rev = true;
            }
        }

        let mut first = remaining.swap_remove(best_start_idx);
        if best_start_rev {
            first.reverse();
        }
        let mut current_pos = *first.last().unwrap_or(&origin);
        ordered.push(first);

        while !remaining.is_empty() {
            let mut best_idx = 0;
            let mut best_dist = f64::INFINITY;
            let mut best_rev = false;

            for (i, p) in remaining.iter().enumerate() {
                if p.is_empty() {
                    continue;
                }
                let d0 = current_pos.distance_to(&p[0]);
                if d0 < best_dist {
                    best_dist = d0;
                    best_idx = i;
                    best_rev = false;
                }
                let d1 = current_pos.distance_to(p.last().unwrap());
                if d1 < best_dist {
                    best_dist = d1;
                    best_idx = i;
                    best_rev = true;
                }
            }

            let mut next_path = remaining.swap_remove(best_idx);
            if best_rev {
                next_path.reverse();
            }
            if let Some(last) = next_path.last() {
                current_pos = *last;
            }
            ordered.push(next_path);
        }

        *paths = ordered;
    }

    /// ドリル穴の順序最適化（最適化前・最適化後の総移動距離も返す）
    pub fn optimize_drill_holes(drill: &DrillData) -> (Vec<DrillHole>, f64, f64) {
        if drill.holes.is_empty() {
            return (Vec::new(), 0.0, 0.0);
        }

        let calc_dist = |holes: &[DrillHole]| -> f64 {
            let mut total = 0.0;
            let mut prev = Point2::new(0.0, 0.0);
            for h in holes {
                total += prev.distance_to(&h.position);
                prev = h.position;
            }
            total
        };

        let original_dist = calc_dist(&drill.holes);
        let mut remaining = drill.holes.clone();
        let mut ordered = Vec::with_capacity(remaining.len());
        let mut current = Point2::new(0.0, 0.0);

        while !remaining.is_empty() {
            let mut best_idx = 0;
            let mut best_dist = f64::INFINITY;
            for (i, h) in remaining.iter().enumerate() {
                let d = current.distance_to(&h.position);
                if d < best_dist {
                    best_dist = d;
                    best_idx = i;
                }
            }
            let chosen = remaining.swap_remove(best_idx);
            current = chosen.position;
            ordered.push(chosen);
        }

        let optimized_dist = calc_dist(&ordered);
        (ordered, original_dist, optimized_dist)
    }

    /// ドリル穴リストを「通常ドリル穴」と「エンドミルくり抜き円パス (MillHoles)」に分離する
    pub fn split_and_generate_mill_holes(&self, holes: &[DrillHole]) -> (Vec<DrillHole>, Vec<Toolpath>) {
        let tool_dia = self.tool_diameter.max(1e-4);
        let mut plunge_holes = Vec::new();
        let mut mill_paths = Vec::new();

        for hole in holes {
            if hole.diameter <= tool_dia + 1e-4 {
                plunge_holes.push(hole.clone());
                continue;
            }

            // エンドミル中心が描く最大半径
            let max_r = (hole.diameter - tool_dia) * 0.5;
            if max_r <= 1e-4 {
                plunge_holes.push(hole.clone());
                continue;
            }

            // 中心に削り残しが出ないようにステップオーバーで同心円を生成
            let step_r = (tool_dia * 0.6).max(0.05);
            let mut radii = Vec::new();
            let mut r = (tool_dia * 0.45).min(max_r);
            while r < max_r - 1e-4 {
                radii.push(r);
                r += step_r;
            }
            radii.push(max_r);

            for rad in radii {
                let steps = ((2.0 * PI * rad / self.arc_resolution).ceil() as usize).clamp(24, 64);
                let mut pts = Vec::with_capacity(steps + 1);
                for i in 0..=steps {
                    let ang = 2.0 * PI * ((i % steps) as f64) / (steps as f64);
                    pts.push(Point2::new(
                        hole.position.x + rad * ang.cos(),
                        hole.position.y + rad * ang.sin(),
                    ));
                }
                mill_paths.push(pts);
            }
        }

        Self::optimize_path_sequence(&mut mill_paths);
        (plunge_holes, mill_paths)
    }

    /// DRC（最小パターン幅、最小クリアランス、現在のツール径での加工不能箇所）を実行する
    pub fn run_drc(&self, geometry: &Geometry, min_trace_width: f64) -> DrcReport {
        let mut report = DrcReport::default();
        let cluster_radius_trace = 0.5_f64;

        // 1. 最小パターン幅の計測と細線検出
        let mut min_w = f64::INFINITY;
        for seg in &geometry.segments {
            if seg.polarity == Polarity::Dark && seg.width > 1e-4 {
                min_w = min_w.min(seg.width);
                if seg.width + 1e-5 < min_trace_width {
                    let mid = Point2::new(
                        (seg.start.x + seg.end.x) * 0.5,
                        (seg.start.y + seg.end.y) * 0.5,
                    );
                    Self::push_clustered_violation(
                        &mut report.narrow_traces,
                        mid,
                        seg.width.max(0.3),
                        format!("細線 {:.3}mm < 基準 {:.3}mm", seg.width, min_trace_width),
                        cluster_radius_trace,
                    );
                }
            }
        }
        for arc in &geometry.arcs {
            if arc.polarity == Polarity::Dark && arc.width > 1e-4 {
                min_w = min_w.min(arc.width);
                if arc.width + 1e-5 < min_trace_width {
                    let mid = Point2::new(
                        (arc.start.x + arc.end.x) * 0.5,
                        (arc.start.y + arc.end.y) * 0.5,
                    );
                    Self::push_clustered_violation(
                        &mut report.narrow_traces,
                        mid,
                        arc.width.max(0.3),
                        format!("細円弧 {:.3}mm < 基準 {:.3}mm", arc.width, min_trace_width),
                        cluster_radius_trace,
                    );
                }
            }
        }
        if min_w.is_finite() {
            report.min_trace_width_mm = Some(min_w);
        }

        // 2. 統合銅箔ポリゴンからのクリアランス計測と加工不能箇所検出
        let unified = self.build_unified_copper_shapes(geometry);
        if unified.is_empty() {
            return report;
        }

        struct EdgeSeg {
            a: Point2,
            b: Point2,
            min_x: f64,
            min_y: f64,
            max_x: f64,
            max_y: f64,
            contour_id: usize,
            edge_idx: usize,
            contour_len: usize,
        }

        let mut contours_edges: Vec<Vec<EdgeSeg>> = Vec::new();
        let mut contour_bounds: Vec<[f64; 4]> = Vec::new();

        for shape in &unified {
            for contour in shape {
                if contour.len() < 3 {
                    continue;
                }
                let cid = contours_edges.len();
                let n = contour.len();
                let mut edges = Vec::with_capacity(n);
                let mut c_min_x = f64::INFINITY;
                let mut c_min_y = f64::INFINITY;
                let mut c_max_x = f64::NEG_INFINITY;
                let mut c_max_y = f64::NEG_INFINITY;

                for i in 0..n {
                    let p0 = Point2::new(contour[i][0], contour[i][1]);
                    let p1 = Point2::new(contour[(i + 1) % n][0], contour[(i + 1) % n][1]);
                    let min_x = p0.x.min(p1.x);
                    let min_y = p0.y.min(p1.y);
                    let max_x = p0.x.max(p1.x);
                    let max_y = p0.y.max(p1.y);
                    c_min_x = c_min_x.min(min_x);
                    c_min_y = c_min_y.min(min_y);
                    c_max_x = c_max_x.max(max_x);
                    c_max_y = c_max_y.max(max_y);
                    edges.push(EdgeSeg {
                        a: p0,
                        b: p1,
                        min_x,
                        min_y,
                        max_x,
                        max_y,
                        contour_id: cid,
                        edge_idx: i,
                        contour_len: n,
                    });
                }
                contours_edges.push(edges);
                contour_bounds.push([c_min_x, c_min_y, c_max_x, c_max_y]);
            }
        }

        let mut min_clearance = f64::INFINITY;
        let cluster_radius_gap = (self.tool_diameter * 1.5).max(0.3);

        for c1 in 0..contours_edges.len() {
            for c2 in c1..contours_edges.len() {
                let threshold = min_clearance.max(self.tool_diameter);
                let b1 = contour_bounds[c1];
                let b2 = contour_bounds[c2];
                if Self::aabb_distance(b1, b2) > threshold {
                    continue;
                }

                let edges1 = &contours_edges[c1];
                let edges2 = &contours_edges[c2];

                for e1 in edges1 {
                    let start_j = if c1 == c2 { e1.edge_idx + 1 } else { 0 };
                    for e2 in &edges2[start_j..] {
                        if e1.contour_id == e2.contour_id {
                            let diff = e2.edge_idx - e1.edge_idx;
                            let wrap_diff = e1.contour_len - diff;
                            if diff.min(wrap_diff) <= 3 {
                                continue;
                            }
                        }

                        let cur_thresh = min_clearance.max(self.tool_diameter);
                        if Self::aabb_distance(
                            [e1.min_x, e1.min_y, e1.max_x, e1.max_y],
                            [e2.min_x, e2.min_y, e2.max_x, e2.max_y],
                        ) > cur_thresh
                        {
                            continue;
                        }

                        let (dist, p1, p2) =
                            Self::segment_distance_and_points(e1.a, e1.b, e2.a, e2.b);
                        if dist <= 1e-4 {
                            continue;
                        }
                        if dist > cur_thresh {
                            continue;
                        }

                        let mid = Point2::new((p1.x + p2.x) * 0.5, (p1.y + p2.y) * 0.5);
                        // 中点が銅箔ポリゴン内部にある場合はクリアランスではなくパターン肉厚なので除外
                        if Self::point_in_shapes(mid, &unified) {
                            continue;
                        }

                        if dist < min_clearance {
                            min_clearance = dist;
                        }
                        if dist < self.tool_diameter - 1e-4 {
                            Self::push_clustered_violation(
                                &mut report.uncut_bridges,
                                mid,
                                self.tool_diameter.max(0.3),
                                format!(
                                    "加工不能: クリアランス {:.3}mm < ツール径 {:.3}mm",
                                    dist, self.tool_diameter
                                ),
                                cluster_radius_gap,
                            );
                        }
                    }
                }
            }
        }

        if min_clearance.is_finite() {
            report.min_clearance_mm = Some(min_clearance);
        }

        report
    }

    fn push_clustered_violation(
        list: &mut Vec<DrcViolation>,
        pos: Point2,
        diameter: f64,
        description: String,
        cluster_radius: f64,
    ) {
        if list.iter().any(|v| v.position.distance_to(&pos) < cluster_radius) {
            return;
        }
        list.push(DrcViolation {
            position: pos,
            diameter,
            description,
        });
    }

    fn aabb_distance(a: [f64; 4], b: [f64; 4]) -> f64 {
        let dx = if a[2] < b[0] {
            b[0] - a[2]
        } else if b[2] < a[0] {
            a[0] - b[2]
        } else {
            0.0
        };
        let dy = if a[3] < b[1] {
            b[1] - a[3]
        } else if b[3] < a[1] {
            a[1] - b[3]
        } else {
            0.0
        };
        (dx * dx + dy * dy).sqrt()
    }

    /// 2D点 p が統一シェイプ（外周+穴）の内部にあるかを偶数奇規則で判定
    fn point_in_shapes(p: Point2, shapes: &ShapesF64) -> bool {
        let mut inside = false;
        for shape in shapes {
            for contour in shape {
                let n = contour.len();
                if n < 3 {
                    continue;
                }
                let mut j = n - 1;
                for i in 0..n {
                    let yi = contour[i][1];
                    let yj = contour[j][1];
                    if (yi > p.y) != (yj > p.y) {
                        let xi = contour[i][0];
                        let xj = contour[j][0];
                        let x_intersect = (xj - xi) * (p.y - yi) / (yj - yi) + xi;
                        if p.x < x_intersect {
                            inside = !inside;
                        }
                    }
                    j = i;
                }
            }
        }
        inside
    }

    /// 2つの線分 [a0, a1] と [b0, b1] の最短距離と両線分上の最近接点を返す
    fn segment_distance_and_points(
        a0: Point2,
        a1: Point2,
        b0: Point2,
        b1: Point2,
    ) -> (f64, Point2, Point2) {
        let closest_on_seg = |p: Point2, s0: Point2, s1: Point2| -> Point2 {
            let dx = s1.x - s0.x;
            let dy = s1.y - s0.y;
            let len_sq = dx * dx + dy * dy;
            if len_sq < 1e-14 {
                return s0;
            }
            let t = (((p.x - s0.x) * dx + (p.y - s0.y) * dy) / len_sq).clamp(0.0, 1.0);
            Point2::new(s0.x + t * dx, s0.y + t * dy)
        };

        let cross = |o: Point2, a: Point2, b: Point2| -> f64 {
            (a.x - o.x) * (b.y - o.y) - (a.y - o.y) * (b.x - o.x)
        };
        let d1 = cross(b0, b1, a0);
        let d2 = cross(b0, b1, a1);
        let d3 = cross(a0, a1, b0);
        let d4 = cross(a0, a1, b1);
        if ((d1 > 0.0 && d2 < 0.0) || (d1 < 0.0 && d2 > 0.0))
            && ((d3 > 0.0 && d4 < 0.0) || (d3 < 0.0 && d4 > 0.0))
        {
            return (0.0, a0, a0);
        }

        let candidates = [
            (a0, closest_on_seg(a0, b0, b1)),
            (a1, closest_on_seg(a1, b0, b1)),
            (closest_on_seg(b0, a0, a1), b0),
            (closest_on_seg(b1, a0, a1), b1),
        ];

        let mut best_dist = f64::INFINITY;
        let mut best_pa = a0;
        let mut best_pb = b0;
        for (pa, pb) in candidates {
            let d = pa.distance_to(&pb);
            if d < best_dist {
                best_dist = d;
                best_pa = pa;
                best_pb = pb;
            }
        }
        (best_dist, best_pa, best_pb)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::excellon::ExcellonParser;
    use crate::core::gerber::GerberParser;
    use crate::gcode::generator::{parse_gcode_for_sim, GCodeGenerator, GCodeSettings};

    #[test]
    fn test_two_segments_and_outline() {
        let s1 = [[0.0, 0.0], [10.0, 0.0]];
        let s2 = [[0.0, 0.0], [0.0, 10.0]];
        let style = StrokeStyle::new(2.0)
            .start_cap(LineCap::Round(0.25))
            .end_cap(LineCap::Round(0.25))
            .line_join(LineJoin::Round(0.25));
        let mut shapes: ShapesF64 = Vec::new();
        shapes.extend(s1.stroke_as::<i64>(style.clone(), false));
        shapes.extend(s2.stroke_as::<i64>(style, false));
        let empty: ShapesF64 = Vec::new();
        let unified = shapes.overlay_as::<i64>(&empty, OverlayRule::Union, FillRule::NonZero);
        println!("unified shapes count: {}", unified.len());
        for (i, s) in unified.iter().enumerate() {
            println!("  unified shape {} contours: {}", i, s.len());
            for (ci, c) in s.iter().enumerate() {
                println!("    contour {} len: {}", ci, c.len());
            }
        }
        let mut ostyle = OutlineStyle::new(0.1);
        ostyle.join = LineJoin::Round(0.2);
        let outlined = unified.outline_as::<i64>(&ostyle);
        println!("outlined shapes count: {}", outlined.len());
        for (i, s) in outlined.iter().enumerate() {
            println!("  outlined shape {} contours: {}", i, s.len());
            for (ci, c) in s.iter().enumerate() {
                println!("    contour {} len: {}", ci, c.len());
            }
        }
        assert_eq!(unified.len(), 1);
        assert_eq!(unified[0].len(), 1, "2本の線分が穴なしで1つの塊になること");
    }

    #[test]
    fn test_stroke_cap() {
        let path = [[0.0, 0.0], [10.0, 0.0]];
        let style = StrokeStyle::new(2.0)
            .start_cap(LineCap::Round(0.25))
            .end_cap(LineCap::Round(0.25))
            .line_join(LineJoin::Round(0.25));
        let stroked = path.stroke_as::<i64>(style, false);
        println!("stroked shapes count: {}", stroked.len());
        for (si, s) in stroked.iter().enumerate() {
            println!("  shape {} contours: {}", si, s.len());
            for (ci, c) in s.iter().enumerate() {
                println!("    contour {} pts({}): {:?}", ci, c.len(), c);
            }
        }
    }

    #[test]
    fn test_sample_b_cu_pipeline() {
        let geo = GerberParser::parse_file("sample/SSP4-B_Cu.gbr").expect("B_Cu 読込失敗");
        assert!(!geo.segments.is_empty(), "線分が読み込まれていること");
        assert!(!geo.polygons.is_empty(), "ポリゴン（Outline8P等）が読み込まれていること");

        let gen = ToolpathGenerator::new(0.1);
        let unified = gen.build_unified_copper_shapes(&geo);
        let mut hole_count = 0;
        let mut cw_outer = 0;
        for shape in &unified {
            if let Some(outer) = shape.first() {
                if signed_area(outer) < 0.0 {
                    cw_outer += 1;
                }
            }
            hole_count += shape.len().saturating_sub(1);
        }
        println!(
            "B_Cu unified shapes: {}, holes: {}, cw_outer: {}, segs: {}, arcs: {}, circles: {}, polys: {}",
            unified.len(),
            hole_count,
            cw_outer,
            geo.segments.len(),
            geo.arcs.len(),
            geo.circles.len(),
            geo.polygons.len()
        );
        // 最初の5セグメントと最初の5ポリゴンの向きも確認
        for (i, s) in geo.segments.iter().take(5).enumerate() {
            println!("  seg {}: ({:.3},{:.3})->({:.3},{:.3}) w={:.3}", i, s.start.x, s.start.y, s.end.x, s.end.y, s.width);
        }
        for (i, p) in geo.polygons.iter().take(5).enumerate() {
            let pts: Vec<[f64; 2]> = p.points.iter().map(|pt| [pt.x, pt.y]).collect();
            println!("  poly {}: pts={} area={:.4}", i, pts.len(), signed_area(&pts));
        }

        assert_eq!(cw_outer, 0, "外周輪郭はすべてCCWであること");
        assert!(hole_count <= 7, "重なり部分に不要な穴が生成されないこと (holes={})", hole_count);

        let paths = gen.generate_isolation_toolpaths(&geo, 1, 0.25, true);
        println!("B_Cu generated toolpaths: {}", paths.len());
        assert!(!paths.is_empty(), "アイソレーションパスが生成されること");
        // 各パスが閉じていること
        for p in &paths {
            assert!(p.len() >= 3);
            assert!(p.first().unwrap().distance_to(p.last().unwrap()) < 1e-4);
        }
    }

    fn signed_area(pts: &[[f64; 2]]) -> f64 {
        let mut a = 0.0;
        for i in 0..pts.len() {
            let j = (i + 1) % pts.len();
            a += pts[i][0] * pts[j][1] - pts[j][0] * pts[i][1];
        }
        a * 0.5
    }

    #[test]
    fn test_sample_edge_cuts_pipeline() {
        let geo = GerberParser::parse_file("sample/SSP4-Edge_Cuts.gbr").expect("Edge_Cuts 読込失敗");
        let gen = ToolpathGenerator::new(1.0);
        let paths = gen.generate_edge_cuts_toolpaths(&geo, EdgeCutMode::Outside, true);
        assert_eq!(paths.len(), 1, "基板外形は1つの外側ループになること");
    }

    #[test]
    fn test_sample_drill_and_gcode_pipeline() {
        let drill = ExcellonParser::parse_file("sample/SSP4.drl").expect("Drill 読込失敗");
        assert!(!drill.holes.is_empty(), "ドリル穴が読み込まれていること");

        let (ordered, orig, opt) = ToolpathGenerator::optimize_drill_holes(&drill);
        assert_eq!(ordered.len(), drill.holes.len());
        assert!(opt <= orig, "最適化後の距離が元の距離以下であること");

        let settings = GCodeSettings {
            cut_z: -1.6,
            pass_depth: 0.4,
            ..Default::default()
        };
        let gcode_gen = GCodeGenerator::new(settings);
        let lines = gcode_gen.generate_drill_gcode(&ordered, &drill.slots);
        let (segs, stats) = parse_gcode_for_sim(&lines);
        assert!(!segs.is_empty());
        assert!(stats.cut_distance_mm > 0.0);
    }

    #[test]
    fn test_edge_cuts_margin_and_gaps() {
        let mut geo = Geometry::new();
        // 0..20 x 0..10 の矩形外形
        geo.polygons.push(crate::core::geometry::PolygonShape {
            points: vec![
                Point2::new(0.0, 0.0),
                Point2::new(20.0, 0.0),
                Point2::new(20.0, 10.0),
                Point2::new(0.0, 10.0),
            ],
            polarity: Polarity::Dark,
        });
        let gen = ToolpathGenerator::new(1.0);
        let paths = gen.generate_edge_cuts_toolpaths_with_options(
            &geo,
            EdgeCutMode::Outside,
            0.5,
            EdgeGapMode::FourSides,
            2.0,
            true,
        );
        // 4辺の中央にタブが入るので、4つのコーナーパスに分割される
        assert_eq!(paths.len(), 4, "4方向タブにより4本のコーナーパスに分割されること");
        // マージン 0.5mm + ツール半径 0.5mm = 外側に 1.0mm オフセットされていること
        let mut min_x = f64::INFINITY;
        let mut max_x = f64::NEG_INFINITY;
        for p in &paths {
            for pt in p {
                min_x = min_x.min(pt.x);
                max_x = max_x.max(pt.x);
            }
        }
        assert!((min_x - (-1.0)).abs() < 0.05, "min_x={}", min_x);
        assert!((max_x - 21.0).abs() < 0.05, "max_x={}", max_x);
    }

    #[test]
    fn test_drill_mill_holes() {
        let holes = vec![
            DrillHole {
                position: Point2::new(5.0, 5.0),
                diameter: 0.8,
            },
            DrillHole {
                position: Point2::new(15.0, 5.0),
                diameter: 2.0,
            },
        ];
        let gen = ToolpathGenerator::new(0.8);
        let (plunge, mill_paths) = gen.split_and_generate_mill_holes(&holes);
        assert_eq!(plunge.len(), 1, "φ0.8穴は通常ドリル穴になること");
        assert!(!mill_paths.is_empty(), "φ2.0穴はエンドミルくり抜き円パスが生成されること");
        // 最外周円パスの半径が (2.0 - 0.8)/2 = 0.6mm であること
        let max_r = mill_paths
            .iter()
            .flat_map(|p| p.iter())
            .map(|pt| pt.distance_to(&Point2::new(15.0, 5.0)))
            .fold(0.0_f64, f64::max);
        assert!((max_r - 0.6).abs() < 0.02, "max_r={}", max_r);
    }

    #[test]
    fn test_drc_check_clearance_and_width() {
        let mut geo = Geometry::new();
        // 太さ0.2mmの線を y=0 と y=0.3 に配置 -> 中心間0.3mm、線幅各0.1mmずつなのでクリアランス0.1mm
        geo.segments.push(crate::core::geometry::Segment {
            start: Point2::new(0.0, 0.0),
            end: Point2::new(10.0, 0.0),
            width: 0.2,
            polarity: Polarity::Dark,
        });
        geo.segments.push(crate::core::geometry::Segment {
            start: Point2::new(0.0, 0.3),
            end: Point2::new(10.0, 0.3),
            width: 0.12,
            polarity: Polarity::Dark,
        });

        let gen = ToolpathGenerator::new(0.2);
        let report = gen.run_drc(&geo, 0.15);

        let min_w = report.min_trace_width_mm.expect("最小パターン幅が取得できること");
        assert!((min_w - 0.12).abs() < 1e-4, "min_w={}", min_w);
        assert_eq!(report.narrow_traces.len(), 1, "0.15mm未満の細線が1件検出されること");

        let min_c = report.min_clearance_mm.expect("最小クリアランスが取得できること");
        // 線幅 0.2/2 + 0.12/2 = 0.16, 中心間 0.30 -> クリアランス 0.14mm
        assert!((min_c - 0.14).abs() < 0.02, "min_c={}", min_c);
        assert!(
            !report.uncut_bridges.is_empty(),
            "ツール径0.2mmに対してクリアランス0.14mmの加工不能箇所が検出されること"
        );
    }

    #[test]
    fn test_sample_b_cu_drc_and_mill_holes_gcode() {
        let geo = GerberParser::parse_file("sample/SSP4-B_Cu.gbr").expect("B_Cu 読込失敗");
        let gen = ToolpathGenerator::new(0.2);
        let report = gen.run_drc(&geo, 0.15);
        assert!(report.min_trace_width_mm.is_some());
        assert!(report.min_clearance_mm.is_some());

        let drill = ExcellonParser::parse_file("sample/SSP4.drl").expect("Drill 読込失敗");
        let drill_gen = ToolpathGenerator::new(0.5);
        let (plunge, mill_paths) = drill_gen.split_and_generate_mill_holes(&drill.holes);
        let gcode_gen = GCodeGenerator::new(GCodeSettings {
            cut_z: -1.7,
            pass_depth: 0.5,
            ..Default::default()
        });
        let lines = gcode_gen.generate_drill_and_mill_gcode(&plunge, &drill.slots, &mill_paths);
        let (_, stats) = parse_gcode_for_sim(&lines);
        assert!(stats.estimated_minutes > 0.0);
    }
}
