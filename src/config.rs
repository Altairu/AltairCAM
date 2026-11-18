//! アプリケーションおよびレイヤー設定の永続化

use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};

use crate::core::mirror::MirrorAxis;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum EdgeCutMode {
    Outside, // 外側カット（基板寸法を維持）
    Inside,  // 内側カット
    OnPath,  // 線上カット（ツール中心が外形線上を通る）
}

impl Default for EdgeCutMode {
    fn default() -> Self {
        Self::Outside
    }
}

/// 基板外形カット時に残す保持タブ (Gaps) の配置モード
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum EdgeGapMode {
    None,      // タブなし
    TopBottom, // 上下 2箇所
    LeftRight, // 左右 2箇所
    FourSides, // 上下左右 4箇所
}

impl Default for EdgeGapMode {
    fn default() -> Self {
        Self::None
    }
}

fn default_edge_gap_size() -> f64 {
    2.0
}

fn default_min_trace_width() -> f64 {
    0.15
}

fn default_true() -> bool {
    true
}

/// 工具および切削条件のプリセット
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolPreset {
    pub name: String,
    pub target_layer: String, // "B_Cu", "Edge_Cuts", "Drill"
    pub tool_diameter: f64,
    pub cut_depth: f64,
    pub pass_depth: f64,
    pub feed_rate: f64,
    pub plunge_feed_rate: f64,
    pub rapid_feed_rate: f64,
    pub spindle_speed: u32,
    pub isolation_passes: usize,
    pub isolation_overlap: f64,
}

impl ToolPreset {
    /// レイヤー設定からプリセットを作成する
    pub fn from_layer(name: String, target_layer: &str, cfg: &LayerConfig) -> Self {
        Self {
            name,
            target_layer: target_layer.to_string(),
            tool_diameter: cfg.tool_diameter,
            cut_depth: cfg.cut_depth,
            pass_depth: cfg.pass_depth,
            feed_rate: cfg.feed_rate,
            plunge_feed_rate: cfg.plunge_feed_rate,
            rapid_feed_rate: cfg.rapid_feed_rate,
            spindle_speed: cfg.spindle_speed,
            isolation_passes: cfg.isolation_passes,
            isolation_overlap: cfg.isolation_overlap,
        }
    }
}

/// デフォルトの切削条件プリセット群を生成する
pub fn default_tool_presets() -> Vec<ToolPreset> {
    vec![
        ToolPreset {
            name: "B_Cu 標準 Vカッター (0.1mm)".to_string(),
            target_layer: "B_Cu".to_string(),
            tool_diameter: 0.1,
            cut_depth: -0.05,
            pass_depth: 0.0,
            feed_rate: 100.0,
            plunge_feed_rate: 50.0,
            rapid_feed_rate: 500.0,
            spindle_speed: 10000,
            isolation_passes: 1,
            isolation_overlap: 0.25,
        },
        ToolPreset {
            name: "B_Cu エンドミル (0.2mm / 2周)".to_string(),
            target_layer: "B_Cu".to_string(),
            tool_diameter: 0.2,
            cut_depth: -0.08,
            pass_depth: 0.0,
            feed_rate: 120.0,
            plunge_feed_rate: 50.0,
            rapid_feed_rate: 500.0,
            spindle_speed: 12000,
            isolation_passes: 2,
            isolation_overlap: 0.25,
        },
        ToolPreset {
            name: "Edge_Cuts FR-4 1.6mm用 (1.0mm)".to_string(),
            target_layer: "Edge_Cuts".to_string(),
            tool_diameter: 1.0,
            cut_depth: -1.7,
            pass_depth: 0.4,
            feed_rate: 80.0,
            plunge_feed_rate: 40.0,
            rapid_feed_rate: 500.0,
            spindle_speed: 10000,
            isolation_passes: 1,
            isolation_overlap: 0.0,
        },
        ToolPreset {
            name: "Edge_Cuts 細径エンドミル (0.8mm)".to_string(),
            target_layer: "Edge_Cuts".to_string(),
            tool_diameter: 0.8,
            cut_depth: -1.7,
            pass_depth: 0.3,
            feed_rate: 60.0,
            plunge_feed_rate: 30.0,
            rapid_feed_rate: 500.0,
            spindle_speed: 12000,
            isolation_passes: 1,
            isolation_overlap: 0.0,
        },
        ToolPreset {
            name: "Drill 標準 (0.8mm)".to_string(),
            target_layer: "Drill".to_string(),
            tool_diameter: 0.8,
            cut_depth: -1.7,
            pass_depth: 0.0,
            feed_rate: 50.0,
            plunge_feed_rate: 50.0,
            rapid_feed_rate: 500.0,
            spindle_speed: 10000,
            isolation_passes: 1,
            isolation_overlap: 0.0,
        },
    ]
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LayerConfig {
    pub name: String,
    pub filepath: String,
    pub enabled: bool,
    pub mirror_axis: MirrorAxis,
    pub tool_diameter: f64,
    pub cut_depth: f64,
    pub pass_depth: f64,      // Zステップダウン (0以下で1回切り込み)
    pub feed_rate: f64,
    pub safe_z: f64,
    pub travel_z: f64,
    pub rapid_feed_rate: f64,
    pub plunge_feed_rate: f64,
    pub spindle_speed: u32,
    pub optimize_toolpath: bool,
    pub isolation_passes: usize, // B_Cu用: アイソレーション周回数
    pub isolation_overlap: f64,  // 重なり率 (0.0 - 0.8)
    pub edge_cut_mode: EdgeCutMode,
    #[serde(default)]
    pub edge_margin: f64,        // Edge_Cuts用: 追加余白 (mm)
    #[serde(default)]
    pub edge_gap_mode: EdgeGapMode, // Edge_Cuts用: 保持タブ配置
    #[serde(default = "default_edge_gap_size")]
    pub edge_gap_size: f64,      // Edge_Cuts用: 保持タブ幅 (mm)
    #[serde(default)]
    pub mill_holes: bool,        // Drill用: ツール径より大きい穴をエンドミルでくり抜く
    #[serde(default = "default_min_trace_width")]
    pub drc_min_trace_width: f64, // B_Cu用: DRC最小許容パターン幅 (mm)
    #[serde(default = "default_true")]
    pub drc_enabled: bool,       // B_Cu用: DRCチェックを有効にする
}

impl LayerConfig {
    /// プリセットの切削条件をレイヤー設定へ適用する
    pub fn apply_preset(&mut self, preset: &ToolPreset) {
        self.tool_diameter = preset.tool_diameter;
        self.cut_depth = preset.cut_depth;
        self.pass_depth = preset.pass_depth;
        self.feed_rate = preset.feed_rate;
        self.plunge_feed_rate = preset.plunge_feed_rate;
        self.rapid_feed_rate = preset.rapid_feed_rate;
        self.spindle_speed = preset.spindle_speed;
        self.isolation_passes = preset.isolation_passes;
        self.isolation_overlap = preset.isolation_overlap;
    }

    pub fn default_b_cu() -> Self {
        Self {
            name: "B_Cu".to_string(),
            filepath: String::new(),
            enabled: true,
            mirror_axis: MirrorAxis::None,
            tool_diameter: 0.1,
            cut_depth: -0.05,
            pass_depth: 0.0,
            feed_rate: 100.0,
            safe_z: 5.0,
            travel_z: 2.0,
            rapid_feed_rate: 500.0,
            plunge_feed_rate: 50.0,
            spindle_speed: 10000,
            optimize_toolpath: true,
            isolation_passes: 1,
            isolation_overlap: 0.25,
            edge_cut_mode: EdgeCutMode::Outside,
            edge_margin: 0.0,
            edge_gap_mode: EdgeGapMode::None,
            edge_gap_size: default_edge_gap_size(),
            mill_holes: false,
            drc_min_trace_width: default_min_trace_width(),
            drc_enabled: true,
        }
    }

    pub fn default_edge_cuts() -> Self {
        Self {
            name: "Edge_Cuts".to_string(),
            filepath: String::new(),
            enabled: true,
            mirror_axis: MirrorAxis::None,
            tool_diameter: 1.0,
            cut_depth: -1.6,
            pass_depth: 0.4,
            feed_rate: 80.0,
            safe_z: 5.0,
            travel_z: 2.0,
            rapid_feed_rate: 500.0,
            plunge_feed_rate: 40.0,
            spindle_speed: 10000,
            optimize_toolpath: true,
            isolation_passes: 1,
            isolation_overlap: 0.0,
            edge_cut_mode: EdgeCutMode::Outside,
            edge_margin: 0.0,
            edge_gap_mode: EdgeGapMode::None,
            edge_gap_size: default_edge_gap_size(),
            mill_holes: false,
            drc_min_trace_width: default_min_trace_width(),
            drc_enabled: true,
        }
    }

    pub fn default_drill() -> Self {
        Self {
            name: "Drill".to_string(),
            filepath: String::new(),
            enabled: true,
            mirror_axis: MirrorAxis::None,
            tool_diameter: 0.8,
            cut_depth: -1.7,
            pass_depth: 0.0,
            feed_rate: 50.0,
            safe_z: 5.0,
            travel_z: 2.0,
            rapid_feed_rate: 500.0,
            plunge_feed_rate: 50.0,
            spindle_speed: 10000,
            optimize_toolpath: true,
            isolation_passes: 1,
            isolation_overlap: 0.0,
            edge_cut_mode: EdgeCutMode::OnPath,
            edge_margin: 0.0,
            edge_gap_mode: EdgeGapMode::None,
            edge_gap_size: default_edge_gap_size(),
            mill_holes: false,
            drc_min_trace_width: default_min_trace_width(),
            drc_enabled: true,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppProjectConfig {
    pub b_cu: LayerConfig,
    pub edge_cuts: LayerConfig,
    pub drill: LayerConfig,
    pub zero_origin_on_load: bool,
    pub auto_check_updates: bool,
    #[serde(default = "default_tool_presets")]
    pub presets: Vec<ToolPreset>,
}

impl Default for AppProjectConfig {
    fn default() -> Self {
        Self {
            b_cu: LayerConfig::default_b_cu(),
            edge_cuts: LayerConfig::default_edge_cuts(),
            drill: LayerConfig::default_drill(),
            zero_origin_on_load: true,
            auto_check_updates: true,
            presets: default_tool_presets(),
        }
    }
}

impl AppProjectConfig {
    pub fn default_config_path() -> Option<PathBuf> {
        if let Ok(appdata) = std::env::var("APPDATA") {
            let dir = PathBuf::from(appdata).join("AltairCAM");
            let _ = fs::create_dir_all(&dir);
            return Some(dir.join("config.json"));
        }
        std::env::current_exe()
            .ok()
            .and_then(|p| p.parent().map(|d| d.join("altaircam_config.json")))
    }

    pub fn load_from_file<P: AsRef<Path>>(path: P) -> Result<Self, String> {
        let data = fs::read_to_string(path.as_ref())
            .map_err(|e| format!("設定の読み込みに失敗しました: {}", e))?;
        serde_json::from_str(&data).map_err(|e| format!("設定の解析に失敗しました: {}", e))
    }

    pub fn save_to_file<P: AsRef<Path>>(&self, path: P) -> Result<(), String> {
        let json = serde_json::to_string_pretty(self)
            .map_err(|e| format!("設定のシリアライズに失敗しました: {}", e))?;
        fs::write(path.as_ref(), json)
            .map_err(|e| format!("設定の保存に失敗しました: {}", e))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_legacy_config_backward_compatibility_and_presets() {
        // 旧バージョンのフィールドのみを持つJSONでもエラーなく読み込めること
        let legacy_json = r#"{
            "b_cu": {
                "name": "B_Cu",
                "filepath": "",
                "enabled": true,
                "mirror_axis": "None",
                "tool_diameter": 0.15,
                "cut_depth": -0.05,
                "pass_depth": 0.0,
                "feed_rate": 100.0,
                "safe_z": 5.0,
                "travel_z": 2.0,
                "rapid_feed_rate": 500.0,
                "plunge_feed_rate": 50.0,
                "spindle_speed": 10000,
                "optimize_toolpath": true,
                "isolation_passes": 1,
                "isolation_overlap": 0.25,
                "edge_cut_mode": "Outside"
            },
            "edge_cuts": {
                "name": "Edge_Cuts",
                "filepath": "",
                "enabled": true,
                "mirror_axis": "None",
                "tool_diameter": 1.0,
                "cut_depth": -1.6,
                "pass_depth": 0.4,
                "feed_rate": 80.0,
                "safe_z": 5.0,
                "travel_z": 2.0,
                "rapid_feed_rate": 500.0,
                "plunge_feed_rate": 40.0,
                "spindle_speed": 10000,
                "optimize_toolpath": true,
                "isolation_passes": 1,
                "isolation_overlap": 0.0,
                "edge_cut_mode": "Outside"
            },
            "drill": {
                "name": "Drill",
                "filepath": "",
                "enabled": true,
                "mirror_axis": "None",
                "tool_diameter": 0.8,
                "cut_depth": -1.7,
                "pass_depth": 0.0,
                "feed_rate": 50.0,
                "safe_z": 5.0,
                "travel_z": 2.0,
                "rapid_feed_rate": 500.0,
                "plunge_feed_rate": 50.0,
                "spindle_speed": 10000,
                "optimize_toolpath": true,
                "isolation_passes": 1,
                "isolation_overlap": 0.0,
                "edge_cut_mode": "OnPath"
            },
            "zero_origin_on_load": true,
            "auto_check_updates": true
        }"#;

        let mut cfg: AppProjectConfig =
            serde_json::from_str(legacy_json).expect("旧設定JSONのデシリアライズに失敗");
        assert!(!cfg.presets.is_empty(), "初期プリセットが補完されること");
        assert_eq!(cfg.edge_cuts.edge_gap_mode, EdgeGapMode::None);
        assert!((cfg.edge_cuts.edge_gap_size - 2.0).abs() < 1e-6);
        assert!(!cfg.drill.mill_holes);
        assert!(cfg.b_cu.drc_enabled);

        let preset = cfg
            .presets
            .iter()
            .find(|p| p.target_layer == "B_Cu" && (p.tool_diameter - 0.2).abs() < 1e-6)
            .cloned()
            .expect("B_Cu 0.2mmプリセットが存在すること");
        cfg.b_cu.apply_preset(&preset);
        assert!((cfg.b_cu.tool_diameter - 0.2).abs() < 1e-6);
        assert_eq!(cfg.b_cu.isolation_passes, 2);
    }
}
