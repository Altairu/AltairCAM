//! メインアプリケーション UI

use std::sync::mpsc::{self, Receiver};
use std::thread;

use eframe::egui;

use crate::config::{AppProjectConfig, EdgeCutMode, LayerConfig};
use crate::core::excellon::ExcellonParser;
use crate::core::geometry::{Bounds, DrillData, DrillHole, Geometry, Point2, Toolpath};
use crate::core::gerber::GerberParser;
use crate::core::mirror::{mirror_drill_data, mirror_geometry, MirrorAxis};
use crate::core::toolpath::ToolpathGenerator;
use crate::gcode::generator::{GCodeGenerator, GCodeSettings};
use crate::ui::preview_2d::Preview2DState;
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

    // 表示コンポーネント
    preview_2d: Preview2DState,
    preview_3d: Preview3DState,
    simulator: SimulatorState,

    // ログとダイアログ
    logs: Vec<String>,
    show_help_dialog: bool,
    show_about_dialog: bool,
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
            preview_2d: Preview2DState::default(),
            preview_3d: Preview3DState::default(),
            simulator: SimulatorState::default(),
            logs: vec![format!("AltairCAM v{} を起動しました。", env!("APP_VERSION"))],
            show_help_dialog: false,
            show_about_dialog: false,
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
        } else {
            self.b_cu_toolpaths.clear();
        }

        if let Some(ref g) = self.edge_cuts_data {
            let gen = ToolpathGenerator::new(self.config.edge_cuts.tool_diameter);
            self.edge_toolpaths = gen.generate_edge_cuts_toolpaths(
                g,
                self.config.edge_cuts.edge_cut_mode,
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
        } else {
            self.drill_ordered_holes.clear();
        }
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
                Ok(gen.generate_drill_gcode(&self.drill_ordered_holes, &d.slots))
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
        let Some(folder) = rfd::FileDialog::new().pick_folder() else {
            return;
        };
        let targets = [
            (LayerTab::BCu, "b_cu.nc"),
            (LayerTab::EdgeCuts, "edge_cuts.nc"),
            (LayerTab::Drill, "drill.nc"),
        ];
        for (tab, filename) in targets {
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
                    if ui.button("使い方").clicked() {
                        self.show_help_dialog = true;
                        ui.close();
                    }
                    if ui.button("アップデートを確認").clicked() {
                        self.start_update_check(true, ctx.clone());
                        ui.close();
                    }
                    ui.separator();
                    if ui.button("バージョン情報").clicked() {
                        self.show_about_dialog = true;
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
                ui.separator();

                egui::ScrollArea::vertical()
                    .max_height(ui.available_height() - 140.0)
                    .show(ui, |ui| {
                        let tab = self.active_layer_tab;
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
                        }

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
                                        let label = match tab {
                                            LayerTab::BCu => "B_Cu",
                                            LayerTab::EdgeCuts => "Edge_Cuts",
                                            LayerTab::Drill => "Drill",
                                        };
                                        self.simulator.load_gcode_lines(label, &lines);
                                        self.active_view_tab = ViewTab::Simulation;
                                    }
                                    Err(e) => self.log(e),
                                }
                            }
                        });
                    });

                ui.separator();
                ui.label("ログ");
                egui::ScrollArea::vertical()
                    .stick_to_bottom(true)
                    .max_height(110.0)
                    .show(ui, |ui| {
                        for line in &self.logs {
                            ui.label(line);
                        }
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
                        .drill_ordered_holes
                        .iter()
                        .map(|h| h.position)
                        .collect();
                    self.preview_2d.ui(
                        ui,
                        b_cu,
                        edge,
                        drill,
                        &self.b_cu_toolpaths,
                        &self.edge_toolpaths,
                        &drill_pts,
                        bounds,
                    );
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
    fn layer_settings_ui(
        ui: &mut egui::Ui,
        cfg: &mut LayerConfig,
        tab: LayerTab,
        extensions: &[&str],
    ) -> bool {
        let mut changed = false;

        changed |= ui.checkbox(&mut cfg.enabled, "このレイヤーを有効にする").changed();

        ui.horizontal(|ui| {
            ui.label("ファイル:");
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
            ui.label("反転:");
            changed |= ui
                .radio_value(&mut cfg.mirror_axis, MirrorAxis::None, "なし")
                .changed();
            changed |= ui
                .radio_value(&mut cfg.mirror_axis, MirrorAxis::X, "X軸")
                .changed();
            changed |= ui
                .radio_value(&mut cfg.mirror_axis, MirrorAxis::Y, "Y軸")
                .changed();
        });

        ui.separator();

        egui::Grid::new(format!("grid_{:?}", tab))
            .num_columns(2)
            .spacing([12.0, 6.0])
            .show(ui, |ui| {
                ui.label("ツール直径 (mm):");
                changed |= ui
                    .add(egui::DragValue::new(&mut cfg.tool_diameter).speed(0.02).range(0.01..=10.0))
                    .changed();
                ui.end_row();

                ui.label("切削深さ (mm):");
                changed |= ui
                    .add(egui::DragValue::new(&mut cfg.cut_depth).speed(0.02).range(-10.0..=0.0))
                    .changed();
                ui.end_row();

                ui.label("1回切り込み深さ (mm):");
                changed |= ui
                    .add(egui::DragValue::new(&mut cfg.pass_depth).speed(0.02).range(0.0..=5.0))
                    .on_hover_text("0にすると1回で目標深さまで削ります。外形カット時は0.3〜0.5mm推奨。")
                    .changed();
                ui.end_row();

                ui.label("送り速度 (mm/min):");
                changed |= ui
                    .add(egui::DragValue::new(&mut cfg.feed_rate).speed(2.0).range(1.0..=3000.0))
                    .changed();
                ui.end_row();

                ui.label("スピンドル (rpm):");
                changed |= ui
                    .add(egui::DragValue::new(&mut cfg.spindle_speed).speed(100.0).range(0..=30000))
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
                        ui.label("アイソレーション周回数:");
                        changed |= ui
                            .add(egui::DragValue::new(&mut cfg.isolation_passes).range(1..=10))
                            .changed();
                        ui.end_row();

                        ui.label("パス重なり率:");
                        changed |= ui
                            .add(
                                egui::DragValue::new(&mut cfg.isolation_overlap)
                                    .speed(0.05)
                                    .range(0.0..=0.8),
                            )
                            .changed();
                        ui.end_row();
                    });
                changed |= ui
                    .checkbox(&mut cfg.optimize_toolpath, "パス順序を最適化する")
                    .changed();
            }
            LayerTab::EdgeCuts => {
                ui.horizontal(|ui| {
                    ui.label("カット位置:");
                    changed |= ui
                        .radio_value(&mut cfg.edge_cut_mode, EdgeCutMode::Outside, "外側")
                        .changed();
                    changed |= ui
                        .radio_value(&mut cfg.edge_cut_mode, EdgeCutMode::Inside, "内側")
                        .changed();
                    changed |= ui
                        .radio_value(&mut cfg.edge_cut_mode, EdgeCutMode::OnPath, "線上")
                        .changed();
                });
                changed |= ui
                    .checkbox(&mut cfg.optimize_toolpath, "パス順序を最適化する")
                    .changed();
            }
            LayerTab::Drill => {
                changed |= ui
                    .checkbox(&mut cfg.optimize_toolpath, "穴あけ順序を最適化する")
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
                        ui.label("Safe Z (mm):");
                        changed |= ui
                            .add(egui::DragValue::new(&mut cfg.safe_z).speed(0.5).range(0.5..=50.0))
                            .changed();
                        ui.end_row();

                        ui.label("Travel Z (mm):");
                        changed |= ui
                            .add(egui::DragValue::new(&mut cfg.travel_z).speed(0.5).range(0.2..=30.0))
                            .changed();
                        ui.end_row();

                        ui.label("早送り速度 (mm/min):");
                        changed |= ui
                            .add(
                                egui::DragValue::new(&mut cfg.rapid_feed_rate)
                                    .speed(10.0)
                                    .range(10.0..=5000.0),
                            )
                            .changed();
                        ui.end_row();

                        ui.label("Z軸下降速度 (mm/min):");
                        changed |= ui
                            .add(
                                egui::DragValue::new(&mut cfg.plunge_feed_rate)
                                    .speed(2.0)
                                    .range(5.0..=1000.0),
                            )
                            .changed();
                        ui.end_row();
                    });
            });

        changed
    }

    fn render_dialogs(&mut self, ctx: &egui::Context) {
        if self.show_help_dialog {
            let mut open = true;
            egui::Window::new("使い方")
                .open(&mut open)
                .collapsible(false)
                .resizable(false)
                .show(ctx, |ui| {
                    ui.label("1. 各レイヤータブで KiCad などから出力したガーバー / ドリルファイルを選択します。");
                    ui.label("2. 「すべて読み込み」を押すと2D/3Dプレビューとツールパスが表示されます。");
                    ui.label("3. 必要に応じて「原点(0,0)合わせ」やツール径、切り込み深さを調整します。");
                    ui.label("4. 「シミュレーションで確認」で動作を確認してから「Gコード出力」を実行します。");
                    ui.add_space(8.0);
                    ui.label("推奨加工順序: B_Cu (銅箔) -> Drill (穴あけ) -> Edge_Cuts (外形切り出し)");
                });
            if !open {
                self.show_help_dialog = false;
            }
        }

        if self.show_about_dialog {
            let mut open = true;
            egui::Window::new("バージョン情報")
                .open(&mut open)
                .collapsible(false)
                .resizable(false)
                .show(ctx, |ui| {
                    ui.heading("AltairCAM");
                    ui.label(format!("バージョン: {}", env!("APP_VERSION")));
                    ui.label("Rust製 PCB CNC用 CAMソフトウェア");
                    ui.add_space(6.0);
                    ui.checkbox(&mut self.config.auto_check_updates, "起動時に自動更新を確認する");
                    ui.add_space(6.0);
                    ui.hyperlink_to("GitHub リポジトリ", "https://github.com/Altairu/AltairCAM");
                });
            if !open {
                self.show_about_dialog = false;
            }
        }

        if let Some(info) = self.update_dialog.clone() {
            let mut open = true;
            egui::Window::new("アップデートが利用可能です")
                .open(&mut open)
                .collapsible(false)
                .resizable(true)
                .default_width(420.0)
                .show(ctx, |ui| {
                    ui.label(format!(
                        "現在: v{}  ->  最新: v{}",
                        info.current_version, info.latest_version
                    ));
                    ui.separator();
                    egui::ScrollArea::vertical().max_height(180.0).show(ui, |ui| {
                        ui.label(&info.release_notes);
                    });
                    ui.separator();
                    if let Some(ref status) = self.update_status_text {
                        ui.label(status);
                        ui.add_space(4.0);
                    }
                    ui.horizontal(|ui| {
                        if let Some(ref dl_url) = info.exe_download_url {
                            let btn_text = if self.updating_in_progress {
                                "更新中..."
                            } else {
                                "ダウンロードして更新"
                            };
                            if ui
                                .add_enabled(!self.updating_in_progress, egui::Button::new(btn_text))
                                .clicked()
                            {
                                self.start_update_install(dl_url.clone(), ctx.clone());
                            }
                        }
                        ui.hyperlink_to("リリースページを開く", &info.html_url);
                    });
                });
            if !open && !self.updating_in_progress {
                self.update_dialog = None;
            }
        }
    }
}
