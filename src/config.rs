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
}

impl LayerConfig {
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
}

impl Default for AppProjectConfig {
    fn default() -> Self {
        Self {
            b_cu: LayerConfig::default_b_cu(),
            edge_cuts: LayerConfig::default_edge_cuts(),
            drill: LayerConfig::default_drill(),
            zero_origin_on_load: true,
            auto_check_updates: true,
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
