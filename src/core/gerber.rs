//! RS-274X ガーバーファイルパーサー

use std::collections::HashMap;
use std::f64::consts::PI;
use std::fs;
use std::path::Path;

use crate::core::geometry::{
    ArcSegment, CircleShape, Geometry, Point2, Polarity, PolygonShape, Segment,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ZeroSuppression {
    Leading,
    Trailing,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Notation {
    Absolute,
    Incremental,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Unit {
    Mm,
    Inch,
}

#[derive(Debug, Clone)]
enum Aperture {
    Circle {
        diameter: f64,
    },
    Rectangle {
        width: f64,
        height: f64,
    },
    Obround {
        width: f64,
        height: f64,
    },
    Polygon {
        outer_diameter: f64,
        vertices: usize,
        rotation_deg: f64,
    },
    Macro {
        primitives: Vec<MacroPrimitive>,
    },
}

impl Aperture {
    /// 線幅として使用する場合の代表太さ（mm換算済み）
    fn stroke_width(&self) -> f64 {
        match self {
            Aperture::Circle { diameter } => *diameter,
            Aperture::Rectangle { width, height } => width.max(*height),
            Aperture::Obround { width, height } => width.min(*height),
            Aperture::Polygon { outer_diameter, .. } => *outer_diameter,
            Aperture::Macro { .. } => 0.1,
        }
    }
}

#[derive(Debug, Clone)]
enum MacroPrimitive {
    Circle {
        exposure: bool,
        diameter: f64,
        center: Point2,
        rotation_deg: f64,
    },
    Outline {
        exposure: bool,
        points: Vec<Point2>,
        rotation_deg: f64,
    },
    Polygon {
        exposure: bool,
        vertices: usize,
        center: Point2,
        diameter: f64,
        rotation_deg: f64,
    },
    VectorLine {
        exposure: bool,
        width: f64,
        start: Point2,
        end: Point2,
        rotation_deg: f64,
    },
    CenterLine {
        exposure: bool,
        width: f64,
        height: f64,
        center: Point2,
        rotation_deg: f64,
    },
}

pub struct GerberParser {
    geometry: Geometry,
    fmt_int: usize,
    fmt_dec: usize,
    zero_suppression: ZeroSuppression,
    notation: Notation,
    unit: Unit,
    polarity: Polarity,

    apertures: HashMap<i32, Aperture>,
    macros: HashMap<String, Vec<String>>,
    current_aperture: i32,

    x: f64,
    y: f64,
    op_mode: i32, // 1=D01, 2=D02, 3=D03
    interp_mode: i32, // 1=G01, 2=G02, 3=G03
    multi_quadrant: bool,

    region_mode: bool,
    region_points: Vec<Point2>,
}

impl GerberParser {
    pub fn new() -> Self {
        Self {
            geometry: Geometry::new(),
            fmt_int: 4,
            fmt_dec: 6,
            zero_suppression: ZeroSuppression::Leading,
            notation: Notation::Absolute,
            unit: Unit::Mm,
            polarity: Polarity::Dark,
            apertures: HashMap::new(),
            macros: HashMap::new(),
            current_aperture: 0,
            x: 0.0,
            y: 0.0,
            op_mode: 2,
            interp_mode: 1,
            multi_quadrant: true,
            region_mode: false,
            region_points: Vec::new(),
        }
    }

    pub fn parse_file<P: AsRef<Path>>(path: P) -> Result<Geometry, String> {
        let bytes = fs::read(path.as_ref())
            .map_err(|e| format!("ファイルの読み込みに失敗しました: {}", e))?;
        let content = String::from_utf8_lossy(&bytes);
        let mut parser = Self::new();
        Ok(parser.parse(&content))
    }

    pub fn parse(&mut self, content: &str) -> Geometry {
        let commands = Self::split_commands(content);

        for cmd in commands {
            let trimmed = cmd.trim();
            if trimmed.is_empty() {
                continue;
            }
            if trimmed.starts_with('%') && trimmed.ends_with('%') && trimmed.len() >= 2 {
                self.parse_extended_block(&trimmed[1..trimmed.len() - 1]);
            } else {
                self.parse_command(trimmed);
            }
        }

        if self.region_mode && self.region_points.len() >= 3 {
            self.geometry.polygons.push(PolygonShape {
                points: std::mem::take(&mut self.region_points),
                polarity: self.polarity,
            });
            self.region_mode = false;
        }

        std::mem::take(&mut self.geometry)
    }

    fn split_commands(content: &str) -> Vec<String> {
        let mut commands = Vec::new();
        let mut buffer = String::new();
        let mut in_extended = false;

        for ch in content.chars() {
            if ch == '%' {
                if in_extended {
                    buffer.push(ch);
                    commands.push(std::mem::take(&mut buffer));
                    in_extended = false;
                } else {
                    if !buffer.trim().is_empty() {
                        commands.push(std::mem::take(&mut buffer));
                    } else {
                        buffer.clear();
                    }
                    buffer.push(ch);
                    in_extended = true;
                }
            } else if ch == '*' && !in_extended {
                buffer.push(ch);
                commands.push(std::mem::take(&mut buffer));
            } else if ch != '\r' && ch != '\n' || in_extended {
                buffer.push(ch);
            }
        }

        if !buffer.trim().is_empty() {
            commands.push(buffer);
        }
        commands
    }

    fn unit_scale(&self) -> f64 {
        match self.unit {
            Unit::Mm => 1.0,
            Unit::Inch => 25.4,
        }
    }

    fn parse_extended_block(&mut self, block: &str) {
        // アパーチャマクロ (%AMname*...%) はブロック全体で1つの定義になる
        let trimmed = block.trim();
        if let Some(rest) = trimmed.strip_prefix("AM") {
            let parts: Vec<String> = rest
                .split('*')
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty())
                .collect();
            if let Some((name, body)) = parts.split_first() {
                self.macros.insert(name.clone(), body.to_vec());
            }
            return;
        }

        for stmt in block.split('*') {
            let s = stmt.trim();
            if s.is_empty() {
                continue;
            }
            if s.starts_with("FS") {
                self.parse_fs(s);
            } else if s.starts_with("MO") {
                self.parse_mo(s);
            } else if s.starts_with("LP") {
                self.parse_lp(s);
            } else if s.starts_with("ADD") {
                self.parse_add(s);
            }
            // TF, TA, TD, AS などは無視して問題ない
        }
    }

    fn parse_fs(&mut self, stmt: &str) {
        // 例: FSLAX46Y46 または FSTIX24Y24
        if stmt.contains("LA") || stmt.contains("LI") {
            self.zero_suppression = ZeroSuppression::Leading;
        } else if stmt.contains("TA") || stmt.contains("TI") {
            self.zero_suppression = ZeroSuppression::Trailing;
        }
        if stmt.contains('I') {
            self.notation = Notation::Incremental;
        } else if stmt.contains('A') {
            self.notation = Notation::Absolute;
        }

        if let Some(xpos) = stmt.find('X') {
            let digits: String = stmt[xpos + 1..]
                .chars()
                .take_while(|c| c.is_ascii_digit())
                .collect();
            if digits.len() >= 2 {
                if let (Ok(i), Ok(d)) = (digits[0..1].parse::<usize>(), digits[1..2].parse::<usize>())
                {
                    self.fmt_int = i;
                    self.fmt_dec = d;
                }
            }
        }
    }

    fn parse_mo(&mut self, stmt: &str) {
        if stmt.contains("IN") {
            self.unit = Unit::Inch;
        } else if stmt.contains("MM") {
            self.unit = Unit::Mm;
        }
    }

    fn parse_lp(&mut self, stmt: &str) {
        if stmt.ends_with('C') {
            self.polarity = Polarity::Clear;
        } else {
            self.polarity = Polarity::Dark;
        }
    }

    fn parse_add(&mut self, stmt: &str) {
        // ADD<D-code><Type>,<params...>
        let rest = match stmt.strip_prefix("ADD") {
            Some(r) => r,
            None => return,
        };
        let num_len = rest.chars().take_while(|c| c.is_ascii_digit()).count();
        if num_len == 0 {
            return;
        }
        let code: i32 = match rest[..num_len].parse() {
            Ok(c) => c,
            Err(_) => return,
        };
        let tail = &rest[num_len..];
        let (name, params_str) = match tail.find(',') {
            Some(idx) => (&tail[..idx], &tail[idx + 1..]),
            None => (tail, ""),
        };

        let scale = self.unit_scale();
        let params: Vec<f64> = params_str
            .split('X')
            .filter(|s| !s.is_empty())
            .filter_map(|s| s.parse::<f64>().ok())
            .collect();

        let aperture = match name {
            "C" => {
                let d = params.first().copied().unwrap_or(0.0) * scale;
                Aperture::Circle { diameter: d }
            }
            "R" => {
                let w = params.first().copied().unwrap_or(0.0) * scale;
                let h = params.get(1).copied().unwrap_or(w / scale) * scale;
                Aperture::Rectangle {
                    width: w,
                    height: h,
                }
            }
            "O" => {
                let w = params.first().copied().unwrap_or(0.0) * scale;
                let h = params.get(1).copied().unwrap_or(w / scale) * scale;
                Aperture::Obround {
                    width: w,
                    height: h,
                }
            }
            "P" => {
                let od = params.first().copied().unwrap_or(0.0) * scale;
                let v = params.get(1).copied().unwrap_or(4.0).round().max(3.0) as usize;
                let rot = params.get(2).copied().unwrap_or(0.0);
                Aperture::Polygon {
                    outer_diameter: od,
                    vertices: v,
                    rotation_deg: rot,
                }
            }
            macro_name => {
                if let Some(lines) = self.macros.get(macro_name).cloned() {
                    let primitives = Self::instantiate_macro(&lines, &params, scale);
                    Aperture::Macro { primitives }
                } else {
                    let d = params.first().copied().unwrap_or(0.1) * scale;
                    Aperture::Circle { diameter: d }
                }
            }
        };

        self.apertures.insert(code, aperture);
    }

    fn instantiate_macro(lines: &[String], params: &[f64], scale: f64) -> Vec<MacroPrimitive> {
        let mut primitives = Vec::new();
        let mut vars: HashMap<usize, f64> = HashMap::new();
        for (i, &val) in params.iter().enumerate() {
            vars.insert(i + 1, val);
        }

        for line in lines {
            let s = line.trim();
            if s.is_empty() || s.starts_with("0 ") || s == "0" {
                continue;
            }
            // ローカル変数定義 $n=...
            if s.starts_with('$') {
                if let Some(eq) = s.find('=') {
                    if let Ok(idx) = s[1..eq].trim().parse::<usize>() {
                        let val = Self::eval_macro_expr(&s[eq + 1..], &vars);
                        vars.insert(idx, val);
                    }
                    continue;
                }
            }

            let fields: Vec<f64> = s
                .split(',')
                .map(|f| Self::eval_macro_expr(f.trim(), &vars))
                .collect();

            if fields.is_empty() {
                continue;
            }
            let code = fields[0].round() as i32;
            match code {
                1 => {
                    // Circle: 1, exposure, diameter, cx, cy [, rot]
                    if fields.len() >= 5 {
                        primitives.push(MacroPrimitive::Circle {
                            exposure: fields[1] > 0.5,
                            diameter: fields[2] * scale,
                            center: Point2::new(fields[3] * scale, fields[4] * scale),
                            rotation_deg: fields.get(5).copied().unwrap_or(0.0),
                        });
                    }
                }
                4 => {
                    // Outline: 4, exposure, num_vertices, x0, y0, ..., xn, yn, rot
                    if fields.len() >= 6 {
                        let exposure = fields[1] > 0.5;
                        let n_verts = fields[2].round().max(0.0) as usize;
                        // 頂点数は始点+n_verts点の場合と、閉じたn_verts+1点の場合がある
                        let coord_start = 3;
                        let mut pts = Vec::new();
                        let max_pairs = (fields.len().saturating_sub(coord_start + 1)) / 2;
                        let count = n_verts.min(max_pairs);
                        for i in 0..count {
                            let x = fields[coord_start + i * 2] * scale;
                            let y = fields[coord_start + i * 2 + 1] * scale;
                            pts.push(Point2::new(x, y));
                        }
                        let rot_idx = coord_start + (n_verts + 1) * 2;
                        let rotation_deg = if rot_idx < fields.len() {
                            fields[rot_idx]
                        } else {
                            *fields.last().unwrap_or(&0.0)
                        };
                        if pts.len() >= 3 {
                            primitives.push(MacroPrimitive::Outline {
                                exposure,
                                points: pts,
                                rotation_deg,
                            });
                        }
                    }
                }
                5 => {
                    // Polygon: 5, exposure, vertices, cx, cy, diameter, rot
                    if fields.len() >= 7 {
                        primitives.push(MacroPrimitive::Polygon {
                            exposure: fields[1] > 0.5,
                            vertices: fields[2].round().max(3.0) as usize,
                            center: Point2::new(fields[3] * scale, fields[4] * scale),
                            diameter: fields[5] * scale,
                            rotation_deg: fields[6],
                        });
                    }
                }
                20 => {
                    // Vector Line: 20, exposure, width, sx, sy, ex, ey, rot
                    if fields.len() >= 8 {
                        primitives.push(MacroPrimitive::VectorLine {
                            exposure: fields[1] > 0.5,
                            width: fields[2] * scale,
                            start: Point2::new(fields[3] * scale, fields[4] * scale),
                            end: Point2::new(fields[5] * scale, fields[6] * scale),
                            rotation_deg: fields[7],
                        });
                    }
                }
                21 => {
                    // Center Line: 21, exposure, width, height, cx, cy, rot
                    if fields.len() >= 7 {
                        primitives.push(MacroPrimitive::CenterLine {
                            exposure: fields[1] > 0.5,
                            width: fields[2] * scale,
                            height: fields[3] * scale,
                            center: Point2::new(fields[4] * scale, fields[5] * scale),
                            rotation_deg: fields[6],
                        });
                    }
                }
                _ => {}
            }
        }
        primitives
    }

    fn eval_macro_expr(expr: &str, vars: &HashMap<usize, f64>) -> f64 {
        // $n を数値置換してから簡易四則演算を評価
        let mut resolved = String::new();
        let chars: Vec<char> = expr.chars().collect();
        let mut i = 0;
        while i < chars.len() {
            if chars[i] == '$' {
                i += 1;
                let mut num_str = String::new();
                while i < chars.len() && chars[i].is_ascii_digit() {
                    num_str.push(chars[i]);
                    i += 1;
                }
                let idx = num_str.parse::<usize>().unwrap_or(0);
                let val = vars.get(&idx).copied().unwrap_or(0.0);
                resolved.push_str(&format!("{:.8}", val));
            } else if chars[i] == 'x' || chars[i] == 'X' {
                resolved.push('*');
                i += 1;
            } else {
                resolved.push(chars[i]);
                i += 1;
            }
        }
        Self::eval_arithmetic(&resolved)
    }

    fn eval_arithmetic(s: &str) -> f64 {
        let s = s.trim();
        if s.is_empty() {
            return 0.0;
        }
        if let Ok(v) = s.parse::<f64>() {
            return v;
        }
        // 加減算（右から探して単項符号を避ける）
        let bytes = s.as_bytes();
        let mut depth = 0i32;
        for idx in (1..bytes.len()).rev() {
            match bytes[idx] {
                b')' => depth += 1,
                b'(' => depth -= 1,
                b'+' | b'-' if depth == 0 => {
                    let prev = bytes[idx - 1];
                    if prev != b'+' && prev != b'-' && prev != b'*' && prev != b'/' && prev != b'e' && prev != b'E' {
                        let lhs = Self::eval_arithmetic(&s[..idx]);
                        let rhs = Self::eval_arithmetic(&s[idx + 1..]);
                        return if bytes[idx] == b'+' { lhs + rhs } else { lhs - rhs };
                    }
                }
                _ => {}
            }
        }
        depth = 0;
        for idx in (1..bytes.len()).rev() {
            match bytes[idx] {
                b')' => depth += 1,
                b'(' => depth -= 1,
                b'*' | b'/' if depth == 0 => {
                    let lhs = Self::eval_arithmetic(&s[..idx]);
                    let rhs = Self::eval_arithmetic(&s[idx + 1..]);
                    return if bytes[idx] == b'*' {
                        lhs * rhs
                    } else if rhs.abs() > 1e-12 {
                        lhs / rhs
                    } else {
                        0.0
                    };
                }
                _ => {}
            }
        }
        if s.starts_with('(') && s.ends_with(')') {
            return Self::eval_arithmetic(&s[1..s.len() - 1]);
        }
        0.0
    }

    fn parse_command(&mut self, cmd: &str) {
        let cmd = cmd.trim_end_matches('*').trim();
        if cmd.is_empty() || cmd.starts_with("G04") || cmd.starts_with("G4") {
            return;
        }

        let mut rest = cmd;

        // Gコードの抽出（1行に G01X... のように連結されることがある）
        if rest.starts_with('G') {
            let digits: String = rest[1..].chars().take_while(|c| c.is_ascii_digit()).collect();
            if let Ok(g) = digits.parse::<i32>() {
                match g {
                    1 => self.interp_mode = 1,
                    2 => self.interp_mode = 2,
                    3 => self.interp_mode = 3,
                    36 => {
                        self.region_mode = true;
                        self.region_points.clear();
                    }
                    37 => {
                        if self.region_mode && self.region_points.len() >= 3 {
                            self.geometry.polygons.push(PolygonShape {
                                points: std::mem::take(&mut self.region_points),
                                polarity: self.polarity,
                            });
                        }
                        self.region_mode = false;
                        self.region_points.clear();
                    }
                    70 => self.unit = Unit::Inch,
                    71 => self.unit = Unit::Mm,
                    74 => self.multi_quadrant = false,
                    75 => self.multi_quadrant = true,
                    90 => self.notation = Notation::Absolute,
                    91 => self.notation = Notation::Incremental,
                    _ => {}
                }
                rest = &rest[1 + digits.len()..];
            }
        }

        // アパーチャ選択のみのコマンド (D10*, G54D10*)
        if rest.is_empty() {
            return;
        }

        // 座標・I/J・Dコードの抽出
        let x_val = Self::extract_coord(rest, 'X').map(|s| self.convert_coord(&s));
        let y_val = Self::extract_coord(rest, 'Y').map(|s| self.convert_coord(&s));
        let i_val = Self::extract_coord(rest, 'I')
            .map(|s| self.convert_coord(&s))
            .unwrap_or(0.0);
        let j_val = Self::extract_coord(rest, 'J')
            .map(|s| self.convert_coord(&s))
            .unwrap_or(0.0);

        let d_code = Self::extract_d_code(rest);

        // D10以上はアパーチャ選択
        if let Some(d) = d_code {
            if d >= 10 {
                self.current_aperture = d;
                // 座標が同時に含まれない場合は終了
                if x_val.is_none() && y_val.is_none() {
                    return;
                }
            } else {
                self.op_mode = d;
            }
        }

        // M02 (終了) など座標がない場合
        if x_val.is_none() && y_val.is_none() && d_code.is_none() {
            return;
        }

        let (new_x, new_y) = match self.notation {
            Notation::Absolute => (x_val.unwrap_or(self.x), y_val.unwrap_or(self.y)),
            Notation::Incremental => (
                self.x + x_val.unwrap_or(0.0),
                self.y + y_val.unwrap_or(0.0),
            ),
        };

        let op = match d_code {
            Some(d) if d <= 3 => d,
            _ => self.op_mode,
        };

        if self.region_mode {
            match op {
                2 => {
                    // 既存のリージョンが閉じていれば確定して新しいサブリージョン開始
                    if self.region_points.len() >= 3 {
                        self.geometry.polygons.push(PolygonShape {
                            points: std::mem::take(&mut self.region_points),
                            polarity: self.polarity,
                        });
                    } else {
                        self.region_points.clear();
                    }
                    self.region_points.push(Point2::new(new_x, new_y));
                }
                1 => {
                    if self.region_points.is_empty() {
                        self.region_points.push(Point2::new(self.x, self.y));
                    }
                    if self.interp_mode == 1 {
                        self.region_points.push(Point2::new(new_x, new_y));
                    } else {
                        // リージョン内の円弧を線形補間して追加
                        let center = self.compute_arc_center(self.x, self.y, new_x, new_y, i_val, j_val);
                        let cw = self.interp_mode == 2;
                        let pts = Self::sample_arc_points(
                            Point2::new(self.x, self.y),
                            Point2::new(new_x, new_y),
                            center,
                            cw,
                            0.05,
                        );
                        self.region_points.extend(pts.into_iter().skip(1));
                    }
                }
                _ => {}
            }
        } else {
            match op {
                1 => {
                    let width = self
                        .apertures
                        .get(&self.current_aperture)
                        .map(|a| a.stroke_width())
                        .unwrap_or(0.1);
                    if self.interp_mode == 1 {
                        self.geometry.segments.push(Segment {
                            start: Point2::new(self.x, self.y),
                            end: Point2::new(new_x, new_y),
                            width,
                            polarity: self.polarity,
                        });
                    } else {
                        let center = self.compute_arc_center(self.x, self.y, new_x, new_y, i_val, j_val);
                        self.geometry.arcs.push(ArcSegment {
                            start: Point2::new(self.x, self.y),
                            end: Point2::new(new_x, new_y),
                            center,
                            clockwise: self.interp_mode == 2,
                            width,
                            polarity: self.polarity,
                        });
                    }
                }
                3 => {
                    self.flash_aperture(Point2::new(new_x, new_y));
                }
                _ => {}
            }
        }

        self.x = new_x;
        self.y = new_y;
    }

    fn compute_arc_center(
        &self,
        sx: f64,
        sy: f64,
        ex: f64,
        ey: f64,
        i_val: f64,
        j_val: f64,
    ) -> Point2 {
        if self.multi_quadrant {
            Point2::new(sx + i_val, sy + j_val)
        } else {
            // シングルクアドラント: I, J の符号候補4つから始点・終点の半径一致が最も良いものを選択
            let candidates = [
                (sx + i_val.abs(), sy + j_val.abs()),
                (sx - i_val.abs(), sy + j_val.abs()),
                (sx + i_val.abs(), sy - j_val.abs()),
                (sx - i_val.abs(), sy - j_val.abs()),
            ];
            let mut best = candidates[0];
            let mut best_err = f64::INFINITY;
            for (cx, cy) in candidates {
                let r1 = ((sx - cx).powi(2) + (sy - cy).powi(2)).sqrt();
                let r2 = ((ex - cx).powi(2) + (ey - cy).powi(2)).sqrt();
                let err = (r1 - r2).abs();
                if err < best_err {
                    best_err = err;
                    best = (cx, cy);
                }
            }
            Point2::new(best.0, best.1)
        }
    }

    pub fn sample_arc_points(
        start: Point2,
        end: Point2,
        center: Point2,
        clockwise: bool,
        resolution: f64,
    ) -> Vec<Point2> {
        let radius = center.distance_to(&start);
        if radius < 1e-6 {
            return vec![start, end];
        }
        let start_angle = (start.y - center.y).atan2(start.x - center.x);
        let end_angle = (end.y - center.y).atan2(end.x - center.x);

        let mut sweep = if clockwise {
            let mut s = start_angle - end_angle;
            if s <= 1e-9 {
                s += 2.0 * PI;
            }
            -s
        } else {
            let mut s = end_angle - start_angle;
            if s <= 1e-9 {
                s += 2.0 * PI;
            }
            s
        };

        // 始点と終点がほぼ同じなら全円
        if start.distance_to(&end) < 1e-5 {
            sweep = if clockwise { -2.0 * PI } else { 2.0 * PI };
        }

        let arc_len = radius * sweep.abs();
        let steps = ((arc_len / resolution.max(0.01)).ceil() as usize).clamp(8, 256);

        let mut pts = Vec::with_capacity(steps + 1);
        for i in 0..=steps {
            let t = i as f64 / steps as f64;
            let ang = start_angle + sweep * t;
            pts.push(Point2::new(
                center.x + radius * ang.cos(),
                center.y + radius * ang.sin(),
            ));
        }
        pts
    }

    fn flash_aperture(&mut self, pos: Point2) {
        let Some(ap) = self.apertures.get(&self.current_aperture).cloned() else {
            return;
        };
        let pol = self.polarity;

        match ap {
            Aperture::Circle { diameter } => {
                if diameter > 0.0 {
                    self.geometry.circles.push(CircleShape {
                        center: pos,
                        radius: diameter * 0.5,
                        polarity: pol,
                    });
                }
            }
            Aperture::Rectangle { width, height } => {
                let hw = width * 0.5;
                let hh = height * 0.5;
                self.geometry.polygons.push(PolygonShape {
                    points: vec![
                        Point2::new(pos.x - hw, pos.y - hh),
                        Point2::new(pos.x + hw, pos.y - hh),
                        Point2::new(pos.x + hw, pos.y + hh),
                        Point2::new(pos.x - hw, pos.y + hh),
                    ],
                    polarity: pol,
                });
            }
            Aperture::Obround { width, height } => {
                let pts = Self::make_obround_points(pos, width, height);
                self.geometry.polygons.push(PolygonShape {
                    points: pts,
                    polarity: pol,
                });
            }
            Aperture::Polygon {
                outer_diameter,
                vertices,
                rotation_deg,
            } => {
                let pts = Self::make_regular_polygon(pos, outer_diameter * 0.5, vertices, rotation_deg);
                self.geometry.polygons.push(PolygonShape {
                    points: pts,
                    polarity: pol,
                });
            }
            Aperture::Macro { primitives } => {
                for prim in primitives {
                    let effective_pol = match (pol, prim.exposure()) {
                        (Polarity::Dark, true) => Polarity::Dark,
                        (Polarity::Dark, false) => Polarity::Clear,
                        (Polarity::Clear, true) => Polarity::Clear,
                        (Polarity::Clear, false) => Polarity::Dark,
                    };
                    match prim {
                        MacroPrimitive::Circle {
                            diameter,
                            center,
                            rotation_deg,
                            ..
                        } => {
                            let c = Self::rotate_point(center, rotation_deg);
                            self.geometry.circles.push(CircleShape {
                                center: Point2::new(pos.x + c.x, pos.y + c.y),
                                radius: diameter * 0.5,
                                polarity: effective_pol,
                            });
                        }
                        MacroPrimitive::Outline {
                            points,
                            rotation_deg,
                            ..
                        } => {
                            let pts: Vec<Point2> = points
                                .into_iter()
                                .map(|p| {
                                    let rp = Self::rotate_point(p, rotation_deg);
                                    Point2::new(pos.x + rp.x, pos.y + rp.y)
                                })
                                .collect();
                            self.geometry.polygons.push(PolygonShape {
                                points: pts,
                                polarity: effective_pol,
                            });
                        }
                        MacroPrimitive::Polygon {
                            vertices,
                            center,
                            diameter,
                            rotation_deg,
                            ..
                        } => {
                            let c = Self::rotate_point(center, rotation_deg);
                            let pts = Self::make_regular_polygon(
                                Point2::new(pos.x + c.x, pos.y + c.y),
                                diameter * 0.5,
                                vertices,
                                rotation_deg,
                            );
                            self.geometry.polygons.push(PolygonShape {
                                points: pts,
                                polarity: effective_pol,
                            });
                        }
                        MacroPrimitive::VectorLine {
                            width,
                            start,
                            end,
                            rotation_deg,
                            ..
                        } => {
                            let s = Self::rotate_point(start, rotation_deg);
                            let e = Self::rotate_point(end, rotation_deg);
                            self.geometry.segments.push(Segment {
                                start: Point2::new(pos.x + s.x, pos.y + s.y),
                                end: Point2::new(pos.x + e.x, pos.y + e.y),
                                width,
                                polarity: effective_pol,
                            });
                        }
                        MacroPrimitive::CenterLine {
                            width,
                            height,
                            center,
                            rotation_deg,
                            ..
                        } => {
                            let hw = width * 0.5;
                            let hh = height * 0.5;
                            let local = [
                                Point2::new(center.x - hw, center.y - hh),
                                Point2::new(center.x + hw, center.y - hh),
                                Point2::new(center.x + hw, center.y + hh),
                                Point2::new(center.x - hw, center.y + hh),
                            ];
                            let pts = local
                                .into_iter()
                                .map(|p| {
                                    let rp = Self::rotate_point(p, rotation_deg);
                                    Point2::new(pos.x + rp.x, pos.y + rp.y)
                                })
                                .collect();
                            self.geometry.polygons.push(PolygonShape {
                                points: pts,
                                polarity: effective_pol,
                            });
                        }
                    }
                }
            }
        }
    }

    fn rotate_point(p: Point2, deg: f64) -> Point2 {
        if deg.abs() < 1e-9 {
            return p;
        }
        let rad = deg.to_radians();
        let (s, c) = rad.sin_cos();
        Point2::new(p.x * c - p.y * s, p.x * s + p.y * c)
    }

    fn make_regular_polygon(center: Point2, radius: f64, vertices: usize, rot_deg: f64) -> Vec<Point2> {
        let mut pts = Vec::with_capacity(vertices);
        let rot_rad = rot_deg.to_radians();
        for i in 0..vertices {
            let ang = rot_rad + 2.0 * PI * (i as f64) / (vertices as f64);
            pts.push(Point2::new(
                center.x + radius * ang.cos(),
                center.y + radius * ang.sin(),
            ));
        }
        pts
    }

    fn make_obround_points(center: Point2, width: f64, height: f64) -> Vec<Point2> {
        let mut pts = Vec::new();
        let steps = 12;
        if width >= height {
            let r = height * 0.5;
            let dx = (width - height) * 0.5;
            for i in 0..=steps {
                let ang = -PI * 0.5 + PI * (i as f64) / (steps as f64);
                pts.push(Point2::new(center.x + dx + r * ang.cos(), center.y + r * ang.sin()));
            }
            for i in 0..=steps {
                let ang = PI * 0.5 + PI * (i as f64) / (steps as f64);
                pts.push(Point2::new(center.x - dx + r * ang.cos(), center.y + r * ang.sin()));
            }
        } else {
            let r = width * 0.5;
            let dy = (height - width) * 0.5;
            for i in 0..=steps {
                let ang = 0.0 + PI * (i as f64) / (steps as f64);
                pts.push(Point2::new(center.x + r * ang.cos(), center.y + dy + r * ang.sin()));
            }
            for i in 0..=steps {
                let ang = PI + PI * (i as f64) / (steps as f64);
                pts.push(Point2::new(center.x + r * ang.cos(), center.y - dy + r * ang.sin()));
            }
        }
        pts
    }

    fn extract_coord(s: &str, axis: char) -> Option<String> {
        let bytes = s.as_bytes();
        let target = axis as u8;
        for i in 0..bytes.len() {
            if bytes[i] == target {
                let mut end = i + 1;
                if end < bytes.len() && (bytes[end] == b'+' || bytes[end] == b'-') {
                    end += 1;
                }
                let start_digits = end;
                while end < bytes.len() && bytes[end].is_ascii_digit() {
                    end += 1;
                }
                if end > start_digits {
                    return Some(s[i + 1..end].to_string());
                }
            }
        }
        None
    }

    fn extract_d_code(s: &str) -> Option<i32> {
        let pos = s.rfind('D')?;
        let digits: String = s[pos + 1..]
            .chars()
            .take_while(|c| c.is_ascii_digit())
            .collect();
        if digits.is_empty() {
            None
        } else {
            digits.parse::<i32>().ok()
        }
    }

    fn convert_coord(&self, raw: &str) -> f64 {
        let (sign, digits) = if let Some(rest) = raw.strip_prefix('-') {
            (-1.0, rest)
        } else if let Some(rest) = raw.strip_prefix('+') {
            (1.0, rest)
        } else {
            (1.0, raw)
        };

        let total = self.fmt_int + self.fmt_dec;
        let padded = match self.zero_suppression {
            ZeroSuppression::Leading => {
                if digits.len() < total {
                    format!("{:0>width$}", digits, width = total)
                } else {
                    digits.to_string()
                }
            }
            ZeroSuppression::Trailing => {
                if digits.len() < total {
                    format!("{:0<width$}", digits, width = total)
                } else {
                    digits.to_string()
                }
            }
        };

        let split_pos = padded.len().saturating_sub(self.fmt_dec);
        let int_part = &padded[..split_pos];
        let dec_part = &padded[split_pos..];
        let num_str = format!("{}.{}", if int_part.is_empty() { "0" } else { int_part }, dec_part);
        let val = num_str.parse::<f64>().unwrap_or(0.0) * sign;
        val * self.unit_scale()
    }
}

impl MacroPrimitive {
    fn exposure(&self) -> bool {
        match self {
            MacroPrimitive::Circle { exposure, .. }
            | MacroPrimitive::Outline { exposure, .. }
            | MacroPrimitive::Polygon { exposure, .. }
            | MacroPrimitive::VectorLine { exposure, .. }
            | MacroPrimitive::CenterLine { exposure, .. } => *exposure,
        }
    }
}
