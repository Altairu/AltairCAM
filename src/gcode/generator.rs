//! Gコード生成および解析モジュール

use std::fs;
use std::path::Path;

use crate::core::geometry::{DrillHole, DrillSlot, Point2, Toolpath};

#[derive(Debug, Clone)]
pub struct GCodeSettings {
    pub feed_rate: f64,       // 切削送り速度 (mm/min)
    pub rapid_feed_rate: f64, // 早送り速度 (mm/min)
    pub plunge_feed_rate: f64,// Z軸下降速度 (mm/min)
    pub travel_z: f64,        // 移動高さ (mm)
    pub safe_z: f64,          // 安全高さ (mm)
    pub cut_z: f64,           // 目標切削深さ (mm, 負値)
    pub pass_depth: f64,      // 1パスあたりの最大切り込み深さ (mm, 正値, 0なら1回で切削)
    pub spindle_speed: u32,   // スピンドル回転数 (rpm)
}

impl Default for GCodeSettings {
    fn default() -> Self {
        Self {
            feed_rate: 100.0,
            rapid_feed_rate: 500.0,
            plunge_feed_rate: 50.0,
            travel_z: 2.0,
            safe_z: 5.0,
            cut_z: -0.05,
            pass_depth: 0.0,
            spindle_speed: 10000,
        }
    }
}

/// 生成統計情報
#[derive(Debug, Clone, Default)]
pub struct GCodeStats {
    #[allow(dead_code)]
    pub line_count: usize,
    pub cut_distance_mm: f64,
    pub rapid_distance_mm: f64,
    pub estimated_minutes: f64,
}

pub struct GCodeGenerator {
    pub settings: GCodeSettings,
}

impl GCodeGenerator {
    pub fn new(settings: GCodeSettings) -> Self {
        Self { settings }
    }

    pub fn generate_header(&self) -> Vec<String> {
        vec![
            "(AltairCAM Generated G-Code)".to_string(),
            "G21 (Metric units)".to_string(),
            "G90 (Absolute positioning)".to_string(),
            format!("M3 S{} (Start spindle)", self.settings.spindle_speed),
            format!("G0 Z{:.3} (Move to safe height)", self.settings.safe_z),
            String::new(),
        ]
    }

    pub fn generate_footer(&self) -> Vec<String> {
        vec![
            String::new(),
            format!("G0 Z{:.3} (Move to safe height)", self.settings.safe_z),
            "M5 (Stop spindle)".to_string(),
            "M2 (End program)".to_string(),
            String::new(),
        ]
    }

    fn format_move(&self, x: f64, y: f64, z: Option<f64>, feed: bool, plunge: bool) -> String {
        let cmd = if feed { "G1" } else { "G0" };
        let z_part = match z {
            Some(zv) => format!(" Z{:.3}", zv),
            None => String::new(),
        };
        let f_val = if feed {
            if plunge {
                self.settings.plunge_feed_rate
            } else {
                self.settings.feed_rate
            }
        } else {
            self.settings.rapid_feed_rate
        };
        format!("{} X{:.3} Y{:.3}{} F{:.1}", cmd, x, y, z_part, f_val)
    }

    /// Zステップダウンの深さリストを計算する
    fn z_passes(&self) -> Vec<f64> {
        let target = self.settings.cut_z;
        let step = self.settings.pass_depth;
        if step <= 1e-6 || target >= 0.0 || step >= target.abs() {
            return vec![target];
        }
        let mut depths = Vec::new();
        let mut current = 0.0;
        while current > target + 1e-6 {
            current = (current - step).max(target);
            depths.push(current);
        }
        if depths.is_empty() {
            depths.push(target);
        }
        depths
    }

    /// 連続ツールパスからGコードを生成（マルチパスZステップダウン対応）
    pub fn generate_paths_gcode(&self, paths: &[Toolpath], connect_tolerance: f64) -> Vec<String> {
        let mut lines = self.generate_header();
        if paths.is_empty() {
            lines.extend(self.generate_footer());
            return lines;
        }

        let z_levels = self.z_passes();
        let total_passes = z_levels.len();

        for (pass_idx, &z_cut) in z_levels.iter().enumerate() {
            if total_passes > 1 {
                lines.push(format!(
                    "(Pass {}/{} - Z={:.3})",
                    pass_idx + 1,
                    total_passes,
                    z_cut
                ));
            } else {
                lines.push("(Cutting paths)".to_string());
            }

            let mut prev_end: Option<Point2> = None;
            let mut in_cut = false;

            for path in paths {
                if path.is_empty() {
                    continue;
                }
                let start = path[0];

                match prev_end {
                    Some(pe) if pe.distance_to(&start) <= connect_tolerance => {
                        // 連続しているのでZを上げずにそのまま切削続行
                    }
                    Some(pe) => {
                        lines.push(self.format_move(
                            pe.x,
                            pe.y,
                            Some(self.settings.travel_z),
                            false,
                            false,
                        ));
                        lines.push(self.format_move(
                            start.x,
                            start.y,
                            Some(self.settings.travel_z),
                            false,
                            false,
                        ));
                        lines.push(self.format_move(start.x, start.y, Some(z_cut), true, true));
                        in_cut = true;
                    }
                    None => {
                        lines.push(self.format_move(
                            start.x,
                            start.y,
                            Some(self.settings.travel_z),
                            false,
                            false,
                        ));
                        lines.push(self.format_move(start.x, start.y, Some(z_cut), true, true));
                        in_cut = true;
                    }
                }

                for pt in &path[1..] {
                    lines.push(self.format_move(pt.x, pt.y, Some(z_cut), true, false));
                }

                prev_end = Some(*path.last().unwrap());
            }

            if let Some(pe) = prev_end {
                if in_cut {
                    lines.push(self.format_move(
                        pe.x,
                        pe.y,
                        Some(self.settings.travel_z),
                        false,
                        false,
                    ));
                }
            }
        }

        lines.extend(self.generate_footer());
        lines
    }

    /// ドリル穴と長穴からGコードを生成
    pub fn generate_drill_gcode(&self, holes: &[DrillHole], slots: &[DrillSlot]) -> Vec<String> {
        let mut lines = self.generate_header();
        let z_levels = self.z_passes();

        if !holes.is_empty() {
            lines.push("(Drill holes)".to_string());
            for hole in holes {
                lines.push(self.format_move(
                    hole.position.x,
                    hole.position.y,
                    Some(self.settings.travel_z),
                    false,
                    false,
                ));
                // ペックドリル（pass_depthが設定されていればステップ加工）
                for &z_cut in &z_levels {
                    lines.push(self.format_move(
                        hole.position.x,
                        hole.position.y,
                        Some(z_cut),
                        true,
                        true,
                    ));
                    if z_levels.len() > 1 {
                        lines.push(self.format_move(
                            hole.position.x,
                            hole.position.y,
                            Some(self.settings.travel_z),
                            false,
                            false,
                        ));
                    }
                }
                if z_levels.len() == 1 {
                    lines.push(self.format_move(
                        hole.position.x,
                        hole.position.y,
                        Some(self.settings.travel_z),
                        false,
                        false,
                    ));
                }
            }
        }

        if !slots.is_empty() {
            lines.push("(Drill slots)".to_string());
            for slot in slots {
                for &z_cut in &z_levels {
                    lines.push(self.format_move(
                        slot.start.x,
                        slot.start.y,
                        Some(self.settings.travel_z),
                        false,
                        false,
                    ));
                    lines.push(self.format_move(
                        slot.start.x,
                        slot.start.y,
                        Some(z_cut),
                        true,
                        true,
                    ));
                    lines.push(self.format_move(
                        slot.end.x,
                        slot.end.y,
                        Some(z_cut),
                        true,
                        false,
                    ));
                    lines.push(self.format_move(
                        slot.end.x,
                        slot.end.y,
                        Some(self.settings.travel_z),
                        false,
                        false,
                    ));
                }
            }
        }

        lines.extend(self.generate_footer());
        lines
    }

    pub fn save_to_file<P: AsRef<Path>>(lines: &[String], path: P) -> Result<(), String> {
        fs::write(path.as_ref(), lines.join("\n"))
            .map_err(|e| format!("Gコードの保存に失敗しました: {}", e))
    }
}

/// シミュレーター用のGコード移動ステップ
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MoveKind {
    Rapid,
    Cut,
    Plunge,
}

#[derive(Debug, Clone, Copy)]
pub struct SimSegment {
    pub start: [f64; 3],
    pub end: [f64; 3],
    pub kind: MoveKind,
}

/// Gコード行リストを解析してシミュレーション用セグメントと統計を返す
pub fn parse_gcode_for_sim(lines: &[String]) -> (Vec<SimSegment>, GCodeStats) {
    let mut segs = Vec::new();
    let mut cx = 0.0_f64;
    let mut cy = 0.0_f64;
    let mut cz = 0.0_f64;
    let mut cut_dist = 0.0_f64;
    let mut rapid_dist = 0.0_f64;
    let mut est_min = 0.0_f64;
    let mut last_feed = 100.0_f64;
    let mut last_rapid_feed = 500.0_f64;
    let mut is_rapid = true;

    for raw in lines {
        let line = raw.trim();
        if line.is_empty() || line.starts_with('(') || line.starts_with(';') {
            continue;
        }
        // インラインコメント除去
        let code_part = line.split('(').next().unwrap_or(line).trim();

        if code_part.contains("G00") || code_part.contains("G0 ") || code_part == "G0" {
            is_rapid = true;
        } else if code_part.contains("G01") || code_part.contains("G1 ") || code_part == "G1" {
            is_rapid = false;
        }

        let nx = extract_gcode_val(code_part, 'X').unwrap_or(cx);
        let ny = extract_gcode_val(code_part, 'Y').unwrap_or(cy);
        let nz = extract_gcode_val(code_part, 'Z').unwrap_or(cz);
        if let Some(f) = extract_gcode_val(code_part, 'F') {
            if f > 0.0 {
                if is_rapid {
                    last_rapid_feed = f;
                } else {
                    last_feed = f;
                }
            }
        }

        if (nx - cx).abs() > 1e-6 || (ny - cy).abs() > 1e-6 || (nz - cz).abs() > 1e-6 {
            let dx = nx - cx;
            let dy = ny - cy;
            let dz = nz - cz;
            let dist = (dx * dx + dy * dy + dz * dz).sqrt();
            let kind = if is_rapid {
                rapid_dist += dist;
                est_min += dist / last_rapid_feed.max(1.0);
                MoveKind::Rapid
            } else if dz < -1e-6 && dx.abs() < 1e-6 && dy.abs() < 1e-6 {
                cut_dist += dist;
                est_min += dist / last_feed.max(1.0);
                MoveKind::Plunge
            } else {
                cut_dist += dist;
                est_min += dist / last_feed.max(1.0);
                MoveKind::Cut
            };

            segs.push(SimSegment {
                start: [cx, cy, cz],
                end: [nx, ny, nz],
                kind,
            });
            cx = nx;
            cy = ny;
            cz = nz;
        }
    }

    let stats = GCodeStats {
        line_count: lines.len(),
        cut_distance_mm: cut_dist,
        rapid_distance_mm: rapid_dist,
        estimated_minutes: est_min,
    };

    (segs, stats)
}

fn extract_gcode_val(line: &str, key: char) -> Option<f64> {
    let idx = line.find(key)?;
    let s: String = line[idx + 1..]
        .chars()
        .take_while(|c| c.is_ascii_digit() || *c == '.' || *c == '+' || *c == '-')
        .collect();
    s.parse::<f64>().ok()
}
