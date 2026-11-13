#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod config;
mod core;
mod gcode;
mod ui;
mod updater;

use eframe::egui;
use std::path::Path;
use std::sync::Arc;

use crate::ui::app::AltairCamApp;

/// Windows環境で日本語フォントを読み込み、eguiに設定する
fn setup_japanese_fonts(ctx: &egui::Context) {
    let mut fonts = egui::FontDefinitions::default();

    let candidates = [
        r"C:\Windows\Fonts\YuGothM.ttc",
        r"C:\Windows\Fonts\YuGothR.ttc",
        r"C:\Windows\Fonts\meiryo.ttc",
        r"C:\Windows\Fonts\msgothic.ttc",
        r"C:\Windows\Fonts\BIZ-UDGothicR.ttc",
    ];

    for path in &candidates {
        if Path::new(path).exists() {
            if let Ok(data) = std::fs::read(path) {
                fonts.font_data.insert(
                    "jp_font".to_owned(),
                    Arc::new(egui::FontData::from_owned(data)),
                );
                if let Some(vec) = fonts.families.get_mut(&egui::FontFamily::Proportional) {
                    vec.push("jp_font".to_owned());
                }
                if let Some(vec) = fonts.families.get_mut(&egui::FontFamily::Monospace) {
                    vec.push("jp_font".to_owned());
                }
                break;
            }
        }
    }

    ctx.set_fonts(fonts);
}

/// 落ち着いたシンプルなライトテーマを設定する
fn setup_clean_style(ctx: &egui::Context) {
    let mut visuals = egui::Visuals::light();
    visuals.window_corner_radius = egui::CornerRadius::same(4);
    visuals.widgets.noninteractive.corner_radius = egui::CornerRadius::same(3);
    visuals.widgets.inactive.corner_radius = egui::CornerRadius::same(3);
    visuals.widgets.hovered.corner_radius = egui::CornerRadius::same(3);
    visuals.widgets.active.corner_radius = egui::CornerRadius::same(3);
    ctx.set_visuals(visuals);
}

/// 埋め込みアイコンを読み込む
fn load_app_icon() -> Option<egui::IconData> {
    let bytes = include_bytes!("../AltairCAM.ico");
    let img = image::load_from_memory(bytes).ok()?.to_rgba8();
    let (width, height) = img.dimensions();
    Some(egui::IconData {
        rgba: img.into_raw(),
        width,
        height,
    })
}

fn main() -> eframe::Result<()> {
    // 前回の自動更新で残った古いバイナリを削除
    updater::cleanup_old_executable();

    let mut viewport = egui::ViewportBuilder::default()
        .with_inner_size([1360.0, 820.0])
        .with_min_inner_size([960.0, 600.0])
        .with_title("AltairCAM");

    if let Some(icon) = load_app_icon() {
        viewport = viewport.with_icon(Arc::new(icon));
    }

    let options = eframe::NativeOptions {
        viewport,
        ..Default::default()
    };

    eframe::run_native(
        "AltairCAM",
        options,
        Box::new(|cc| {
            setup_japanese_fonts(&cc.egui_ctx);
            setup_clean_style(&cc.egui_ctx);
            Ok(Box::new(AltairCamApp::new(cc)))
        }),
    )
}
