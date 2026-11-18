//! メインアプリケーション UI

use std::sync::mpsc::{self, Receiver};
use std::thread;

use eframe::egui;

use crate::config::{AppProjectConfig, EdgeCutMode, EdgeGapMode, LayerConfig, ToolPreset};
use crate::core::excellon::ExcellonParser;
use crate::core::geometry::{Bounds, DrillData, DrillHole, Geometry, Point2, Toolpath};
use crate::core::gerber::GerberParser;
use crate::core::mirror::{mirror_drill_data, mirror_geometry, MirrorAxis};
use crate::core::toolpath::{DrcReport, ToolpathGenerator};
use crate::gcode::generator::{parse_gcode_for_sim, GCodeGenerator, GCodeSettings, GCodeStats};
use crate::ui::dialogs::{self, DialogsState, HelpTab};
use crate::ui::preview_2d::{Preview2DParams, Preview2DState};
use crate::ui::preview_3d::Preview3DState;
use crate::ui::simulator::SimulatorState;
use crate::updater::{self, UpdateInfo};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum LayerTab {
    BCu,
    EdgeCuts,
    Drill,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ViewTab {
    Preview2D,
    Preview3D,
    Simulation,
}

enum UpdateMessage {
    Checked(Result<Option<UpdateInfo>, String>, bool), // (結果, 手動チェックか)
    InstallFailed(String),
}

pub struct AltairCamApp {
    config: AppProjectConfig,
    active_layer_tab: LayerTab,
    active_view_tab: ViewTab,

    // パース済みデータ
    b_cu_data: Option<Geometry>,
    edge_cuts_data: Option<Geometry>,
    drill_data: Option<DrillData>,

    // 生成済みツールパス（プレビュー用・出力用）
    b_cu_toolpaths: Vec<Toolpath>,
    edge_toolpaths: Vec<Toolpath>,
    drill_ordered_holes: Vec<DrillHole>,
    drill_plunge_holes: Vec<DrillHole>,
    drill_mill_toolpaths: Vec<Toolpath>,

    // 加工時間統計・DRC・プリセット状態
    b_cu_stats: GCodeStats,
    edge_stats: GCodeStats,
    drill_stats: GCodeStats,
    drc_report: Option<DrcReport>,
    selected_preset_idx: usize,
    new_preset_name: String,

    // 表示コンポーネント
    preview_2d: Preview2DState,
    preview_3d: Preview3DState,
    simulator: SimulatorState,

    // ログとダイアログ
    logs: Vec<String>,
    dialogs: DialogsState,
    update_dialog: Option<UpdateInfo>,
    update_status_text: Option<String>,
    update_rx: Option<Receiver<UpdateMessage>>,
    updating_in_progress: bool,
}

impl AltairCamApp {
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        let config = AppProjectConfig::default_config_path()
            .and_then(|p| AppProjectConfig::load_from_file(p).ok())
            .unwrap_or_default();

        let mut app = Self {
            config,
            active_layer_tab: LayerTab::BCu,
            active_view_tab: ViewTab::Preview2D,
            b_cu_data: None,
            edge_cuts_data: None,
            drill_data: None,
            b_cu_toolpaths: Vec::new(),
            edge_toolpaths: Vec::new(),
            drill_ordered_holes: Vec::new(),
            drill_plunge_holes: Vec::new(),
            drill_mill_toolpaths: Vec::new(),
            b_cu_stats: GCodeStats::default(),
            edge_stats: GCodeStats::default(),
            drill_stats: GCodeStats::default(),
            drc_report: None,
            selected_preset_idx: 0,
            new_preset_name: String::new(),
            preview_2d: Preview2DState::default(),
            preview_3d: Preview3DState::default(),
            simulator: SimulatorState::default(),
            logs: vec![format!("AltairCAM v{} を起動しました。", env!("APP_VERSION"))],
            dialogs: DialogsState::default(),
            update_dialog: None,
            update_status_text: None,
            update_rx: None,
            updating_in_progress: false,
        };

        if app.config.auto_check_updates {
            app.start_update_check(false, cc.egui_ctx.clone());
        }

        app
    }

    fn log(&mut self, msg: impl Into<String>) {
        self.logs.push(msg.into());
        if self.logs.len() > 200 {
            self.logs.remove(0);
        }
    }

    fn save_default_config(&self) {
        if let Some(path) = AppProjectConfig::default_config_path() {
            let _ = self.config.save_to_file(path);
        }
    }

    fn start_update_check(&mut self, manual: bool, ctx: egui::Context) {
        if self.updating_in_progress {
            return;
        }
        let (tx, rx) = mpsc::channel();
        self.update_rx = Some(rx);
        if manual {
            self.update_status_text = Some("アップデートを確認しています...".to_string());
        }
        thread::spawn(move || {
            let res = updater::check_for_updates();
            let _ = tx.send(UpdateMessage::Checked(res, manual));
            ctx.request_repaint();
        });
    }

    fn start_update_install(&mut self, url: String, ctx: egui::Context) {
        if self.updating_in_progress {
            return;
        }
        self.updating_in_progress = true;
        self.update_status_text = Some("更新ファイルをダウンロード中...".to_string());
        let (tx, rx) = mpsc::channel();
        self.update_rx = Some(rx);
        thread::spawn(move || {
            if let Err(e) = updater::download_and_install_update(&url) {
                let _ = tx.send(UpdateMessage::InstallFailed(e));
                ctx.request_repaint();
            }
        });
    }

    fn poll_update_messages(&mut self) {
        let mut msgs = Vec::new();
        if let Some(rx) = &self.update_rx {
            while let Ok(msg) = rx.try_recv() {
                msgs.push(msg);
            }
        }
        if msgs.is_empty() {
            return;
        }
        for msg in msgs {
            match msg {
                UpdateMessage::Checked(Ok(Some(info)), _) => {
                    self.update_status_text =
                        Some(format!("新しいバージョン v{} が利用可能です", info.latest_version));
                    self.log(format!("アップデート検出: v{}", info.latest_version));
                    self.update_dialog = Some(info);
                }
                UpdateMessage::Checked(Ok(None), manual) => {
                    if manual {
                        self.update_status_text = Some("最新バージョンです。".to_string());
                        self.log("アップデート確認: 最新バージョンです。");
                    } else {
                        self.update_status_text = None;
                    }
                }
                UpdateMessage::Checked(Err(e), manual) => {
                    if manual {
                        self.update_status_text = Some(format!("確認失敗: {}", e));
                        self.log(format!("アップデート確認失敗: {}", e));
                    } else {
                        self.update_status_text = None;
                    }
                }
                UpdateMessage::InstallFailed(e) => {
                    self.updating_in_progress = false;
                    self.update_status_text = Some(format!("更新エラー: {}", e));
                    self.log(format!("更新エラー: {}", e));
                }
            }
        }
        if !self.updating_in_progress {
            self.update_rx = None;
        }
    }

    fn total_bounds(&self) -> Bounds {
        let mut b = Bounds::empty();
        if self.config.b_cu.enabled {
            if let Some(g) = &self.b_cu_data {
                b.merge(&g.bounds());
            }
        }
        if self.config.edge_cuts.enabled {
            if let Some(g) = &self.edge_cuts_data {
                b.merge(&g.bounds());
            }
        }
        if self.config.drill.enabled {
            if let Some(d) = &self.drill_data {
                b.merge(&d.bounds());
            }
        }
        b
    }

    fn load_all_files(&mut self) {
        self.b_cu_data = None;
        self.edge_cuts_data = None;
        self.drill_data = None;
        self.b_cu_toolpaths.clear();
        self.edge_toolpaths.clear();
        self.drill_ordered_holes.clear();
        self.drill_plunge_holes.clear();
        self.drill_mill_toolpaths.clear();
        self.drc_report = None;
        self.b_cu_stats = GCodeStats::default();
        self.edge_stats = GCodeStats::default();
        self.drill_stats = GCodeStats::default();

        if self.config.b_cu.enabled && !self.config.b_cu.filepath.is_empty() {
            match GerberParser::parse_file(&self.config.b_cu.filepath) {
                Ok(g) => {
                    self.log(format!("B_Cu 読込: {}", g.summary()));
                    self.b_cu_data = Some(g);
                }
                Err(e) => self.log(format!("B_Cu エラー: {}", e)),
            }
        }

        if self.config.edge_cuts.enabled && !self.config.edge_cuts.filepath.is_empty() {
            match GerberParser::parse_file(&self.config.edge_cuts.filepath) {
                Ok(g) => {
                    self.log(format!("Edge_Cuts 読込: {}", g.summary()));
                    self.edge_cuts_data = Some(g);
                }
                Err(e) => self.log(format!("Edge_Cuts エラー: {}", e)),
            }
        }

        if self.config.drill.enabled && !self.config.drill.filepath.is_empty() {
            match ExcellonParser::parse_file(&self.config.drill.filepath) {
                Ok(d) => {
                    self.log(format!("Drill 読込: {}", d.summary()));
                    self.drill_data = Some(d);
                }
                Err(e) => self.log(format!("Drill エラー: {}", e)),
            }
        }

        // 反転処理（統一バウンディングボックス中心を基準）
        let bounds = self.total_bounds();
        if bounds.is_valid() {
            let center = bounds.center();
            if let Some(ref mut g) = self.b_cu_data {
                let axis = self.config.b_cu.mirror_axis;
                if axis != MirrorAxis::None {
                    let r = if axis == MirrorAxis::X { center.y } else { center.x };
                    *g = mirror_geometry(g, axis, r);
                }
            }
            if let Some(ref mut g) = self.edge_cuts_data {
                let axis = self.config.edge_cuts.mirror_axis;
                if axis != MirrorAxis::None {
                    let r = if axis == MirrorAxis::X { center.y } else { center.x };
                    *g = mirror_geometry(g, axis, r);
                }
            }
            if let Some(ref mut d) = self.drill_data {
                let axis = self.config.drill.mirror_axis;
                if axis != MirrorAxis::None {
                    let r = if axis == MirrorAxis::X { center.y } else { center.x };
                    *d = mirror_drill_data(d, axis, r);
                }
            }

            if self.config.zero_origin_on_load {
                self.shift_origin_to_zero();
            }
        }

        self.regenerate_all_toolpaths();
        self.preview_2d.needs_fit = true;
        self.save_default_config();
    }

    fn shift_origin_to_zero(&mut self) {
        let bounds = self.total_bounds();
        if !bounds.is_valid() {
            return;
        }
        let dx = -bounds.min_x;
        let dy = -bounds.min_y;
        if dx.abs() < 1e-6 && dy.abs() < 1e-6 {
            return;
        }
        if let Some(ref mut g) = self.b_cu_data {
            g.translate(dx, dy);
        }
        if let Some(ref mut g) = self.edge_cuts_data {
            g.translate(dx, dy);
        }
        if let Some(ref mut d) = self.drill_data {
            d.translate(dx, dy);
        }
        self.log(format!("原点を左下に移動しました (dx={:.2}, dy={:.2})", dx, dy));
    }

    fn run_b_cu_drc(&mut self) {
        if let Some(ref g) = self.b_cu_data {
            let gen = ToolpathGenerator::new(self.config.b_cu.tool_diameter);
            let report = gen.run_drc(g, self.config.b_cu.drc_min_trace_width);
            if !report.uncut_bridges.is_empty() || !report.narrow_traces.is_empty() {
                self.log(format!(
                    "DRC チェック: 加工不能 {} 件, 細線 {} 件",
                    report.uncut_bridges.len(),
                    report.narrow_traces.len()
                ));
            }
            self.drc_report = Some(report);
        } else {
            self.drc_report = None;
        }
    }

    fn update_gcode_stats(&mut self) {
        self.b_cu_stats = if self.config.b_cu.enabled {
            self.build_gcode_for_tab(LayerTab::BCu)
                .map(|lines| parse_gcode_for_sim(&lines).1)
                .unwrap_or_default()
        } else {
            GCodeStats::default()
        };

        self.edge_stats = if self.config.edge_cuts.enabled {
            self.build_gcode_for_tab(LayerTab::EdgeCuts)
                .map(|lines| parse_gcode_for_sim(&lines).1)
                .unwrap_or_default()
        } else {
            GCodeStats::default()
        };

        self.drill_stats = if self.config.drill.enabled {
            self.build_gcode_for_tab(LayerTab::Drill)
                .map(|lines| parse_gcode_for_sim(&lines).1)
                .unwrap_or_default()
        } else {
            GCodeStats::default()
        };
    }

    fn regenerate_all_toolpaths(&mut self) {
        if let Some(ref g) = self.b_cu_data {
            let gen = ToolpathGenerator::new(self.config.b_cu.tool_diameter);
            self.b_cu_toolpaths = gen.generate_isolation_toolpaths(
                g,
                self.config.b_cu.isolation_passes,
                self.config.b_cu.isolation_overlap,
                self.config.b_cu.optimize_toolpath,
            );
            self.log(format!("B_Cu パス生成: {} ループ", self.b_cu_toolpaths.len()));
            self.run_b_cu_drc();
        } else {
            self.b_cu_toolpaths.clear();
            self.drc_report = None;
        }

        if let Some(ref g) = self.edge_cuts_data {
            let gen = ToolpathGenerator::new(self.config.edge_cuts.tool_diameter);
            self.edge_toolpaths = gen.generate_edge_cuts_toolpaths_with_options(
                g,
                self.config.edge_cuts.edge_cut_mode,
                self.config.edge_cuts.edge_margin,
                self.config.edge_cuts.edge_gap_mode,
                self.config.edge_cuts.edge_gap_size,
                self.config.edge_cuts.optimize_toolpath,
            );
            self.log(format!("Edge_Cuts パス生成: {} パス", self.edge_toolpaths.len()));
        } else {
            self.edge_toolpaths.clear();
        }

        if let Some(ref d) = self.drill_data {
            if self.config.drill.optimize_toolpath {
                let (ordered, orig, opt) = ToolpathGenerator::optimize_drill_holes(d);
                self.drill_ordered_holes = ordered;
                if orig > 0.0 {
                    let pct = ((orig - opt) / orig) * 100.0;
                    self.log(format!(
                        "Drill 最適化: {:.1}mm -> {:.1}mm ({:.1}% 削減)",
                        orig, opt, pct
                    ));
                }
            } else {
                self.drill_ordered_holes = d.holes.clone();
            }

            if self.config.drill.mill_holes {
                let gen = ToolpathGenerator::new(self.config.drill.tool_diameter);
                let (plunge, mill_paths) =
                    gen.split_and_generate_mill_holes(&self.drill_ordered_holes);
                self.drill_plunge_holes = plunge;
                self.drill_mill_toolpaths = mill_paths;
                self.log(format!(
                    "Drill MillHoles: 通常穴 {} 個, くり抜き円パス {} 周",
                    self.drill_plunge_holes.len(),
                    self.drill_mill_toolpaths.len()
                ));
            } else {
                self.drill_plunge_holes = self.drill_ordered_holes.clone();
                self.drill_mill_toolpaths.clear();
            }
        } else {
            self.drill_ordered_holes.clear();
            self.drill_plunge_holes.clear();
            self.drill_mill_toolpaths.clear();
        }

        self.update_gcode_stats();
    }

    fn build_gcode_for_tab(&self, tab: LayerTab) -> Result<Vec<String>, String> {
        let cfg = match tab {
            LayerTab::BCu => &self.config.b_cu,
            LayerTab::EdgeCuts => &self.config.edge_cuts,
            LayerTab::Drill => &self.config.drill,
        };

        let settings = GCodeSettings {
            feed_rate: cfg.feed_rate,
            rapid_feed_rate: cfg.rapid_feed_rate,
            plunge_feed_rate: cfg.plunge_feed_rate,
            travel_z: cfg.travel_z,
            safe_z: cfg.safe_z,
            cut_z: cfg.cut_depth,
            pass_depth: cfg.pass_depth,
            spindle_speed: cfg.spindle_speed,
        };
        let gen = GCodeGenerator::new(settings);

        match tab {
            LayerTab::BCu => {
                if self.b_cu_toolpaths.is_empty() {
                    return Err("B_Cu のツールパスがありません。先にファイルを読み込んでください。".to_string());
                }
                Ok(gen.generate_paths_gcode(&self.b_cu_toolpaths, 0.005))
            }
            LayerTab::EdgeCuts => {
                if self.edge_toolpaths.is_empty() {
                    return Err("Edge_Cuts のツールパスがありません。先にファイルを読み込んでください。".to_string());
                }
                Ok(gen.generate_paths_gcode(&self.edge_toolpaths, 0.005))
            }
            LayerTab::Drill => {
                let Some(d) = &self.drill_data else {
                    return Err("Drill データがありません。先にファイルを読み込んでください。".to_string());
                };
                if cfg.mill_holes {
                    Ok(gen.generate_drill_and_mill_gcode(
                        &self.drill_plunge_holes,
                        &d.slots,
                        &self.drill_mill_toolpaths,
                    ))
                } else {
                    Ok(gen.generate_drill_gcode(&self.drill_ordered_holes, &d.slots))
                }
            }
        }
    }

    fn export_gcode_for_tab(&mut self, tab: LayerTab) {
        let default_name = match tab {
            LayerTab::BCu => "b_cu.nc",
            LayerTab::EdgeCuts => "edge_cuts.nc",
            LayerTab::Drill => "drill.nc",
        };
        match self.build_gcode_for_tab(tab) {
            Ok(lines) => {
                if let Some(path) = rfd::FileDialog::new()
                    .set_file_name(default_name)
                    .add_filter("G-Code", &["nc", "gcode"])
                    .save_file()
                {
                    match GCodeGenerator::save_to_file(&lines, &path) {
                        Ok(()) => {
                            self.log(format!("保存完了 ({} 行): {}", lines.len(), path.display()));
                        }
                        Err(e) => self.log(e),
                    }
                }
            }
            Err(e) => self.log(e),
        }
    }

    fn export_all_gcodes_to_folder(&mut self) {
        self.regenerate_all_toolpaths();
        let Some(folder) = rfd::FileDialog::new().pick_folder() else {
            return;
        };
        let targets = [
            (LayerTab::BCu, "b_cu.nc", self.config.b_cu.enabled),
            (LayerTab::EdgeCuts, "edge_cuts.nc", self.config.edge_cuts.enabled),
            (LayerTab::Drill, "drill.nc", self.config.drill.enabled),
        ];
        for (tab, filename, enabled) in targets {
            if !enabled {
                continue;
            }
            if let Ok(lines) = self.build_gcode_for_tab(tab) {
                let out_path = folder.join(filename);
                match GCodeGenerator::save_to_file(&lines, &out_path) {
                    Ok(()) => self.log(format!("一括出力: {}", out_path.display())),
                    Err(e) => self.log(e),
                }
            }
        }
    }
}

impl eframe::App for AltairCamApp {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        self.poll_update_messages();
        let ctx = ui.ctx().clone();

        // 上部メニューバー
        egui::Panel::top("menu_bar").show(ui, |ui| {
            egui::MenuBar::new().ui(ui, |ui| {
                ui.menu_button("ファイル", |ui| {
                    if ui.button("すべて読み込み").clicked() {
                        self.load_all_files();
                        ui.close();
                    }
                    ui.separator();
                    if ui.button("プロジェクト設定を保存...").clicked() {
                        if let Some(path) = rfd::FileDialog::new()
                            .set_file_name("altaircam_project.json")
                            .add_filter("JSON", &["json"])
                            .save_file()
                        {
                            match self.config.save_to_file(&path) {
                                Ok(()) => self.log(format!("設定保存: {}", path.display())),
                                Err(e) => self.log(e),
                            }
                        }
                        ui.close();
                    }
                    if ui.button("プロジェクト設定を読込...").clicked() {
                        if let Some(path) = rfd::FileDialog::new()
                            .add_filter("JSON", &["json"])
                            .pick_file()
                        {
                            match AppProjectConfig::load_from_file(&path) {
                                Ok(cfg) => {
                                    self.config = cfg;
                                    self.log(format!("設定読込: {}", path.display()));
                                    self.load_all_files();
                                }
                                Err(e) => self.log(e),
                            }
                        }
                        ui.close();
                    }
                    ui.separator();
                    if ui.button("終了").clicked() {
                        self.save_default_config();
                        ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                    }
                });

                ui.menu_button("ツール", |ui| {
                    if ui.button("原点を左下に合わせる (0,0)").clicked() {
                        self.shift_origin_to_zero();
                        self.regenerate_all_toolpaths();
                        self.preview_2d.needs_fit = true;
                        ui.close();
                    }
                    if ui.button("Gコードをフォルダへ一括出力...").clicked() {
                        self.export_all_gcodes_to_folder();
                        ui.close();
                    }
                });

                ui.menu_button("ヘルプ", |ui| {
                    if ui.button("使い方・基本操作").clicked() {
                        self.dialogs.show_help = true;
                        self.dialogs.help_tab = HelpTab::BasicUsage;
                        ui.close();
                    }
                    if ui.button("パラメーター解説").clicked() {
                        self.dialogs.show_help = true;
                        self.dialogs.help_tab = HelpTab::Parameters;
                        ui.close();
                    }
                    ui.separator();
                    if ui.button("問題を報告 (GitHub Issues)...").clicked() {
                        self.dialogs.issue_report.open = true;
                        ui.close();
                    }
                    if ui.button("アップデートを確認").clicked() {
                        self.start_update_check(true, ctx.clone());
                        ui.close();
                    }
                    ui.separator();
                    if ui.button("ライセンス・権利情報").clicked() {
                        self.dialogs.show_license = true;
                        ui.close();
                    }
                    if ui.button("バージョン情報").clicked() {
                        self.dialogs.show_about = true;
                        ui.close();
                    }
                });

                if let Some(ref status) = self.update_status_text {
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        ui.label(status);
                    });
                }
            });
        });

        // 左パネル（設定・操作）
        egui::Panel::left("control_panel")
            .default_size(350.0)
            .size_range(310.0..=480.0)
            .show(ui, |ui| {
                // グローバル操作バー
                ui.add_space(4.0);
                ui.horizontal(|ui| {
                    if ui.button("すべて読み込み").clicked() {
                        self.load_all_files();
                    }
                    if ui.button("パス再生成").clicked() {
                        self.regenerate_all_toolpaths();
                    }
                    if ui.button("原点(0,0)合わせ").clicked() {
                        self.shift_origin_to_zero();
                        self.regenerate_all_toolpaths();
                        self.preview_2d.needs_fit = true;
                    }
                });
                ui.checkbox(&mut self.config.zero_origin_on_load, "読込時に自動で原点を(0,0)に合わせる");
                ui.separator();

                // レイヤー切替タブ
                ui.horizontal(|ui| {
                    ui.selectable_value(&mut self.active_layer_tab, LayerTab::BCu, "B_Cu (銅箔)");
                    ui.selectable_value(&mut self.active_layer_tab, LayerTab::EdgeCuts, "Edge_Cuts (外形)");
                    ui.selectable_value(&mut self.active_layer_tab, LayerTab::Drill, "Drill (ドリル)");
                });
                ui.horizontal(|ui| {
                    if ui
                        .small_button("? パラメーターの意味")
                        .on_hover_text("各パラメーターの意味・推奨値・注意点の一覧を開きます")
                        .clicked()
                    {
                        self.dialogs.show_help = true;
                        self.dialogs.help_tab = HelpTab::Parameters;
                    }
                });
                ui.separator();

                // ログ領域を先に下部に確保し、設定エリアとレイアウトが干渉しないようにする
                egui::Panel::bottom("control_log_panel")
                    .resizable(true)
                    .default_size(130.0)
                    .size_range(70.0..=280.0)
                    .show(ui, |ui| {
                        ui.add_space(4.0);
                        ui.label("ログ");
                        egui::ScrollArea::vertical()
                            .id_salt("log_scroll_area")
                            .stick_to_bottom(true)
                            .auto_shrink([false, false])
                            .show(ui, |ui| {
                                for line in &self.logs {
                                    ui.label(line);
                                }
                            });
                    });

                egui::ScrollArea::vertical()
                    .id_salt("layer_settings_scroll_area")
                    .auto_shrink([false, false])
                    .show(ui, |ui| {
                        let tab = self.active_layer_tab;
                        self.preset_selector_ui(ui, tab);
                        ui.separator();

                        let changed = match tab {
                            LayerTab::BCu => Self::layer_settings_ui(
                                ui,
                                &mut self.config.b_cu,
                                LayerTab::BCu,
                                &["gbr", "gtl", "gbl"],
                            ),
                            LayerTab::EdgeCuts => Self::layer_settings_ui(
                                ui,
                                &mut self.config.edge_cuts,
                                LayerTab::EdgeCuts,
                                &["gbr", "gm1", "gko"],
                            ),
                            LayerTab::Drill => Self::layer_settings_ui(
                                ui,
                                &mut self.config.drill,
                                LayerTab::Drill,
                                &["drl", "txt", "xln"],
                            ),
                        };

                        if changed {
                            self.save_default_config();
                            self.update_gcode_stats();
                        }

                        if tab == LayerTab::BCu {
                            self.drc_summary_ui(ui);
                        }

                        self.stats_summary_ui(ui, tab);

                        ui.add_space(6.0);
                        ui.horizontal(|ui| {
                            if ui.button("Gコード出力...").clicked() {
                                self.regenerate_all_toolpaths();
                                self.export_gcode_for_tab(tab);
                            }
                            if ui.button("シミュレーションで確認").clicked() {
                                self.regenerate_all_toolpaths();
                                match self.build_gcode_for_tab(tab) {
                                    Ok(lines) => {
                                        let (label, tool_dia) = match tab {
                                            LayerTab::BCu => {
                                                ("B_Cu", self.config.b_cu.tool_diameter)
                                            }
                                            LayerTab::EdgeCuts => (
                                                "Edge_Cuts",
                                                self.config.edge_cuts.tool_diameter,
                                            ),
                                            LayerTab::Drill => {
                                                ("Drill", self.config.drill.tool_diameter)
                                            }
                                        };
                                        self.simulator
                                            .load_gcode_lines_with_tool(label, &lines, tool_dia);
                                        self.active_view_tab = ViewTab::Simulation;
                                    }
                                    Err(e) => self.log(e),
                                }
                            }
                        });
                    });
            });

        // 右パネル（プレビュー・シミュレーター）
        egui::CentralPanel::default().show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.selectable_value(&mut self.active_view_tab, ViewTab::Preview2D, "2D プレビュー");
                ui.selectable_value(&mut self.active_view_tab, ViewTab::Preview3D, "3D ビュー");
                ui.selectable_value(&mut self.active_view_tab, ViewTab::Simulation, "切削シミュレーション");
            });
            ui.separator();

            let bounds = self.total_bounds();
            let b_cu = if self.config.b_cu.enabled {
                self.b_cu_data.as_ref()
            } else {
                None
            };
            let edge = if self.config.edge_cuts.enabled {
                self.edge_cuts_data.as_ref()
            } else {
                None
            };
            let drill = if self.config.drill.enabled {
                self.drill_data.as_ref()
            } else {
                None
            };

            match self.active_view_tab {
                ViewTab::Preview2D => {
                    let drill_pts: Vec<Point2> = self
                        .drill_plunge_holes
                        .iter()
                        .map(|h| h.position)
                        .collect();
                    let drc_violations = self
                        .drc_report
                        .as_ref()
                        .map(|r| r.all_violations())
                        .unwrap_or_default();
                    let params = Preview2DParams {
                        b_cu,
                        edge_cuts: edge,
                        drill,
                        b_cu_paths: &self.b_cu_toolpaths,
                        edge_paths: &self.edge_toolpaths,
                        drill_order: &drill_pts,
                        drill_mill_paths: &self.drill_mill_toolpaths,
                        b_cu_tool_dia: self.config.b_cu.tool_diameter,
                        edge_tool_dia: self.config.edge_cuts.tool_diameter,
                        drill_tool_dia: self.config.drill.tool_diameter,
                        drc_violations: &drc_violations,
                        total_bounds: bounds,
                    };
                    self.preview_2d.ui(ui, params);
                }
                ViewTab::Preview3D => {
                    self.preview_3d.ui(
                        ui,
                        b_cu,
                        edge,
                        drill,
                        &self.b_cu_toolpaths,
                        &self.edge_toolpaths,
                        self.config.b_cu.cut_depth,
                        self.config.edge_cuts.cut_depth,
                        self.config.drill.cut_depth,
                        bounds,
                    );
                }
                ViewTab::Simulation => {
                    self.simulator.ui(ui, &ctx);
                }
            }
        });

        // ダイアログ群
        self.render_dialogs(&ctx);
    }

    fn on_exit(&mut self) {
        self.save_default_config();
    }
}

impl AltairCamApp {
    fn layer_name(tab: LayerTab) -> &'static str {
        match tab {
            LayerTab::BCu => "B_Cu",
            LayerTab::EdgeCuts => "Edge_Cuts",
            LayerTab::Drill => "Drill",
        }
    }

    fn preset_selector_ui(&mut self, ui: &mut egui::Ui, tab: LayerTab) {
        let target_layer = Self::layer_name(tab);
        let matching_indices: Vec<usize> = self
            .config
            .presets
            .iter()
            .enumerate()
            .filter(|(_, p)| p.target_layer == target_layer)
            .map(|(i, _)| i)
            .collect();

        egui::CollapsingHeader::new("切削条件プリセット")
            .default_open(false)
            .show(ui, |ui| {
                if !matching_indices.is_empty() {
                    if !matching_indices.contains(&self.selected_preset_idx) {
                        self.selected_preset_idx = matching_indices[0];
                    }
                    let selected_name = self.config.presets[self.selected_preset_idx].name.clone();
                    ui.horizontal(|ui| {
                        egui::ComboBox::from_id_salt(format!("preset_combo_{:?}", tab))
                            .selected_text(selected_name)
                            .width(170.0)
                            .show_ui(ui, |ui| {
                                for &idx in &matching_indices {
                                    let name = &self.config.presets[idx].name;
                                    ui.selectable_value(&mut self.selected_preset_idx, idx, name);
                                }
                            });
                        if ui
                            .button("適用")
                            .on_hover_text("選択中のプリセット条件をこのレイヤーに反映します")
                            .clicked()
                        {
                            let preset = self.config.presets[self.selected_preset_idx].clone();
                            match tab {
                                LayerTab::BCu => self.config.b_cu.apply_preset(&preset),
                                LayerTab::EdgeCuts => self.config.edge_cuts.apply_preset(&preset),
                                LayerTab::Drill => self.config.drill.apply_preset(&preset),
                            }
                            self.save_default_config();
                            self.regenerate_all_toolpaths();
                            self.log(format!("プリセット適用: {}", preset.name));
                        }
                        if ui
                            .button("削除")
                            .on_hover_text("選択中のプリセットを削除します")
                            .clicked()
                        {
                            let removed = self.config.presets.remove(self.selected_preset_idx);
                            self.selected_preset_idx = 0;
                            self.save_default_config();
                            self.log(format!("プリセット削除: {}", removed.name));
                        }
                    });
                } else {
                    ui.label("このレイヤー用のプリセットはありません。");
                }

                ui.add_space(4.0);
                ui.horizontal(|ui| {
                    ui.add(
                        egui::TextEdit::singleline(&mut self.new_preset_name)
                            .desired_width(170.0)
                            .hint_text("新しいプリセット名"),
                    );
                    if ui
                        .button("保存")
                        .on_hover_text("現在の切削条件を新しいプリセットとして保存します")
                        .clicked()
                    {
                        let trimmed = self.new_preset_name.trim().to_string();
                        if !trimmed.is_empty() {
                            let cfg = match tab {
                                LayerTab::BCu => &self.config.b_cu,
                                LayerTab::EdgeCuts => &self.config.edge_cuts,
                                LayerTab::Drill => &self.config.drill,
                            };
                            let preset =
                                ToolPreset::from_layer(trimmed.clone(), target_layer, cfg);
                            self.config.presets.push(preset);
                            self.selected_preset_idx = self.config.presets.len() - 1;
                            self.new_preset_name.clear();
                            self.save_default_config();
                            self.log(format!("プリセット保存: {}", trimmed));
                        }
                    }
                });
            });
    }

    fn drc_summary_ui(&mut self, ui: &mut egui::Ui) {
        ui.add_space(4.0);
        ui.group(|ui| {
            ui.horizontal(|ui| {
                ui.strong("DRC チェック結果 (B_Cu)");
                if ui
                    .small_button("再チェック")
                    .on_hover_text("現在のツール径と基準値でクリアランス・細線を再検査します")
                    .clicked()
                {
                    self.run_b_cu_drc();
                }
            });

            if let Some(ref rep) = self.drc_report {
                let width_str = rep
                    .min_trace_width_mm
                    .map(|v| format!("{:.3} mm", v))
                    .unwrap_or_else(|| "---".to_string());
                let clear_str = rep
                    .min_clearance_mm
                    .map(|v| format!("{:.3} mm", v))
                    .unwrap_or_else(|| "---".to_string());
                ui.label(format!("最小パターン幅: {} / 最小クリアランス: {}", width_str, clear_str));

                if rep.uncut_bridges.is_empty() && rep.narrow_traces.is_empty() {
                    ui.colored_label(
                        egui::Color32::from_rgb(20, 130, 60),
                        "問題なし（現在のツール径で加工可能です）",
                    );
                } else {
                    if !rep.uncut_bridges.is_empty() {
                        ui.colored_label(
                            egui::Color32::from_rgb(200, 40, 40),
                            format!(
                                "加工不能箇所 (ツール径不足): {} 箇所",
                                rep.uncut_bridges.len()
                            ),
                        );
                    }
                    if !rep.narrow_traces.is_empty() {
                        ui.colored_label(
                            egui::Color32::from_rgb(200, 110, 20),
                            format!("基準幅未満の細線: {} 箇所", rep.narrow_traces.len()),
                        );
                    }
                }
            } else {
                ui.label("B_Cu 読み込み後に自動チェックされます。");
            }
        });
    }

    fn stats_summary_ui(&self, ui: &mut egui::Ui, tab: LayerTab) {
        let cur = match tab {
            LayerTab::BCu => &self.b_cu_stats,
            LayerTab::EdgeCuts => &self.edge_stats,
            LayerTab::Drill => &self.drill_stats,
        };
        let total_min = self.b_cu_stats.estimated_minutes
            + self.edge_stats.estimated_minutes
            + self.drill_stats.estimated_minutes;

        ui.add_space(4.0);
        ui.group(|ui| {
            ui.label(format!(
                "{} 見積もり: 切削 {:.1}mm / 早送り {:.1}mm / 約 {:.1} 分",
                Self::layer_name(tab),
                cur.cut_distance_mm,
                cur.rapid_distance_mm,
                cur.estimated_minutes
            ));
            ui.label(format!("全有効レイヤー合計の推定加工時間: 約 {:.1} 分", total_min));
        });
    }

    fn layer_settings_ui(
        ui: &mut egui::Ui,
        cfg: &mut LayerConfig,
        tab: LayerTab,
        extensions: &[&str],
    ) -> bool {
        let mut changed = false;

        changed |= ui
            .checkbox(&mut cfg.enabled, "このレイヤーを有効にする")
            .on_hover_text("オフにするとプレビュー表示や一括Gコード出力の対象から外れます。")
            .changed();

        ui.horizontal(|ui| {
            ui.label("ファイル:")
                .on_hover_text("読み込むガーバー (.gbr 等) またはドリル (.drl 等) ファイルのパスです。");
            changed |= ui
                .add(egui::TextEdit::singleline(&mut cfg.filepath).desired_width(180.0))
                .changed();
            if ui.button("参照...").clicked() {
                if let Some(path) = rfd::FileDialog::new()
                    .add_filter("PCB File", extensions)
                    .pick_file()
                {
                    cfg.filepath = path.display().to_string();
                    changed = true;
                }
            }
        });

        ui.horizontal(|ui| {
            ui.label("反転:")
                .on_hover_text("基準点を中心に図形を反転します。裏面加工時はY軸またはX軸反転を使用します。");
            changed |= ui
                .radio_value(&mut cfg.mirror_axis, MirrorAxis::None, "なし")
                .changed();
            changed |= ui
                .radio_value(&mut cfg.mirror_axis, MirrorAxis::X, "X軸")
                .on_hover_text("上下反転します（水平線を軸に反転）。")
                .changed();
            changed |= ui
                .radio_value(&mut cfg.mirror_axis, MirrorAxis::Y, "Y軸")
                .on_hover_text("左右反転します（垂直線を軸に反転）。裏面(B_Cu)加工でよく使われます。")
                .changed();
        });

        ui.separator();

        egui::Grid::new(format!("grid_{:?}", tab))
            .num_columns(2)
            .spacing([12.0, 6.0])
            .show(ui, |ui| {
                ui.label("ツール直径 (mm):")
                    .on_hover_text("使用するエンドミルやドリルビットの刃径です。B_Cuは0.1〜0.2mm、Edge_Cutsは0.8〜2.0mm、Drillは穴径に合わせます。");
                changed |= ui
                    .add(egui::DragValue::new(&mut cfg.tool_diameter).speed(0.02).range(0.01..=10.0))
                    .on_hover_text("使用するエンドミルやドリルビットの刃径 (mm)")
                    .changed();
                ui.end_row();

                ui.label("切削深さ (mm):")
                    .on_hover_text("最終的なZ軸の切り込み深さです（マイナス値）。B_Cuは -0.05〜-0.1mm、Edge_Cuts/Drillは基板厚+余裕 (-1.6〜-1.8mm)。");
                changed |= ui
                    .add(egui::DragValue::new(&mut cfg.cut_depth).speed(0.02).range(-10.0..=0.0))
                    .on_hover_text("目標のZ切削深さ (mm, 0以下)")
                    .changed();
                ui.end_row();

                ui.label("1回切り込み深さ (mm):")
                    .on_hover_text("多段切り込み時の1パスあたりの深さです。0にすると1回で目標深さまで削ります。外形カット時は0.3〜0.5mm推奨。");
                changed |= ui
                    .add(egui::DragValue::new(&mut cfg.pass_depth).speed(0.02).range(0.0..=5.0))
                    .on_hover_text("0にすると1回で目標深さまで削ります。外形カット時は0.3〜0.5mm推奨。")
                    .changed();
                ui.end_row();

                ui.label("送り速度 (mm/min):")
                    .on_hover_text("XY平面で切削しながら移動する速度です。目安: 50〜150 mm/min。速すぎると工具が折れる原因になります。");
                changed |= ui
                    .add(egui::DragValue::new(&mut cfg.feed_rate).speed(2.0).range(1.0..=3000.0))
                    .on_hover_text("切削時のXY送り速度 (mm/min)")
                    .changed();
                ui.end_row();

                ui.label("スピンドル (rpm):")
                    .on_hover_text("主軸の回転数 (Sコマンド) です。使用するCNCマシンと刃物に合わせて設定します。");
                changed |= ui
                    .add(egui::DragValue::new(&mut cfg.spindle_speed).speed(100.0).range(0..=30000))
                    .on_hover_text("スピンドル回転数 (rpm)")
                    .changed();
                ui.end_row();
            });

        ui.add_space(4.0);

        match tab {
            LayerTab::BCu => {
                egui::Grid::new("b_cu_specific")
                    .num_columns(2)
                    .spacing([12.0, 6.0])
                    .show(ui, |ui| {
                        ui.label("アイソレーション周回数:")
                            .on_hover_text("銅箔パターンの周囲を何周削って絶縁溝を広げるかを指定します（1〜10周）。");
                        changed |= ui
                            .add(egui::DragValue::new(&mut cfg.isolation_passes).range(1..=10))
                            .on_hover_text("銅箔パターン周囲の周回数 (1〜10)")
                            .changed();
                        ui.end_row();

                        ui.label("パス重なり率:")
                            .on_hover_text("複数周回削る際の隣接パス同士の重なり割合 (0.0〜0.8) です。0.2〜0.3程度で削り残しを防ぎます。");
                        changed |= ui
                            .add(
                                egui::DragValue::new(&mut cfg.isolation_overlap)
                                    .speed(0.05)
                                    .range(0.0..=0.8),
                            )
                            .on_hover_text("隣接パスの重なり率 (0.0〜0.8)")
                            .changed();
                        ui.end_row();

                        ui.label("最小許容パターン幅 (mm):")
                            .on_hover_text("DRCチェックで警告を出す最小配線幅のしきい値です。");
                        changed |= ui
                            .add(
                                egui::DragValue::new(&mut cfg.drc_min_trace_width)
                                    .speed(0.01)
                                    .range(0.01..=2.0),
                            )
                            .on_hover_text("この幅未満の配線をDRCで警告表示します (mm)")
                            .changed();
                        ui.end_row();
                    });
                changed |= ui
                    .checkbox(&mut cfg.optimize_toolpath, "パス順序を最適化する")
                    .on_hover_text("切削パス間の空走移動距離が短くなるように順序を並べ替えます。")
                    .changed();
            }
            LayerTab::EdgeCuts => {
                ui.horizontal(|ui| {
                    ui.label("カット位置:")
                        .on_hover_text("外形線に対してツール中心を通す位置を選びます。外側カットで基板寸法が設計通りになります。");
                    changed |= ui
                        .radio_value(&mut cfg.edge_cut_mode, EdgeCutMode::Outside, "外側")
                        .on_hover_text("ツール半径分だけ外側を削り、完成基板の寸法を維持します。")
                        .changed();
                    changed |= ui
                        .radio_value(&mut cfg.edge_cut_mode, EdgeCutMode::Inside, "内側")
                        .on_hover_text("ツール半径分だけ内側を削ります（窓抜き加工向け）。")
                        .changed();
                    changed |= ui
                        .radio_value(&mut cfg.edge_cut_mode, EdgeCutMode::OnPath, "線上")
                        .on_hover_text("外形線の中心上をそのまま削ります。")
                        .changed();
                });

                egui::Grid::new("edge_cuts_gap_grid")
                    .num_columns(2)
                    .spacing([12.0, 6.0])
                    .show(ui, |ui| {
                        ui.label("外周マージン (mm):")
                            .on_hover_text("外形カット位置に追加する余白距離です。基板外周を少し大きめに切りたい場合に使います。");
                        changed |= ui
                            .add(
                                egui::DragValue::new(&mut cfg.edge_margin)
                                    .speed(0.05)
                                    .range(0.0..=10.0),
                            )
                            .on_hover_text("外形からの追加余白 (mm)")
                            .changed();
                        ui.end_row();

                        ui.label("保持タブ (Gaps):")
                            .on_hover_text("加工中に基板が外れて飛ぶのを防ぐため、一部を切り残すブリッジ配置を選択します。");
                        ui.horizontal(|ui| {
                            changed |= ui
                                .radio_value(&mut cfg.edge_gap_mode, EdgeGapMode::None, "なし")
                                .changed();
                            changed |= ui
                                .radio_value(
                                    &mut cfg.edge_gap_mode,
                                    EdgeGapMode::TopBottom,
                                    "上下2",
                                )
                                .changed();
                            changed |= ui
                                .radio_value(
                                    &mut cfg.edge_gap_mode,
                                    EdgeGapMode::LeftRight,
                                    "左右2",
                                )
                                .changed();
                            changed |= ui
                                .radio_value(
                                    &mut cfg.edge_gap_mode,
                                    EdgeGapMode::FourSides,
                                    "4方",
                                )
                                .changed();
                        });
                        ui.end_row();

                        ui.label("タブ幅 (mm):")
                            .on_hover_text("保持タブ（切り残し部分）の幅です。ツール直径分も自動的に考慮されます。");
                        changed |= ui
                            .add_enabled(
                                cfg.edge_gap_mode != EdgeGapMode::None,
                                egui::DragValue::new(&mut cfg.edge_gap_size)
                                    .speed(0.1)
                                    .range(0.2..=20.0),
                            )
                            .on_hover_text("切り残す保持タブの幅 (mm)")
                            .changed();
                        ui.end_row();
                    });

                changed |= ui
                    .checkbox(&mut cfg.optimize_toolpath, "パス順序を最適化する")
                    .on_hover_text("複数の外形パスがある場合に移動距離が短くなる順で加工します。")
                    .changed();
            }
            LayerTab::Drill => {
                changed |= ui
                    .checkbox(
                        &mut cfg.mill_holes,
                        "大きい穴をエンドミルでくり抜く (Mill Holes)",
                    )
                    .on_hover_text("ツール直径より大きい穴を、同心円ポケット加工でくり抜きます。刃物交換を減らせます。")
                    .changed();
                changed |= ui
                    .checkbox(&mut cfg.optimize_toolpath, "穴あけ順序を最適化する")
                    .on_hover_text("穴あけの移動順序を最適化し、移動距離と加工時間を短縮します。")
                    .changed();
            }
        }

        ui.add_space(4.0);
        egui::CollapsingHeader::new("詳細設定 (Z高さ / 移動速度)")
            .default_open(false)
            .show(ui, |ui| {
                egui::Grid::new(format!("detail_{:?}", tab))
                    .num_columns(2)
                    .spacing([12.0, 6.0])
                    .show(ui, |ui| {
                        ui.label("Safe Z (mm):")
                            .on_hover_text("加工開始・終了時や安全退避時のZ高さです。クランプに干渉しない高さ（例: 5.0mm）を設定します。");
                        changed |= ui
                            .add(egui::DragValue::new(&mut cfg.safe_z).speed(0.5).range(0.5..=50.0))
                            .on_hover_text("安全退避高さ Safe Z (mm)")
                            .changed();
                        ui.end_row();

                        ui.label("Travel Z (mm):")
                            .on_hover_text("切削箇所から次の切削箇所へXY移動する際のZ退避高さです（例: 2.0mm）。");
                        changed |= ui
                            .add(egui::DragValue::new(&mut cfg.travel_z).speed(0.5).range(0.2..=30.0))
                            .on_hover_text("移動時のZ退避高さ Travel Z (mm)")
                            .changed();
                        ui.end_row();

                        ui.label("早送り速度 (mm/min):")
                            .on_hover_text("空中移動時の速度です。");
                        changed |= ui
                            .add(
                                egui::DragValue::new(&mut cfg.rapid_feed_rate)
                                    .speed(10.0)
                                    .range(10.0..=5000.0),
                            )
                            .on_hover_text("非切削移動時の早送り速度 (mm/min)")
                            .changed();
                        ui.end_row();

                        ui.label("Z軸下降速度 (mm/min):")
                            .on_hover_text("Z軸を材料へ切り込む際（プランジ）の下降速度です。刃先保護のため送り速度より遅めを推奨します。");
                        changed |= ui
                            .add(
                                egui::DragValue::new(&mut cfg.plunge_feed_rate)
                                    .speed(2.0)
                                    .range(5.0..=1000.0),
                            )
                            .on_hover_text("Z軸切り込み時の下降速度 (mm/min)")
                            .changed();
                        ui.end_row();
                    });
            });

        changed
    }

    fn render_dialogs(&mut self, ctx: &egui::Context) {
        let mut install_url: Option<String> = None;
        dialogs::render_all_dialogs(
            ctx,
            &mut self.dialogs,
            &mut self.config,
            &self.logs,
            &mut self.update_dialog,
            &self.update_status_text,
            self.updating_in_progress,
            &mut |url| {
                install_url = Some(url);
            },
        );
        if let Some(url) = install_url {
            self.start_update_install(url, ctx.clone());
        }
    }
}
