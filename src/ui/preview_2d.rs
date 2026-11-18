//! 2D プレビューキャンバス

use eframe::egui;

use crate::core::geometry::{Bounds, DrillData, Geometry, Point2, Polarity, Toolpath};
use crate::core::gerber::GerberParser;
use crate::core::toolpath::DrcViolation;

/// 2D プレビュー描画に必要なデータ一式
pub struct Preview2DParams<'a> {
    pub b_cu: Option<&'a Geometry>,
    pub edge_cuts: Option<&'a Geometry>,
    pub drill: Option<&'a DrillData>,
    pub b_cu_paths: &'a [Toolpath],
    pub edge_paths: &'a [Toolpath],
    pub drill_order: &'a [Point2],
    pub drill_mill_paths: &'a [Toolpath],
    pub b_cu_tool_dia: f64,
    pub edge_tool_dia: f64,
    pub drill_tool_dia: f64,
    pub drc_violations: &'a [DrcViolation],
    pub total_bounds: Bounds,
}

struct CachedPoly {
    points: Vec<Point2>,
    indices: Vec<u32>,
    polarity: Polarity,
}

#[derive(Default)]
struct PolyMeshCache {
    hash: u64,
    polys: Vec<CachedPoly>,
}

impl PolyMeshCache {
    fn update(&mut self, geo: &Geometry) {
        let h = Self::compute_hash(geo);
        if self.hash == h && !self.polys.is_empty() {
            return;
        }
        self.hash = h;
        self.polys.clear();

        for poly in &geo.polygons {
            if poly.points.len() < 3 {
                continue;
            }
            let mut coords = Vec::with_capacity(poly.points.len() * 2);
            for p in &poly.points {
                coords.push(p.x);
                coords.push(p.y);
            }
            let raw_indices = earcutr::earcut(&coords, &[], 2).unwrap_or_default();
            let indices: Vec<u32> = raw_indices.into_iter().map(|i| i as u32).collect();
            self.polys.push(CachedPoly {
                points: poly.points.clone(),
                indices,
                polarity: poly.polarity,
            });
        }
    }

    fn compute_hash(geo: &Geometry) -> u64 {
        let mut h: u64 = (geo.polygons.len() as u64).wrapping_mul(0x9e3779b97f4a7c15);
        for p in &geo.polygons {
            h = h.rotate_left(7) ^ (p.points.len() as u64);
            if let Some(first) = p.points.first() {
                h = h.wrapping_mul(0x517cc1b727220a95) ^ first.x.to_bits() ^ first.y.to_bits();
            }
            if let Some(last) = p.points.last() {
                h = h.wrapping_mul(0x517cc1b727220a95) ^ last.x.to_bits() ^ last.y.to_bits();
            }
        }
        h
    }
}

pub struct Preview2DState {
    pub zoom: f32,           // px per mm
    pub pan: egui::Vec2,     // 画面中央からのオフセット (px)
    pub show_b_cu: bool,
    pub show_edge_cuts: bool,
    pub show_drill: bool,
    pub show_toolpaths: bool,
    pub show_machined_preview: bool,
    pub show_drc_warnings: bool,
    pub show_grid: bool,
    pub cursor_world: Option<Point2>,
    pub needs_fit: bool,
    b_cu_cache: PolyMeshCache,
    edge_cuts_cache: PolyMeshCache,
}

impl Default for Preview2DState {
    fn default() -> Self {
        Self {
            zoom: 5.0,
            pan: egui::Vec2::ZERO,
            show_b_cu: true,
            show_edge_cuts: true,
            show_drill: true,
            show_toolpaths: true,
            show_machined_preview: false,
            show_drc_warnings: true,
            show_grid: true,
            cursor_world: None,
            needs_fit: false,
            b_cu_cache: PolyMeshCache::default(),
            edge_cuts_cache: PolyMeshCache::default(),
        }
    }
}

impl Preview2DState {
    pub fn fit_bounds(&mut self, bounds: Bounds, rect: egui::Rect) {
        if !bounds.is_valid() || rect.width() <= 10.0 || rect.height() <= 10.0 {
            return;
        }
        let bw = bounds.width().max(1.0) as f32;
        let bh = bounds.height().max(1.0) as f32;
        let margin = 0.85;
        let zx = (rect.width() * margin) / bw;
        let zy = (rect.height() * margin) / bh;
        self.zoom = zx.min(zy).clamp(0.5, 500.0);

        let center = bounds.center();
        self.pan = egui::vec2(-(center.x as f32) * self.zoom, (center.y as f32) * self.zoom);
        self.needs_fit = false;
    }

    fn world_to_screen(&self, p: Point2, center: egui::Pos2) -> egui::Pos2 {
        egui::pos2(
            center.x + self.pan.x + (p.x as f32) * self.zoom,
            center.y + self.pan.y - (p.y as f32) * self.zoom,
        )
    }

    fn screen_to_world(&self, s: egui::Pos2, center: egui::Pos2) -> Point2 {
        Point2::new(
            ((s.x - center.x - self.pan.x) / self.zoom) as f64,
            ((center.y + self.pan.y - s.y) / self.zoom) as f64,
        )
    }

    pub fn ui(&mut self, ui: &mut egui::Ui, params: Preview2DParams<'_>) {
        // ツールバー
        ui.horizontal_wrapped(|ui| {
            ui.checkbox(&mut self.show_b_cu, "B_Cu");
            ui.checkbox(&mut self.show_edge_cuts, "Edge_Cuts");
            ui.checkbox(&mut self.show_drill, "Drill");
            ui.separator();
            ui.checkbox(&mut self.show_toolpaths, "ツールパス");
            ui.checkbox(&mut self.show_machined_preview, "切削後イメージ")
                .on_hover_text("ツール直径の実寸幅で削られた溝と加工後の基板イメージを表示します");
            ui.checkbox(&mut self.show_drc_warnings, "DRC警告")
                .on_hover_text("ツール径に対して狭すぎる箇所や細線の警告位置を表示します");
            ui.checkbox(&mut self.show_grid, "グリッド");
            ui.separator();
            if ui.button("全体表示").clicked() {
                self.needs_fit = true;
            }
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if let Some(c) = self.cursor_world {
                    ui.label(format!("X: {:.3} mm  Y: {:.3} mm", c.x, c.y));
                } else {
                    ui.label("X: ---  Y: ---");
                }
            });
        });
        ui.separator();

        let available = ui.available_size();
        let (rect, response) = ui.allocate_exact_size(available, egui::Sense::click_and_drag());

        if self.needs_fit {
            self.fit_bounds(params.total_bounds, rect);
        }

        let center = rect.center();

        // マウス座標追跡
        if let Some(hover_pos) = response.hover_pos() {
            self.cursor_world = Some(self.screen_to_world(hover_pos, center));
        } else {
            self.cursor_world = None;
        }

        // パン操作（ドラッグ）
        if response.dragged_by(egui::PointerButton::Primary)
            || response.dragged_by(egui::PointerButton::Middle)
        {
            self.pan += response.drag_delta();
        }

        // カーソル位置基準のズーム
        if response.hovered() {
            let scroll = ui.input(|i| i.smooth_scroll_delta.y);
            if scroll.abs() > 0.1 {
                let factor = (scroll * 0.0025).clamp(-0.3, 0.3).exp();
                if let Some(mouse_pos) = response.hover_pos() {
                    let before = self.screen_to_world(mouse_pos, center);
                    self.zoom = (self.zoom * factor).clamp(0.2, 1000.0);
                    let after_screen = self.world_to_screen(before, center);
                    self.pan += mouse_pos - after_screen;
                } else {
                    self.zoom = (self.zoom * factor).clamp(0.2, 1000.0);
                }
            }
        }

        let painter = ui.painter_at(rect);

        // 背景（落ち着いたオフホワイト）
        painter.rect_filled(rect, 0.0, egui::Color32::from_rgb(248, 249, 250));

        // グリッド描画
        if self.show_grid {
            self.draw_grid(&painter, rect, center);
        }

        // 切削後イメージモード時の基板下地（外形パスの外側まで余白を含めた生基板プレート）
        if self.show_machined_preview {
            let mut board_bounds = params.total_bounds;
            for path in params.edge_paths {
                for &pt in path {
                    board_bounds.expand_point(pt);
                }
            }
            if board_bounds.is_valid() {
                let margin = (params.edge_tool_dia * 2.5).max(3.0);
                let p_min = self.world_to_screen(
                    Point2::new(board_bounds.min_x - margin, board_bounds.max_y + margin),
                    center,
                );
                let p_max = self.world_to_screen(
                    Point2::new(board_bounds.max_x + margin, board_bounds.min_y - margin),
                    center,
                );
                let board_rect = egui::Rect::from_two_pos(p_min, p_max);
                painter.rect_filled(
                    board_rect,
                    0.0,
                    egui::Color32::from_rgba_unmultiplied(205, 135, 75, 95),
                );
            }
        }

        // 原点軸 (0,0)
        let origin_s = self.world_to_screen(Point2::new(0.0, 0.0), center);
        painter.line_segment(
            [
                egui::pos2(rect.left(), origin_s.y),
                egui::pos2(rect.right(), origin_s.y),
            ],
            egui::Stroke::new(1.0, egui::Color32::from_rgba_unmultiplied(200, 60, 60, 110)),
        );
        painter.line_segment(
            [
                egui::pos2(origin_s.x, rect.top()),
                egui::pos2(origin_s.x, rect.bottom()),
            ],
            egui::Stroke::new(1.0, egui::Color32::from_rgba_unmultiplied(60, 150, 60, 110)),
        );

        // B_Cu 描画
        if self.show_b_cu {
            if let Some(geo) = params.b_cu {
                self.b_cu_cache.update(geo);
                self.draw_geometry(
                    &painter,
                    center,
                    geo,
                    &self.b_cu_cache.polys,
                    egui::Color32::from_rgb(184, 92, 42),
                    egui::Color32::from_rgb(248, 249, 250),
                );
            }
        }

        // Edge_Cuts 描画
        if self.show_edge_cuts {
            if let Some(geo) = params.edge_cuts {
                self.edge_cuts_cache.update(geo);
                self.draw_geometry(
                    &painter,
                    center,
                    geo,
                    &self.edge_cuts_cache.polys,
                    egui::Color32::from_rgb(200, 140, 20),
                    egui::Color32::from_rgb(248, 249, 250),
                );
            }
        }

        // 切削後イメージ（実寸ツール直径幅で削られた基材露出溝・外形貫通スリット）
        if self.show_machined_preview {
            let fr4_substrate = egui::Color32::from_rgb(185, 205, 165);
            let through_slot = egui::Color32::from_rgb(65, 72, 82);
            if self.show_b_cu {
                for path in params.b_cu_paths {
                    self.draw_machined_stroke(
                        &painter,
                        center,
                        path,
                        params.b_cu_tool_dia,
                        fr4_substrate,
                    );
                }
            }
            if self.show_edge_cuts {
                for path in params.edge_paths {
                    self.draw_machined_stroke(
                        &painter,
                        center,
                        path,
                        params.edge_tool_dia,
                        through_slot,
                    );
                }
            }
            if self.show_drill {
                for path in params.drill_mill_paths {
                    self.draw_machined_stroke(
                        &painter,
                        center,
                        path,
                        params.drill_tool_dia,
                        through_slot,
                    );
                }
            }
        }

        // Drill 描画
        if self.show_drill {
            if let Some(d) = params.drill {
                let hole_fill = if self.show_machined_preview {
                    egui::Color32::from_rgb(45, 55, 65)
                } else {
                    egui::Color32::from_rgb(55, 110, 180)
                };
                let hole_stroke = egui::Stroke::new(1.0, egui::Color32::from_rgb(30, 65, 120));
                for h in &d.holes {
                    let pos = self.world_to_screen(h.position, center);
                    let r = ((h.diameter * 0.5) as f32 * self.zoom).max(1.5);
                    painter.circle(pos, r, hole_fill, hole_stroke);
                }
                for s in &d.slots {
                    let p0 = self.world_to_screen(s.start, center);
                    let p1 = self.world_to_screen(s.end, center);
                    let w = (s.diameter as f32 * self.zoom).max(2.0);
                    painter.line_segment([p0, p1], egui::Stroke::new(w, hole_fill));
                }
            }
        }

        // ツールパス描画
        if self.show_toolpaths {
            if self.show_b_cu {
                let stroke = egui::Stroke::new(1.3, egui::Color32::from_rgb(16, 130, 85));
                for path in params.b_cu_paths {
                    self.draw_polyline(&painter, center, path, stroke);
                }
            }
            if self.show_edge_cuts {
                let stroke = egui::Stroke::new(1.5, egui::Color32::from_rgb(140, 50, 160));
                for path in params.edge_paths {
                    self.draw_polyline(&painter, center, path, stroke);
                }
            }
            if self.show_drill {
                if params.drill_order.len() >= 2 {
                    let stroke = egui::Stroke::new(
                        1.0,
                        egui::Color32::from_rgba_unmultiplied(40, 120, 210, 140),
                    );
                    self.draw_polyline(&painter, center, params.drill_order, stroke);
                }
                let mill_stroke = egui::Stroke::new(1.2, egui::Color32::from_rgb(20, 130, 190));
                for path in params.drill_mill_paths {
                    self.draw_polyline(&painter, center, path, mill_stroke);
                }
            }
        }

        // DRC 警告マーカー描画
        if self.show_drc_warnings && !params.drc_violations.is_empty() {
            let warn_stroke = egui::Stroke::new(1.8, egui::Color32::from_rgb(220, 35, 35));
            let warn_fill = egui::Color32::from_rgba_unmultiplied(230, 40, 40, 55);
            let mut hovered_desc: Option<&str> = None;
            let mut best_hover_px = 18.0_f32;

            for v in params.drc_violations {
                let pos = self.world_to_screen(v.position, center);
                let r = ((v.diameter * 0.5) as f32 * self.zoom).clamp(5.0, 16.0);
                painter.circle(pos, r, warn_fill, warn_stroke);
                painter.line_segment(
                    [egui::pos2(pos.x - 4.0, pos.y), egui::pos2(pos.x + 4.0, pos.y)],
                    warn_stroke,
                );
                painter.line_segment(
                    [egui::pos2(pos.x, pos.y - 4.0), egui::pos2(pos.x, pos.y + 4.0)],
                    warn_stroke,
                );

                if let Some(mp) = response.hover_pos() {
                    let dist_px = mp.distance(pos);
                    if dist_px <= r.max(10.0) && dist_px < best_hover_px {
                        best_hover_px = dist_px;
                        hovered_desc = Some(&v.description);
                    }
                }
            }

            if let Some(desc) = hovered_desc {
                if let Some(mp) = response.hover_pos() {
                    let text_pos = egui::pos2(mp.x + 12.0, mp.y - 12.0);
                    let galley = painter.layout_no_wrap(
                        desc.to_string(),
                        egui::FontId::proportional(12.0),
                        egui::Color32::WHITE,
                    );
                    let bg_rect = egui::Rect::from_min_size(
                        text_pos - egui::vec2(4.0, 2.0),
                        galley.size() + egui::vec2(8.0, 4.0),
                    );
                    painter.rect_filled(
                        bg_rect,
                        3.0,
                        egui::Color32::from_rgba_unmultiplied(30, 30, 35, 220),
                    );
                    painter.galley(text_pos, galley, egui::Color32::WHITE);
                }
            }
        }
    }

    /// 実寸ツール幅で丸キャップ付き切削溝を描画する
    fn draw_machined_stroke(
        &self,
        painter: &egui::Painter,
        center: egui::Pos2,
        pts: &[Point2],
        tool_diameter: f64,
        color: egui::Color32,
    ) {
        if pts.len() < 2 {
            return;
        }
        let w = (tool_diameter as f32 * self.zoom).max(1.5);
        let r = w * 0.5;
        let stroke = egui::Stroke::new(w, color);
        let screen_pts: Vec<egui::Pos2> = pts
            .iter()
            .map(|&p| self.world_to_screen(p, center))
            .collect();
        painter.add(egui::Shape::line(screen_pts.clone(), stroke));
        if r > 1.2 {
            for p in screen_pts {
                painter.circle_filled(p, r, color);
            }
        }
    }

    fn draw_grid(&self, painter: &egui::Painter, rect: egui::Rect, center: egui::Pos2) {
        // 画面上で約40px以上の間隔になるような10のべき乗/5の倍数ステップを選ぶ
        let min_world_step = (40.0 / self.zoom) as f64;
        let candidates = [0.1, 0.5, 1.0, 2.0, 5.0, 10.0, 20.0, 50.0, 100.0];
        let step = candidates
            .iter()
            .copied()
            .find(|&s| s >= min_world_step)
            .unwrap_or(100.0);

        let tl = self.screen_to_world(rect.left_top(), center);
        let br = self.screen_to_world(rect.right_bottom(), center);

        let min_x = tl.x.min(br.x);
        let max_x = tl.x.max(br.x);
        let min_y = tl.y.min(br.y);
        let max_y = tl.y.max(br.y);

        let grid_color = egui::Color32::from_rgb(225, 228, 232);
        let stroke = egui::Stroke::new(1.0, grid_color);

        let start_x = (min_x / step).floor() as i64;
        let end_x = (max_x / step).ceil() as i64;
        for ix in start_x..=end_x {
            let wx = (ix as f64) * step;
            let sx = self.world_to_screen(Point2::new(wx, 0.0), center).x;
            painter.line_segment([egui::pos2(sx, rect.top()), egui::pos2(sx, rect.bottom())], stroke);
        }

        let start_y = (min_y / step).floor() as i64;
        let end_y = (max_y / step).ceil() as i64;
        for iy in start_y..=end_y {
            let wy = (iy as f64) * step;
            let sy = self.world_to_screen(Point2::new(0.0, wy), center).y;
            painter.line_segment([egui::pos2(rect.left(), sy), egui::pos2(rect.right(), sy)], stroke);
        }
    }

    fn draw_geometry(
        &self,
        painter: &egui::Painter,
        center: egui::Pos2,
        geo: &Geometry,
        cached_polys: &[CachedPoly],
        dark_color: egui::Color32,
        clear_color: egui::Color32,
    ) {
        // ポリゴン描画（外周線＋半透明塗りつぶし）
        for poly in cached_polys {
            if poly.points.len() < 3 {
                continue;
            }
            let base = if poly.polarity == Polarity::Dark {
                dark_color
            } else {
                clear_color
            };
            let pts: Vec<egui::Pos2> = poly
                .points
                .iter()
                .map(|&p| self.world_to_screen(p, center))
                .collect();
            let fill = egui::Color32::from_rgba_unmultiplied(base.r(), base.g(), base.b(), 120);
            if !poly.indices.is_empty() {
                let mut mesh = egui::Mesh::default();
                mesh.vertices = pts
                    .iter()
                    .map(|&pos| egui::epaint::Vertex {
                        pos,
                        uv: egui::epaint::WHITE_UV,
                        color: fill,
                    })
                    .collect();
                mesh.indices = poly.indices.clone();
                painter.add(egui::Shape::mesh(mesh));
            }
            painter.add(egui::Shape::closed_line(pts, egui::Stroke::new(1.0, base)));
        }

        // 線分（端点に丸キャップを描画して接続部の欠けを防ぐ）
        for seg in &geo.segments {
            let color = if seg.polarity == Polarity::Dark {
                dark_color
            } else {
                clear_color
            };
            let p0 = self.world_to_screen(seg.start, center);
            let p1 = self.world_to_screen(seg.end, center);
            let w = (seg.width as f32 * self.zoom).max(1.2);
            painter.line_segment([p0, p1], egui::Stroke::new(w, color));
            if w > 2.0 {
                let r = w * 0.5;
                painter.circle_filled(p0, r, color);
                painter.circle_filled(p1, r, color);
            }
        }

        // 円弧
        for arc in &geo.arcs {
            let color = if arc.polarity == Polarity::Dark {
                dark_color
            } else {
                clear_color
            };
            let pts = GerberParser::sample_arc_points(
                arc.start,
                arc.end,
                arc.center,
                arc.clockwise,
                0.1,
            );
            let w = (arc.width as f32 * self.zoom).max(1.2);
            self.draw_polyline(painter, center, &pts, egui::Stroke::new(w, color));
        }

        // 円
        for c in &geo.circles {
            let color = if c.polarity == Polarity::Dark {
                dark_color
            } else {
                clear_color
            };
            let pos = self.world_to_screen(c.center, center);
            let r = (c.radius as f32 * self.zoom).max(1.2);
            painter.circle_filled(pos, r, color);
        }
    }

    fn draw_polyline(
        &self,
        painter: &egui::Painter,
        center: egui::Pos2,
        pts: &[Point2],
        stroke: egui::Stroke,
    ) {
        if pts.len() < 2 {
            return;
        }
        let screen_pts: Vec<egui::Pos2> = pts
            .iter()
            .map(|&p| self.world_to_screen(p, center))
            .collect();
        painter.add(egui::Shape::line(screen_pts, stroke));
    }
}
