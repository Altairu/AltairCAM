//! Gコード 切削シミュレーター

use eframe::egui;
use std::fs;

use crate::core::geometry::{Bounds, Point2};
use crate::gcode::generator::{parse_gcode_for_sim, GCodeStats, MoveKind, SimSegment};

pub struct SimulatorState {
    pub segments: Vec<SimSegment>,
    pub stats: GCodeStats,
    pub source_label: String,
    pub playing: bool,
    pub current_step: usize,
    pub speed_steps_per_sec: f32,
    pub step_accumulator: f32,
    pub show_rapid: bool,
    pub zoom: f32,
    pub pan: egui::Vec2,
    pub bounds: Bounds,
    pub needs_fit: bool,
}

impl Default for SimulatorState {
    fn default() -> Self {
        Self {
            segments: Vec::new(),
            stats: GCodeStats::default(),
            source_label: "未読込".to_string(),
            playing: false,
            current_step: 0,
            speed_steps_per_sec: 40.0,
            step_accumulator: 0.0,
            show_rapid: true,
            zoom: 5.0,
            pan: egui::Vec2::ZERO,
            bounds: Bounds::empty(),
            needs_fit: false,
        }
    }
}

impl SimulatorState {
    pub fn load_gcode_lines(&mut self, label: &str, lines: &[String]) {
        let (segs, stats) = parse_gcode_for_sim(lines);
        let mut b = Bounds::empty();
        for s in &segs {
            b.expand_point(Point2::new(s.start[0], s.start[1]));
            b.expand_point(Point2::new(s.end[0], s.end[1]));
        }
        self.segments = segs;
        self.stats = stats;
        self.source_label = label.to_string();
        self.playing = false;
        self.current_step = self.segments.len();
        self.step_accumulator = 0.0;
        self.bounds = b;
        self.needs_fit = true;
    }

    pub fn load_from_file(&mut self) -> Result<(), String> {
        if let Some(path) = rfd::FileDialog::new()
            .add_filter("G-Code", &["nc", "gcode", "tap", "txt"])
            .pick_file()
        {
            let content = fs::read_to_string(&path)
                .map_err(|e| format!("Gコードの読み込みに失敗しました: {}", e))?;
            let lines: Vec<String> = content.lines().map(|l| l.to_string()).collect();
            let name = path
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("ファイル");
            self.load_gcode_lines(name, &lines);
        }
        Ok(())
    }

    fn fit(&mut self, rect: egui::Rect) {
        if !self.bounds.is_valid() || rect.width() <= 10.0 || rect.height() <= 10.0 {
            return;
        }
        let bw = self.bounds.width().max(1.0) as f32;
        let bh = self.bounds.height().max(1.0) as f32;
        let zx = (rect.width() * 0.85) / bw;
        let zy = (rect.height() * 0.85) / bh;
        self.zoom = zx.min(zy).clamp(0.5, 500.0);
        let c = self.bounds.center();
        self.pan = egui::vec2(-(c.x as f32) * self.zoom, (c.y as f32) * self.zoom);
        self.needs_fit = false;
    }

    fn world_to_screen(&self, x: f64, y: f64, center: egui::Pos2) -> egui::Pos2 {
        egui::pos2(
            center.x + self.pan.x + (x as f32) * self.zoom,
            center.y + self.pan.y - (y as f32) * self.zoom,
        )
    }

    pub fn ui(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        // コントロールバー
        ui.horizontal(|ui| {
            ui.label(format!("対象: {}", self.source_label));
            ui.separator();

            if self.playing {
                if ui.button("一時停止").clicked() {
                    self.playing = false;
                }
            } else if ui.button("再生").clicked() {
                if self.current_step >= self.segments.len() {
                    self.current_step = 0;
                }
                self.playing = true;
            }

            if ui.button("最初へ").clicked() {
                self.playing = false;
                self.current_step = 0;
            }
            if ui.button("最後へ").clicked() {
                self.playing = false;
                self.current_step = self.segments.len();
            }

            ui.separator();
            ui.label("速度:");
            ui.add(egui::Slider::new(&mut self.speed_steps_per_sec, 5.0..=500.0).logarithmic(true));
            ui.checkbox(&mut self.show_rapid, "早送り線を表示");

            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui.button("Gコードファイルを開く...").clicked() {
                    let _ = self.load_from_file();
                }
            });
        });

        // 進捗バーと統計
        let max_step = self.segments.len();
        ui.horizontal(|ui| {
            ui.label("進捗:");
            let mut step_val = self.current_step;
            if ui
                .add(egui::Slider::new(&mut step_val, 0..=max_step))
                .changed()
            {
                self.current_step = step_val;
                self.playing = false;
            }
            ui.separator();
            ui.label(format!(
                "切削: {:.1} mm | 早送り: {:.1} mm | 推定時間: {:.1} 分",
                self.stats.cut_distance_mm,
                self.stats.rapid_distance_mm,
                self.stats.estimated_minutes
            ));
        });

        // ツール現在座標表示
        if self.current_step > 0 && self.current_step <= self.segments.len() {
            let p = self.segments[self.current_step - 1].end;
            ui.label(format!(
                "ツール位置: X={:.3} mm  Y={:.3} mm  Z={:.3} mm",
                p[0], p[1], p[2]
            ));
        } else {
            ui.label("ツール位置: X=0.000 mm  Y=0.000 mm  Z=0.000 mm");
        }

        ui.separator();

        // アニメーション更新
        if self.playing && !self.segments.is_empty() {
            let dt = ctx.input(|i| i.stable_dt).min(0.1);
            self.step_accumulator += dt * self.speed_steps_per_sec;
            let advance = self.step_accumulator.floor() as usize;
            if advance > 0 {
                self.step_accumulator -= advance as f32;
                self.current_step = (self.current_step + advance).min(self.segments.len());
                if self.current_step >= self.segments.len() {
                    self.playing = false;
                }
            }
            ctx.request_repaint();
        }

        // キャンバス描画
        let available = ui.available_size();
        let (rect, response) = ui.allocate_exact_size(available, egui::Sense::click_and_drag());
        if self.needs_fit {
            self.fit(rect);
        }

        if response.dragged_by(egui::PointerButton::Primary)
            || response.dragged_by(egui::PointerButton::Middle)
        {
            self.pan += response.drag_delta();
        }
        if response.hovered() {
            let scroll = ui.input(|i| i.smooth_scroll_delta.y);
            if scroll.abs() > 0.1 {
                let factor = (scroll * 0.0025).clamp(-0.3, 0.3).exp();
                self.zoom = (self.zoom * factor).clamp(0.2, 1000.0);
            }
        }

        let painter = ui.painter_at(rect);
        painter.rect_filled(rect, 0.0, egui::Color32::from_rgb(248, 249, 250));

        let center = rect.center();
        let rapid_stroke = egui::Stroke::new(
            1.0,
            egui::Color32::from_rgba_unmultiplied(210, 80, 80, 110),
        );
        let cut_stroke = egui::Stroke::new(1.5, egui::Color32::from_rgb(30, 100, 200));
        let future_stroke = egui::Stroke::new(1.0, egui::Color32::from_rgb(215, 218, 222));

        // 未実行の切削パスを薄く背景描画
        for seg in self.segments.iter().skip(self.current_step) {
            if seg.kind == MoveKind::Cut {
                let p0 = self.world_to_screen(seg.start[0], seg.start[1], center);
                let p1 = self.world_to_screen(seg.end[0], seg.end[1], center);
                painter.line_segment([p0, p1], future_stroke);
            }
        }

        // 実行済みパス描画
        for seg in self.segments.iter().take(self.current_step) {
            let p0 = self.world_to_screen(seg.start[0], seg.start[1], center);
            let p1 = self.world_to_screen(seg.end[0], seg.end[1], center);
            match seg.kind {
                MoveKind::Rapid => {
                    if self.show_rapid {
                        painter.line_segment([p0, p1], rapid_stroke);
                    }
                }
                MoveKind::Cut => {
                    painter.line_segment([p0, p1], cut_stroke);
                }
                MoveKind::Plunge => {
                    painter.circle_filled(p1, 2.0, egui::Color32::from_rgb(180, 60, 20));
                }
            }
        }

        // 現在ツールヘッド位置
        if self.current_step > 0 && self.current_step <= self.segments.len() {
            let seg = &self.segments[self.current_step - 1];
            let pos = self.world_to_screen(seg.end[0], seg.end[1], center);
            let color = if seg.end[2] < 0.0 {
                egui::Color32::from_rgb(220, 40, 40)
            } else {
                egui::Color32::from_rgb(40, 160, 60)
            };
            painter.circle_stroke(pos, 5.0, egui::Stroke::new(2.0, color));
        }
    }
}
