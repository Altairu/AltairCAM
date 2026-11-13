//! Excellon ドリルファイルパーサー

use std::collections::HashMap;
use std::fs;
use std::path::Path;

use crate::core::geometry::{DrillData, DrillHole, DrillSlot, Point2};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Unit {
    Mm,
    Inch,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ZeroMode {
    Leading,  // LZ: 先頭ゼロを残す（末尾ゼロ省略がTZ、Leading残し=TZの場合と逆の用語混乱に注意）
    Trailing, // TZ: 末尾ゼロ省略
}

pub struct ExcellonParser {
    drill_data: DrillData,
    tools: HashMap<i32, f64>,
    current_tool: Option<i32>,
    current_x: f64,
    current_y: f64,
    unit: Unit,
    zero_mode: ZeroMode,
    fmt_int: usize,
    fmt_dec: usize,
    in_header: bool,
}

impl ExcellonParser {
    pub fn new() -> Self {
        Self {
            drill_data: DrillData::new(),
            tools: HashMap::new(),
            current_tool: None,
            current_x: 0.0,
            current_y: 0.0,
            unit: Unit::Mm,
            zero_mode: ZeroMode::Leading,
            fmt_int: 3,
            fmt_dec: 3,
            in_header: false,
        }
    }

    pub fn parse_file<P: AsRef<Path>>(path: P) -> Result<DrillData, String> {
        let bytes = fs::read(path.as_ref())
            .map_err(|e| format!("ドリルファイルの読み込みに失敗しました: {}", e))?;
        let content = String::from_utf8_lossy(&bytes);
        let mut parser = Self::new();
        Ok(parser.parse(&content))
    }

    pub fn parse(&mut self, content: &str) -> DrillData {
        for raw_line in content.lines() {
            let line = raw_line.trim();
            if line.is_empty() {
                continue;
            }

            // コメント内のフォーマットヒントをチェック (; FILE_FORMAT=3:3 など)
            if line.starts_with(';') {
                self.check_comment_hints(line);
                continue;
            }

            if line == "M48" {
                self.in_header = true;
                continue;
            }
            if line == "%" || line == "M95" {
                self.in_header = false;
                continue;
            }
            if line == "M30" {
                break;
            }

            if line.starts_with("METRIC") {
                self.unit = Unit::Mm;
                if line.contains("TZ") {
                    self.zero_mode = ZeroMode::Trailing;
                } else if line.contains("LZ") {
                    self.zero_mode = ZeroMode::Leading;
                }
                continue;
            }
            if line.starts_with("INCH") {
                self.unit = Unit::Inch;
                if line.contains("TZ") {
                    self.zero_mode = ZeroMode::Trailing;
                } else if line.contains("LZ") {
                    self.zero_mode = ZeroMode::Leading;
                }
                continue;
            }

            if line.starts_with("FMAT") || line == "G90" || line == "G05" || line == "ICI,OFF" {
                continue;
            }

            // ツール定義 (例: T01C0.800, T1F...S...C0.8)
            if line.starts_with('T') {
                if line.contains('C') {
                    self.parse_tool_definition(line);
                } else {
                    self.parse_tool_selection(line);
                }
                continue;
            }

            // 長穴 (G85)
            if let Some(g85_pos) = line.find("G85") {
                self.parse_slot(&line[..g85_pos], &line[g85_pos + 3..]);
                continue;
            }

            // 通常座標
            if line.starts_with('X') || line.starts_with('Y') {
                self.parse_coordinate(line);
            }
        }

        std::mem::take(&mut self.drill_data)
    }

    fn check_comment_hints(&mut self, line: &str) {
        if let Some(pos) = line.find("FILE_FORMAT=") {
            let rest = &line[pos + "FILE_FORMAT=".len()..];
            let parts: Vec<&str> = rest.split(':').collect();
            if parts.len() == 2 {
                if let (Ok(i), Ok(d)) = (
                    parts[0].trim().parse::<usize>(),
                    parts[1].trim().parse::<usize>(),
                ) {
                    self.fmt_int = i;
                    self.fmt_dec = d;
                }
            }
        }
    }

    fn parse_tool_definition(&mut self, line: &str) {
        let digits: String = line[1..].chars().take_while(|c| c.is_ascii_digit()).collect();
        let Ok(tool_num) = digits.parse::<i32>() else {
            return;
        };
        if let Some(c_pos) = line.find('C') {
            let dia_str: String = line[c_pos + 1..]
                .chars()
                .take_while(|c| c.is_ascii_digit() || *c == '.')
                .collect();
            if let Ok(mut dia) = dia_str.parse::<f64>() {
                if self.unit == Unit::Inch {
                    dia *= 25.4;
                }
                self.tools.insert(tool_num, dia);
            }
        }
    }

    fn parse_tool_selection(&mut self, line: &str) {
        let digits: String = line[1..].chars().take_while(|c| c.is_ascii_digit()).collect();
        if let Ok(tool_num) = digits.parse::<i32>() {
            if tool_num == 0 {
                self.current_tool = None;
            } else {
                self.current_tool = Some(tool_num);
            }
        }
    }

    fn extract_axis(s: &str, axis: char) -> Option<String> {
        let idx = s.find(axis)?;
        let val: String = s[idx + 1..]
            .chars()
            .take_while(|c| c.is_ascii_digit() || *c == '.' || *c == '+' || *c == '-')
            .collect();
        if val.is_empty() {
            None
        } else {
            Some(val)
        }
    }

    fn parse_coordinate(&mut self, line: &str) {
        if let Some(xs) = Self::extract_axis(line, 'X') {
            self.current_x = self.convert_coord(&xs);
        }
        if let Some(ys) = Self::extract_axis(line, 'Y') {
            self.current_y = self.convert_coord(&ys);
        }

        if let Some(t) = self.current_tool {
            if let Some(&dia) = self.tools.get(&t) {
                self.drill_data.holes.push(DrillHole {
                    position: Point2::new(self.current_x, self.current_y),
                    diameter: dia,
                });
            }
        }
    }

    fn parse_slot(&mut self, start_part: &str, end_part: &str) {
        let sx = Self::extract_axis(start_part, 'X')
            .map(|s| self.convert_coord(&s))
            .unwrap_or(self.current_x);
        let sy = Self::extract_axis(start_part, 'Y')
            .map(|s| self.convert_coord(&s))
            .unwrap_or(self.current_y);

        let ex = Self::extract_axis(end_part, 'X')
            .map(|s| self.convert_coord(&s))
            .unwrap_or(sx);
        let ey = Self::extract_axis(end_part, 'Y')
            .map(|s| self.convert_coord(&s))
            .unwrap_or(sy);

        self.current_x = ex;
        self.current_y = ey;

        if let Some(t) = self.current_tool {
            if let Some(&dia) = self.tools.get(&t) {
                self.drill_data.slots.push(DrillSlot {
                    start: Point2::new(sx, sy),
                    end: Point2::new(ex, ey),
                    diameter: dia,
                });
            }
        }
    }

    fn convert_coord(&self, raw: &str) -> f64 {
        let mut val = if raw.contains('.') {
            raw.parse::<f64>().unwrap_or(0.0)
        } else {
            let (sign, digits) = if let Some(r) = raw.strip_prefix('-') {
                (-1.0, r)
            } else if let Some(r) = raw.strip_prefix('+') {
                (1.0, r)
            } else {
                (1.0, raw)
            };
            let total = self.fmt_int + self.fmt_dec;
            let padded = match self.zero_mode {
                ZeroMode::Leading => format!("{:0>width$}", digits, width = total),
                ZeroMode::Trailing => format!("{:0<width$}", digits, width = total),
            };
            let split_pos = padded.len().saturating_sub(self.fmt_dec);
            let int_part = &padded[..split_pos];
            let dec_part = &padded[split_pos..];
            let num = format!("{}.{}", if int_part.is_empty() { "0" } else { int_part }, dec_part);
            sign * num.parse::<f64>().unwrap_or(0.0)
        };

        if self.unit == Unit::Inch {
            val *= 25.4;
        }
        val
    }
}
