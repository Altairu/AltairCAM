//! ツールパス生成および最適化エンジン (i_overlay ベース)

use std::f64::consts::PI;

use i_overlay::core::fill_rule::FillRule;
use i_overlay::core::overlay_rule::OverlayRule;
use i_overlay::float::single::SingleFloatOverlay;
use i_overlay::mesh::float::outline::offset::OutlineOffset;
use i_overlay::mesh::float::stroke::offset::StrokeOffset;
use i_overlay::mesh::float::style::{LineCap, LineJoin, OutlineStyle, StrokeStyle};

use crate::config::EdgeCutMode;
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

    /// 基板外形（Edge_Cuts）専用ツールパス生成
    pub fn generate_edge_cuts_toolpaths(
        &self,
        geometry: &Geometry,
        mode: EdgeCutMode,
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

        if mode == EdgeCutMode::OnPath {
            let mut result: Vec<Toolpath> = chains
                .into_iter()
                .map(|c| self.simplify_path(&c))
                .filter(|c| c.len() >= 2)
                .collect();
            if optimize {
                Self::optimize_path_sequence(&mut result);
            }
            return result;
        }

        // Outside / Inside の場合は閉ループをポリゴンとしてオフセット
        let mut closed_shapes: ShapesF64 = Vec::new();
        let mut open_paths: Vec<Toolpath> = Vec::new();

        for chain in chains {
            if chain.len() >= 3 && chain.first().unwrap().distance_to(chain.last().unwrap()) < 0.05 {
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

        let mut result: Vec<Toolpath> = Vec::new();

        if !closed_shapes.is_empty() {
            // 複数外形がある場合も一旦整理（Union）
            let empty: ShapesF64 = Vec::new();
            let normalized = closed_shapes.overlay_as::<i64>(&empty, OverlayRule::Union, FillRule::EvenOdd);
            let offset_val = match mode {
                EdgeCutMode::Outside => self.tool_diameter * 0.5,
                EdgeCutMode::Inside => -self.tool_diameter * 0.5,
                EdgeCutMode::OnPath => 0.0,
            };
            let mut style = OutlineStyle::new(offset_val);
            style.join = LineJoin::Round(0.2);

            let offset_shapes = normalized.outline_as::<i64>(&style);
            for shape in offset_shapes {
                for contour in shape {
                    if contour.len() < 3 {
                        continue;
                    }
                    let mut pts: Vec<Point2> = contour.iter().map(|p| Point2::new(p[0], p[1])).collect();
                    pts.push(pts[0]);
                    let simplified = self.simplify_closed_path(&pts);
                    if simplified.len() >= 4 {
                        result.push(simplified);
                    }
                }
            }
        }

        result.extend(open_paths);

        if optimize {
            Self::optimize_path_sequence(&mut result);
        }

        result
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
}
