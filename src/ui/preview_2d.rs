//! 2D プレビューキャンバス

use eframe::egui;

use crate::core::geometry::{Bounds, DrillData, Geometry, Point2, Polarity, Toolpath};
use crate::core::gerber::GerberParser;

pub struct Preview2DState {
    pub zoom: f32,           // px per mm
    pub pan: egui::Vec2,     // 画面中央からのオフセット (px)
    pub show_b_cu: bool,
    pub show_edge_cuts: bool,
    pub show_drill: bool,
    pub show_toolpaths: bool,
    pub show_grid: bool,
    pub cursor_world: Option<Point2>,
    pub needs_fit: bool,
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
            show_grid: true,
            cursor_world: None,
            needs_fit: false,
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

    #[allow(clippy::too_many_arguments)]
    pub fn ui(
        &mut self,
        ui: &mut egui::Ui,
        b_cu: Option<&Geometry>,
        edge_cuts: Option<&Geometry>,
        drill: Option<&DrillData>,
        b_cu_paths: &[Toolpath],
        edge_paths: &[Toolpath],
        drill_order: &[Point2],
        total_bounds: Bounds,
    ) {
        // ツールバー
        ui.horizontal(|ui| {
            ui.checkbox(&mut self.show_b_cu, "B_Cu");
            ui.checkbox(&mut self.show_edge_cuts, "Edge_Cuts");
            ui.checkbox(&mut self.show_drill, "Drill");
            ui.separator();
            ui.checkbox(&mut self.show_toolpaths, "ツールパス");
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
            self.fit_bounds(total_bounds, rect);
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

        // 原点軸 (0,0)
        let origin_s = self.world_to_screen(Point2::new(0.0, 0.0), center);
        if rect.contains(origin_s) || true {
            painter.line_segment(
                [egui::pos2(rect.left(), origin_s.y), egui::pos2(rect.right(), origin_s.y)],
                egui::Stroke::new(1.0, egui::Color32::from_rgba_unmultiplied(200, 60, 60, 110)),
            );
            painter.line_segment(
                [egui::pos2(origin_s.x, rect.top()), egui::pos2(origin_s.x, rect.bottom())],
                egui::Stroke::new(1.0, egui::Color32::from_rgba_unmultiplied(60, 150, 60, 110)),
            );
        }

        // B_Cu 描画
        if self.show_b_cu {
            if let Some(geo) = b_cu {
                self.draw_geometry(
                    &painter,
                    center,
                    geo,
                    egui::Color32::from_rgb(184, 92, 42),
                    egui::Color32::from_rgb(248, 249, 250),
                );
            }
        }

        // Edge_Cuts 描画
        if self.show_edge_cuts {
            if let Some(geo) = edge_cuts {
                self.draw_geometry(
                    &painter,
                    center,
                    geo,
                    egui::Color32::from_rgb(200, 140, 20),
                    egui::Color32::from_rgb(248, 249, 250),
                );
            }
        }

        // Drill 描画
        if self.show_drill {
            if let Some(d) = drill {
                let hole_fill = egui::Color32::from_rgb(55, 110, 180);
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
                for path in b_cu_paths {
                    self.draw_polyline(&painter, center, path, stroke);
                }
            }
            if self.show_edge_cuts {
                let stroke = egui::Stroke::new(1.5, egui::Color32::from_rgb(140, 50, 160));
                for path in edge_paths {
                    self.draw_polyline(&painter, center, path, stroke);
                }
            }
            if self.show_drill && drill_order.len() >= 2 {
                let stroke = egui::Stroke::new(
                    1.0,
                    egui::Color32::from_rgba_unmultiplied(40, 120, 210, 140),
                );
                self.draw_polyline(&painter, center, drill_order, stroke);
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
        dark_color: egui::Color32,
        clear_color: egui::Color32,
    ) {
        // ポリゴン描画（外周線＋半透明塗りつぶし）
        for poly in &geo.polygons {
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
            let fill = egui::Color32::from_rgba_unmultiplied(base.r(), base.g(), base.b(), 140);
            if pts.len() <= 4 {
                painter.add(egui::Shape::convex_polygon(
                    pts,
                    fill,
                    egui::Stroke::new(1.0, base),
                ));
            } else {
                painter.add(egui::Shape::closed_line(pts, egui::Stroke::new(1.2, base)));
            }
        }

        // 線分
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
