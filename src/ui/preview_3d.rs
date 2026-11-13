//! 3D プレビューキャンバス（軽量ソフトウェア投影）

use eframe::egui;

use crate::core::geometry::{Bounds, DrillData, Geometry, Point2, Toolpath};
use crate::core::gerber::GerberParser;

pub struct Preview3DState {
    pub yaw: f32,
    pub pitch: f32,
    pub zoom: f32,
    pub pan: egui::Vec2,
    pub show_toolpaths: bool,
    pub z_scale: f32,
}

impl Default for Preview3DState {
    fn default() -> Self {
        Self {
            yaw: -0.55,
            pitch: 0.75,
            zoom: 5.0,
            pan: egui::Vec2::ZERO,
            show_toolpaths: true,
            z_scale: 3.0, // Z方向の視認用倍率
        }
    }
}

impl Preview3DState {
    fn project(
        &self,
        x: f64,
        y: f64,
        z: f64,
        center_world: Point2,
        screen_center: egui::Pos2,
    ) -> egui::Pos2 {
        let px = (x - center_world.x) as f32;
        let py = (y - center_world.y) as f32;
        let pz = (z as f32) * self.z_scale;

        let (sy, cy) = self.yaw.sin_cos();
        let (sp, cp) = self.pitch.sin_cos();

        // Z軸回りのYaw回転
        let rx = px * cy - py * sy;
        let ry = px * sy + py * cy;

        // X軸回りのPitch回転
        let ry2 = ry * cp - pz * sp;
        let rz2 = ry * sp + pz * cp;
        let _ = rz2;

        egui::pos2(
            screen_center.x + self.pan.x + rx * self.zoom,
            screen_center.y + self.pan.y - ry2 * self.zoom,
        )
    }

    pub fn fit(&mut self, bounds: Bounds, rect: egui::Rect) {
        if !bounds.is_valid() {
            return;
        }
        let max_dim = bounds.width().max(bounds.height()).max(1.0) as f32;
        self.zoom = (rect.width().min(rect.height()) * 0.65 / max_dim).clamp(0.5, 200.0);
        self.pan = egui::Vec2::ZERO;
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
        b_cu_depth: f64,
        edge_depth: f64,
        drill_depth: f64,
        total_bounds: Bounds,
    ) {
        ui.horizontal(|ui| {
            ui.checkbox(&mut self.show_toolpaths, "ツールパス表示");
            ui.separator();
            ui.label("Z強調:");
            ui.add(egui::Slider::new(&mut self.z_scale, 1.0..=10.0).fixed_decimals(1));
            ui.separator();
            if ui.button("視点リセット").clicked() {
                self.yaw = -0.55;
                self.pitch = 0.75;
                self.pan = egui::Vec2::ZERO;
                let rect = ui.available_rect_before_wrap();
                self.fit(total_bounds, rect);
            }
            ui.label("左ドラッグ: 回転 / 右ドラッグ: 移動 / ホイール: ズーム");
        });
        ui.separator();

        let available = ui.available_size();
        let (rect, response) = ui.allocate_exact_size(available, egui::Sense::click_and_drag());

        if response.dragged_by(egui::PointerButton::Primary) {
            let d = response.drag_delta();
            self.yaw += d.x * 0.005;
            self.pitch = (self.pitch + d.y * 0.005).clamp(-1.55, 1.55);
        }
        if response.dragged_by(egui::PointerButton::Secondary)
            || response.dragged_by(egui::PointerButton::Middle)
        {
            self.pan += response.drag_delta();
        }
        if response.hovered() {
            let scroll = ui.input(|i| i.smooth_scroll_delta.y);
            if scroll.abs() > 0.1 {
                let factor = (scroll * 0.0025).clamp(-0.3, 0.3).exp();
                self.zoom = (self.zoom * factor).clamp(0.3, 400.0);
            }
        }

        let painter = ui.painter_at(rect);
        painter.rect_filled(rect, 0.0, egui::Color32::from_rgb(245, 246, 248));

        let cw = total_bounds.center();
        let sc = rect.center();

        // 基板プレート外形 (バウンディングボックス)
        if total_bounds.is_valid() {
            let corners = [
                self.project(total_bounds.min_x, total_bounds.min_y, 0.0, cw, sc),
                self.project(total_bounds.max_x, total_bounds.min_y, 0.0, cw, sc),
                self.project(total_bounds.max_x, total_bounds.max_y, 0.0, cw, sc),
                self.project(total_bounds.min_x, total_bounds.max_y, 0.0, cw, sc),
            ];
            let thickness = -1.6_f64;
            let b_corners = [
                self.project(total_bounds.min_x, total_bounds.min_y, thickness, cw, sc),
                self.project(total_bounds.max_x, total_bounds.min_y, thickness, cw, sc),
                self.project(total_bounds.max_x, total_bounds.max_y, thickness, cw, sc),
                self.project(total_bounds.min_x, total_bounds.max_y, thickness, cw, sc),
            ];

            let board_stroke = egui::Stroke::new(1.0, egui::Color32::from_rgb(160, 175, 160));
            for i in 0..4 {
                let next = (i + 1) % 4;
                painter.line_segment([corners[i], corners[next]], board_stroke);
                painter.line_segment([b_corners[i], b_corners[next]], board_stroke);
                painter.line_segment([corners[i], b_corners[i]], board_stroke);
            }
        }

        // 銅箔パターン (Z = 0)
        if let Some(geo) = b_cu {
            let stroke = egui::Stroke::new(1.1, egui::Color32::from_rgb(184, 92, 42));
            for seg in &geo.segments {
                let p0 = self.project(seg.start.x, seg.start.y, 0.0, cw, sc);
                let p1 = self.project(seg.end.x, seg.end.y, 0.0, cw, sc);
                painter.line_segment([p0, p1], stroke);
            }
            for arc in &geo.arcs {
                let pts = GerberParser::sample_arc_points(
                    arc.start,
                    arc.end,
                    arc.center,
                    arc.clockwise,
                    0.15,
                );
                let screen_pts: Vec<egui::Pos2> = pts
                    .iter()
                    .map(|p| self.project(p.x, p.y, 0.0, cw, sc))
                    .collect();
                if screen_pts.len() >= 2 {
                    painter.add(egui::Shape::line(screen_pts, stroke));
                }
            }
            for c in &geo.circles {
                let p = self.project(c.center.x, c.center.y, 0.0, cw, sc);
                painter.circle_filled(p, (c.radius as f32 * self.zoom).max(1.2), stroke.color);
            }
        }

        // 外形 (Z = 0)
        if let Some(geo) = edge_cuts {
            let stroke = egui::Stroke::new(1.4, egui::Color32::from_rgb(200, 140, 20));
            for seg in &geo.segments {
                let p0 = self.project(seg.start.x, seg.start.y, 0.0, cw, sc);
                let p1 = self.project(seg.end.x, seg.end.y, 0.0, cw, sc);
                painter.line_segment([p0, p1], stroke);
            }
        }

        // ドリル穴 (Z=0 から drill_depth への縦線)
        if let Some(d) = drill {
            let stroke = egui::Stroke::new(1.5, egui::Color32::from_rgb(55, 110, 180));
            for h in &d.holes {
                let top = self.project(h.position.x, h.position.y, 0.0, cw, sc);
                let bot = self.project(h.position.x, h.position.y, drill_depth, cw, sc);
                painter.line_segment([top, bot], stroke);
                painter.circle_filled(top, 2.0, stroke.color);
            }
        }

        // 生成ツールパス
        if self.show_toolpaths {
            let b_stroke = egui::Stroke::new(1.2, egui::Color32::from_rgb(16, 130, 85));
            for path in b_cu_paths {
                let pts: Vec<egui::Pos2> = path
                    .iter()
                    .map(|p| self.project(p.x, p.y, b_cu_depth, cw, sc))
                    .collect();
                if pts.len() >= 2 {
                    painter.add(egui::Shape::line(pts, b_stroke));
                }
            }

            let e_stroke = egui::Stroke::new(1.4, egui::Color32::from_rgb(140, 50, 160));
            for path in edge_paths {
                let pts: Vec<egui::Pos2> = path
                    .iter()
                    .map(|p| self.project(p.x, p.y, edge_depth, cw, sc))
                    .collect();
                if pts.len() >= 2 {
                    painter.add(egui::Shape::line(pts, e_stroke));
                }
            }
        }
    }
}
