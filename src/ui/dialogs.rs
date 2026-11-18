//! ヘルプ、ライセンス・権利情報、問題報告、バージョン情報ダイアログ

use eframe::egui;

use crate::config::AppProjectConfig;
use crate::updater::UpdateInfo;

/// GitHub リポジトリおよび Issues の URL
pub const GITHUB_REPO_URL: &str = "https://github.com/Altairu/AltairCAM";
pub const GITHUB_ISSUES_URL: &str = "https://github.com/Altairu/AltairCAM/issues";
const GITHUB_NEW_ISSUE_URL: &str = "https://github.com/Altairu/AltairCAM/issues/new";

/// ビルド時に埋め込むライセンスと帰属表示ドキュメント
const LICENSE_TEXT: &str = include_str!("../../LICENSE");
const ATTRIBUTION_TEXT: &str = include_str!("../../ATTRIBUTION.md");

/// ヘルプダイアログのタブ
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HelpTab {
    BasicUsage,
    Parameters,
    FileFormat,
}

/// ライセンス・権利情報ダイアログのタブ
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LicenseTab {
    AltairCam,
    Attribution,
    ThirdParty,
}

/// 問題報告の種類
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IssueKind {
    Bug,
    Feature,
    Question,
}

impl IssueKind {
    fn label(self) -> &'static str {
        match self {
            Self::Bug => "バグ報告",
            Self::Feature => "機能要望",
            Self::Question => "質問・その他",
        }
    }

    fn prefix(self) -> &'static str {
        match self {
            Self::Bug => "[Bug] ",
            Self::Feature => "[Feature] ",
            Self::Question => "[Question] ",
        }
    }
}

/// 問題報告ダイアログの状態
pub struct IssueReportState {
    pub open: bool,
    pub kind: IssueKind,
    pub title: String,
    pub description: String,
    pub include_env_and_logs: bool,
    pub status_message: Option<String>,
}

impl Default for IssueReportState {
    fn default() -> Self {
        Self {
            open: false,
            kind: IssueKind::Bug,
            title: String::new(),
            description: String::new(),
            include_env_and_logs: true,
            status_message: None,
        }
    }
}

/// ダイアログ群の表示状態
pub struct DialogsState {
    pub show_help: bool,
    pub help_tab: HelpTab,
    pub show_license: bool,
    pub license_tab: LicenseTab,
    pub show_about: bool,
    pub issue_report: IssueReportState,
}

impl Default for DialogsState {
    fn default() -> Self {
        Self {
            show_help: false,
            help_tab: HelpTab::BasicUsage,
            show_license: false,
            license_tab: LicenseTab::AltairCam,
            show_about: false,
            issue_report: IssueReportState::default(),
        }
    }
}

/// RFC 3986 に基づく URL クエリ用パーセントエンコード
fn url_encode(input: &str) -> String {
    let mut out = String::with_capacity(input.len() * 2);
    for byte in input.as_bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(*byte as char);
            }
            b' ' => out.push_str("%20"),
            _ => {
                out.push('%');
                out.push_str(&format!("{:02X}", byte));
            }
        }
    }
    out
}

/// 問題報告用の Markdown 本文を組み立てる
fn build_issue_body(
    state: &IssueReportState,
    config: &AppProjectConfig,
    logs: &[String],
    max_log_lines: usize,
) -> String {
    let mut body = String::new();

    body.push_str(&format!("### 種類\n{}\n\n", state.kind.label()));
    body.push_str("### 詳細・再現手順\n");
    if state.description.trim().is_empty() {
        body.push_str("（ここに問題の内容や再現手順を記入してください）\n\n");
    } else {
        body.push_str(state.description.trim());
        body.push_str("\n\n");
    }

    if state.include_env_and_logs {
        body.push_str("### 環境情報\n");
        body.push_str(&format!("- AltairCAM: v{}\n", env!("APP_VERSION")));
        body.push_str(&format!(
            "- OS / Arch: {} / {}\n",
            std::env::consts::OS,
            std::env::consts::ARCH
        ));
        body.push_str(&format!(
            "- 原点自動合わせ: {}\n",
            config.zero_origin_on_load
        ));
        body.push_str(&format!(
            "- B_Cu: enabled={}, tool={}mm, depth={}mm, passes={}, overlap={}\n",
            config.b_cu.enabled,
            config.b_cu.tool_diameter,
            config.b_cu.cut_depth,
            config.b_cu.isolation_passes,
            config.b_cu.isolation_overlap
        ));
        body.push_str(&format!(
            "- Edge_Cuts: enabled={}, tool={}mm, depth={}mm, pass_depth={}mm, mode={:?}\n",
            config.edge_cuts.enabled,
            config.edge_cuts.tool_diameter,
            config.edge_cuts.cut_depth,
            config.edge_cuts.pass_depth,
            config.edge_cuts.edge_cut_mode
        ));
        body.push_str(&format!(
            "- Drill: enabled={}, tool={}mm, depth={}mm, optimize={}\n\n",
            config.drill.enabled,
            config.drill.tool_diameter,
            config.drill.cut_depth,
            config.drill.optimize_toolpath
        ));

        if !logs.is_empty() && max_log_lines > 0 {
            body.push_str("### 直近のログ\n```\n");
            let start = logs.len().saturating_sub(max_log_lines);
            for line in &logs[start..] {
                body.push_str(line);
                body.push('\n');
            }
            body.push_str("```\n");
        }
    }

    body
}

/// ヘルプ・ライセンス・問題報告・バージョン情報・アップデートダイアログを描画する
pub fn render_all_dialogs(
    ctx: &egui::Context,
    dialogs: &mut DialogsState,
    config: &mut AppProjectConfig,
    logs: &[String],
    update_dialog: &mut Option<UpdateInfo>,
    update_status_text: &Option<String>,
    updating_in_progress: bool,
    on_start_update_install: &mut dyn FnMut(String),
) {
    render_help_dialog(ctx, dialogs);
    render_license_dialog(ctx, dialogs);
    render_issue_report_dialog(ctx, dialogs, config, logs);
    render_about_dialog(ctx, dialogs, config);
    render_update_dialog(
        ctx,
        update_dialog,
        update_status_text,
        updating_in_progress,
        on_start_update_install,
    );
}

/// ヘルプダイアログ（使い方・パラメーター解説）
fn render_help_dialog(ctx: &egui::Context, dialogs: &mut DialogsState) {
    if !dialogs.show_help {
        return;
    }

    let mut open = true;
    egui::Window::new("ヘルプ / パラメーター解説")
        .open(&mut open)
        .collapsible(false)
        .resizable(true)
        .default_size([640.0, 520.0])
        .show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.selectable_value(&mut dialogs.help_tab, HelpTab::BasicUsage, "基本操作");
                ui.selectable_value(
                    &mut dialogs.help_tab,
                    HelpTab::Parameters,
                    "パラメーター解説",
                );
                ui.selectable_value(
                    &mut dialogs.help_tab,
                    HelpTab::FileFormat,
                    "対応ファイル / KiCad出力",
                );
            });
            ui.separator();

            egui::ScrollArea::vertical()
                .auto_shrink([false, false])
                .show(ui, |ui| match dialogs.help_tab {
                    HelpTab::BasicUsage => render_help_basic_tab(ui),
                    HelpTab::Parameters => render_help_parameters_tab(ui),
                    HelpTab::FileFormat => render_help_file_tab(ui),
                });
        });

    if !open {
        dialogs.show_help = false;
    }
}

fn render_help_basic_tab(ui: &mut egui::Ui) {
    ui.heading("加工までの流れ");
    ui.add_space(4.0);
    ui.label("• 各レイヤータブ (B_Cu / Edge_Cuts / Drill) でガーバーまたはドリルファイルを選択します。");
    ui.label("• 「すべて読み込み」を押すと、2D / 3Dプレビューとツールパスが生成されます。");
    ui.label("• 必要に応じて「原点(0,0)合わせ」やツール直径、切削深さなどのパラメーターを調整します。");
    ui.label("• 「シミュレーションで確認」でツール移動と切削動作を確認します。");
    ui.label("• 「Gコード出力...」またはメニューの「Gコードをフォルダへ一括出力...」でNCファイルを保存します。");

    ui.add_space(10.0);
    ui.heading("推奨される加工順序");
    ui.add_space(4.0);
    ui.label("B_Cu (銅箔パターン切削) -> Drill (穴あけ) -> Edge_Cuts (基板外形切り出し)");
    ui.label("先に外形を切り抜くと基板が固定されなくなるため、外形カットは最後に行うことを推奨します。");

    ui.add_space(10.0);
    ui.heading("プレビュー操作");
    ui.add_space(4.0);
    ui.label("• 2D プレビュー: 左ドラッグまたは中ドラッグで移動、マウスホイールで拡大縮小、「全体表示」で自動フィット。");
    ui.label("• 3D ビュー: 左ドラッグで視点回転、右ドラッグまたは中ドラッグで移動、マウスホイールで拡大縮小、「視点リセット」で初期視点に戻ります。");
    ui.label("• 読込時に自動で原点を(0,0)に合わせる: 読み込んだ全レイヤーの左下が座標 (0, 0) になるよう平行移動します。");
}

fn render_help_parameters_tab(ui: &mut egui::Ui) {
    ui.label("各パラメーターの意味と設定の目安です。設定画面で項目にマウスカーソルを合わせても説明が表示されます。");
    ui.add_space(8.0);

    let sections: &[(&str, &[(&str, &str)])] = &[
        (
            "基本設定 (全レイヤー共通)",
            &[
                (
                    "切削条件プリセット",
                    "工具直径や深さ・送り速度などの条件を名前付きで保存・適用できます。刃物の交換時にワンクリックで設定を切り替えられます。",
                ),
                (
                    "このレイヤーを有効にする",
                    "オフにするとプレビュー表示や一括Gコード出力の対象から外れます。",
                ),
                (
                    "反転 (なし / X軸 / Y軸)",
                    "基板の基準点を中心に反転します。裏面(B_Cu)を加工する際や、図面と加工面の向きを合わせる場合に使用します。",
                ),
                (
                    "ツール直径 (mm)",
                    "使用するエンドミルやドリルビットの刃径です。B_CuはVカッター先端径や0.1〜0.2mm、Edge_Cutsは0.8〜2.0mm、Drillは穴径に合わせます。",
                ),
                (
                    "切削深さ (mm)",
                    "最終的なZ軸の切り込み深さです（0以下のマイナス値で指定）。目安: B_Cuは -0.05〜-0.1mm、Edge_Cuts/Drillは基板厚+余裕 (-1.6〜-1.8mm)。",
                ),
                (
                    "1回切り込み深さ (mm)",
                    "多段切り込みの1パスあたりの深さ（正の値）。0にすると目標深さまで1回で削ります。基板外形カット時はツール折損防止のため 0.3〜0.5mm を推奨します。",
                ),
                (
                    "送り速度 (mm/min)",
                    "XY平面で切削しながら移動する速度です。目安: 50〜150 mm/min。速すぎると工具が折れる原因になります。",
                ),
                (
                    "スピンドル (rpm)",
                    "主軸の回転数 (Sコマンド) です。使用するCNCマシンと刃物に合わせて設定します。",
                ),
            ],
        ),
        (
            "B_Cu (銅箔) 固有設定 / DRC",
            &[
                (
                    "アイソレーション周回数",
                    "銅箔パターンの周囲を何周削って絶縁溝を広げるかを指定します（1〜10周）。周回数を増やすとパターン間のショートを防ぎやすくなります。",
                ),
                (
                    "パス重なり率",
                    "複数周回削る際の隣接パス同士の重なり割合 (0.0〜0.8) です。0.2〜0.3程度にすると削り残しを防げます。",
                ),
                (
                    "最小許容パターン幅 (mm) / DRC",
                    "最小パターン幅・最小クリアランスを計測し、指定幅未満の細線や現在のツール径で削りきれない狭い隙間（加工不能箇所）を2Dプレビュー上に赤色で警告表示します。",
                ),
                (
                    "パス順序を最適化する",
                    "切削パス間の空走移動距離が短くなるように順序を並べ替え、加工時間を短縮します。",
                ),
            ],
        ),
        (
            "Edge_Cuts (外形) 固有設定",
            &[
                (
                    "カット位置 (外側 / 内側 / 線上)",
                    "ツール中心を通す位置を選びます。「外側」はツール半径分だけ外を削るため、完成基板の寸法が設計通りになります。「内側」は窓抜き用、「線上」は中心線上をそのまま削ります。",
                ),
                (
                    "外周マージン (mm)",
                    "外形カットのオフセット距離にさらに追加する余白です。基板サイズを少し大きめに切り出したい場合に使用します。",
                ),
                (
                    "保持タブ (Gaps) / タブ幅 (mm)",
                    "基板を切り抜く際に、加工中に基板が外れて動かないよう上下・左右・4方に切り残し（ブリッジ）を作ります。ツール直径分も考慮してパスを自動分割します。",
                ),
                (
                    "パス順序を最適化する",
                    "複数の外形パスがある場合に移動距離が短くなる順で加工します。",
                ),
            ],
        ),
        (
            "Drill (ドリル) 固有設定",
            &[
                (
                    "大きい穴をエンドミルでくり抜く (Mill Holes)",
                    "ツール直径より大きい穴を、同心円状のポケット加工パスに変換してエンドミルでくり抜きます。ツール直径以下の穴は通常通り垂直ドリルで加工します。",
                ),
                (
                    "穴あけ順序を最適化する",
                    "最近傍探索などにより穴あけの移動順序を最適化し、移動距離と加工時間を大幅に削減します。",
                ),
            ],
        ),
        (
            "プレビュー / シミュレーション機能",
            &[
                (
                    "切削後イメージ (2D プレビュー)",
                    "ツール直径の実寸幅で切削溝をマスク表示し、加工後に残る銅箔や外形の保持タブ状態を視覚的に確認できます。",
                ),
                (
                    "加工時間見積もり / 実寸ツール幅",
                    "各レイヤーの切削距離・早送り距離・推定加工時間および全レイヤー合計の推定時間を常時表示します。シミュレーターでは実際の刃径幅で軌跡を再生できます。",
                ),
            ],
        ),
        (
            "詳細設定 (Z高さ / 移動速度)",
            &[
                (
                    "Safe Z (mm)",
                    "加工開始時・終了時やツール退避時に移動する安全なZ高さです。クランプやネジに干渉しない高さ（例: 5.0mm）を設定します。",
                ),
                (
                    "Travel Z (mm)",
                    "切削箇所から次の切削箇所へXY移動する際のZ退避高さです。基板表面から少し浮かせた高さ（例: 2.0mm）を設定します。",
                ),
                (
                    "早送り速度 (mm/min)",
                    "空中移動時（G0相当または非切削移動）の移動速度です。",
                ),
                (
                    "Z軸下降速度 (mm/min)",
                    "Z軸を材料へ切り込む際（プランジ）の下降速度です。刃先への負荷を抑えるため送り速度より遅め（例: 30〜50 mm/min）にします。",
                ),
            ],
        ),
    ];

    for (section_title, items) in sections {
        ui.heading(*section_title);
        ui.add_space(4.0);
        egui::Grid::new(format!("help_param_grid_{}", section_title))
            .num_columns(2)
            .spacing([12.0, 6.0])
            .striped(true)
            .show(ui, |ui| {
                for (name, desc) in *items {
                    ui.strong(*name);
                    ui.add(egui::Label::new(*desc).wrap());
                    ui.end_row();
                }
            });
        ui.add_space(10.0);
    }
}

fn render_help_file_tab(ui: &mut egui::Ui) {
    ui.heading("対応ファイル形式");
    ui.add_space(4.0);
    ui.label("• ガーバーファイル (RS-274X): .gbr, .gtl, .gbl, .gm1, .gko");
    ui.label("• ドリルファイル (Excellon): .drl, .txt, .xln（長穴スロット対応）");

    ui.add_space(10.0);
    ui.heading("KiCad からの出力手順");
    ui.add_space(4.0);
    ui.label("1. KiCad PCBエディタで「ファイル」->「プロット」を開きます。");
    ui.label("2. 出力フォーマットを「Gerber」にし、含めるレイヤーで「B.Cu」と「Edge.Cuts」にチェックを入れてプロットします。");
    ui.label("3. 同じダイアログ内の「ドリルファイルの生成」を開き、フォーマット「Excellon」、単位「ミリメートル」で生成します。");
    ui.label("4. 出力された各ファイルを AltairCAM の対応するタブから読み込みます。");
}

/// ライセンス・権利情報ダイアログ
fn render_license_dialog(ctx: &egui::Context, dialogs: &mut DialogsState) {
    if !dialogs.show_license {
        return;
    }

    let mut open = true;
    egui::Window::new("ライセンス・権利情報")
        .open(&mut open)
        .collapsible(false)
        .resizable(true)
        .default_size([600.0, 460.0])
        .show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.selectable_value(
                    &mut dialogs.license_tab,
                    LicenseTab::AltairCam,
                    "AltairCAM (MIT)",
                );
                ui.selectable_value(
                    &mut dialogs.license_tab,
                    LicenseTab::Attribution,
                    "帰属表示 (FlatCAM)",
                );
                ui.selectable_value(
                    &mut dialogs.license_tab,
                    LicenseTab::ThirdParty,
                    "使用ライブラリ (OSS)",
                );
            });
            ui.separator();

            egui::ScrollArea::vertical()
                .auto_shrink([false, false])
                .show(ui, |ui| match dialogs.license_tab {
                    LicenseTab::AltairCam => {
                        ui.label("AltairCAM は MIT License のもとで公開されています。");
                        ui.add_space(6.0);
                        ui.add(
                            egui::TextEdit::multiline(&mut LICENSE_TEXT.to_string())
                                .font(egui::TextStyle::Monospace)
                                .desired_width(f32::INFINITY)
                                .interactive(false),
                        );
                    }
                    LicenseTab::Attribution => {
                        ui.label("アルゴリズムの参考元および帰属表示です。");
                        ui.add_space(6.0);
                        ui.add(
                            egui::TextEdit::multiline(&mut ATTRIBUTION_TEXT.to_string())
                                .font(egui::TextStyle::Monospace)
                                .desired_width(f32::INFINITY)
                                .interactive(false),
                        );
                    }
                    LicenseTab::ThirdParty => {
                        ui.label("AltairCAM は以下のオープンソースライブラリを使用しています。");
                        ui.add_space(6.0);
                        let libs: &[(&str, &str, &str)] = &[
                            ("eframe / egui", "GUIフレームワーク", "MIT / Apache-2.0"),
                            ("i_overlay", "2Dポリゴン・ブール演算 / オフセット", "MIT"),
                            ("rfd", "ネイティブファイルダイアログ", "MIT / Apache-2.0"),
                            ("ureq", "HTTPクライアント (自動更新)", "MIT / Apache-2.0"),
                            ("serde / serde_json", "設定ファイルのシリアライズ", "MIT / Apache-2.0"),
                            ("semver", "バージョン比較", "MIT / Apache-2.0"),
                            ("image", "アイコン画像デコード", "MIT / Apache-2.0"),
                            ("earcutr", "ポリゴン三角形分割 (3D表示)", "ISC"),
                        ];
                        egui::Grid::new("third_party_libs_grid")
                            .num_columns(3)
                            .spacing([14.0, 6.0])
                            .striped(true)
                            .show(ui, |ui| {
                                ui.strong("クレート名");
                                ui.strong("用途");
                                ui.strong("ライセンス");
                                ui.end_row();
                                for (name, role, lic) in libs {
                                    ui.label(*name);
                                    ui.label(*role);
                                    ui.label(*lic);
                                    ui.end_row();
                                }
                            });
                    }
                });
        });

    if !open {
        dialogs.show_license = false;
    }
}

/// 問題報告ダイアログ (GitHub Issues へ送信)
fn render_issue_report_dialog(
    ctx: &egui::Context,
    dialogs: &mut DialogsState,
    config: &AppProjectConfig,
    logs: &[String],
) {
    if !dialogs.issue_report.open {
        return;
    }

    let mut open = true;
    egui::Window::new("問題を報告 (GitHub Issues)")
        .open(&mut open)
        .collapsible(false)
        .resizable(true)
        .default_size([520.0, 440.0])
        .show(ctx, |ui| {
            let state = &mut dialogs.issue_report;
            ui.label("問題や要望を GitHub Issues に送信できます。");
            ui.label("「GitHub Issues で開く」を押すと、入力内容が入った状態でブラウザが開きます。");
            ui.add_space(6.0);

            ui.horizontal(|ui| {
                ui.label("種類:");
                ui.radio_value(&mut state.kind, IssueKind::Bug, IssueKind::Bug.label());
                ui.radio_value(
                    &mut state.kind,
                    IssueKind::Feature,
                    IssueKind::Feature.label(),
                );
                ui.radio_value(
                    &mut state.kind,
                    IssueKind::Question,
                    IssueKind::Question.label(),
                );
            });

            ui.add_space(4.0);
            ui.label("タイトル:");
            ui.add(
                egui::TextEdit::singleline(&mut state.title)
                    .desired_width(f32::INFINITY)
                    .hint_text("例: ガーバー読み込み時に外形のパスが欠ける"),
            );

            ui.add_space(4.0);
            ui.label("詳細 / 再現手順:");
            ui.add(
                egui::TextEdit::multiline(&mut state.description)
                    .desired_rows(7)
                    .desired_width(f32::INFINITY)
                    .hint_text("発生した現象、再現手順、期待する動作などを記入してください。"),
            );

            ui.add_space(4.0);
            ui.checkbox(
                &mut state.include_env_and_logs,
                "バージョン情報・レイヤー設定概要・直近のログを自動添付する",
            );

            ui.add_space(8.0);
            ui.horizontal(|ui| {
                if ui.button("GitHub Issues で開く").clicked() {
                    let full_title = if state.title.trim().is_empty() {
                        format!("{}問題報告", state.kind.prefix())
                    } else {
                        format!("{}{}", state.kind.prefix(), state.title.trim())
                    };
                    // URL長制限を避けるため、まず30行で生成し、長すぎる場合はログ行数を絞る
                    let mut body = build_issue_body(state, config, logs, 30);
                    let mut url = format!(
                        "{}?title={}&body={}",
                        GITHUB_NEW_ISSUE_URL,
                        url_encode(&full_title),
                        url_encode(&body)
                    );
                    if url.len() > 7000 {
                        body = build_issue_body(state, config, logs, 10);
                        url = format!(
                            "{}?title={}&body={}",
                            GITHUB_NEW_ISSUE_URL,
                            url_encode(&full_title),
                            url_encode(&body)
                        );
                    }
                    if url.len() > 7500 {
                        body = build_issue_body(state, config, logs, 0);
                        url = format!(
                            "{}?title={}&body={}",
                            GITHUB_NEW_ISSUE_URL,
                            url_encode(&full_title),
                            url_encode(&body)
                        );
                    }
                    ctx.open_url(egui::OpenUrl::new_tab(url));
                    state.status_message = Some(
                        "ブラウザで GitHub Issues の作成ページを開きました。内容を確認して Submit してください。"
                            .to_string(),
                    );
                }

                if ui.button("報告内容をコピー").clicked() {
                    let full_title = format!("{}{}", state.kind.prefix(), state.title.trim());
                    let body = build_issue_body(state, config, logs, 50);
                    let text = format!("# {}\n\n{}", full_title, body);
                    ctx.copy_text(text);
                    state.status_message =
                        Some("報告内容（ログ含む）をクリップボードにコピーしました。".to_string());
                }

                ui.hyperlink_to("Issues 一覧を見る", GITHUB_ISSUES_URL);
            });

            if let Some(ref msg) = state.status_message {
                ui.add_space(4.0);
                ui.label(msg);
            }
        });

    if !open {
        dialogs.issue_report.open = false;
    }
}

/// バージョン情報ダイアログ
fn render_about_dialog(
    ctx: &egui::Context,
    dialogs: &mut DialogsState,
    config: &mut AppProjectConfig,
) {
    if !dialogs.show_about {
        return;
    }

    let mut open = true;
    egui::Window::new("バージョン情報")
        .open(&mut open)
        .collapsible(false)
        .resizable(false)
        .show(ctx, |ui| {
            ui.heading("AltairCAM");
            ui.label(format!("バージョン: {}", env!("APP_VERSION")));
            ui.label("Rust製 PCB CNC用 CAMソフトウェア");
            ui.add_space(4.0);
            ui.label("Copyright (c) 2026 Altairu");
            ui.label("ライセンス: MIT License");
            ui.add_space(6.0);
            ui.checkbox(
                &mut config.auto_check_updates,
                "起動時に自動更新を確認する",
            );
            ui.add_space(6.0);
            ui.horizontal(|ui| {
                if ui.button("ライセンス・権利情報...").clicked() {
                    dialogs.show_license = true;
                }
                if ui.button("問題を報告...").clicked() {
                    dialogs.issue_report.open = true;
                }
            });
            ui.add_space(4.0);
            ui.horizontal(|ui| {
                ui.hyperlink_to("GitHub リポジトリ", GITHUB_REPO_URL);
                ui.hyperlink_to("Issues", GITHUB_ISSUES_URL);
            });
        });

    if !open {
        dialogs.show_about = false;
    }
}

/// アップデート通知ダイアログ
fn render_update_dialog(
    ctx: &egui::Context,
    update_dialog: &mut Option<UpdateInfo>,
    update_status_text: &Option<String>,
    updating_in_progress: bool,
    on_start_update_install: &mut dyn FnMut(String),
) {
    if let Some(info) = update_dialog.clone() {
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
                egui::ScrollArea::vertical()
                    .max_height(180.0)
                    .show(ui, |ui| {
                        ui.label(&info.release_notes);
                    });
                ui.separator();
                if let Some(ref status) = update_status_text {
                    ui.label(status);
                    ui.add_space(4.0);
                }
                ui.horizontal(|ui| {
                    if let Some(ref dl_url) = info.exe_download_url {
                        let btn_text = if updating_in_progress {
                            "更新中..."
                        } else {
                            "ダウンロードして更新"
                        };
                        if ui
                            .add_enabled(!updating_in_progress, egui::Button::new(btn_text))
                            .clicked()
                        {
                            on_start_update_install(dl_url.clone());
                        }
                    }
                    ui.hyperlink_to("リリースページを開く", &info.html_url);
                });
            });
        if !open && !updating_in_progress {
            *update_dialog = None;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_url_encode_basic() {
        assert_eq!(url_encode("abc-123_XYZ.~"), "abc-123_XYZ.~");
        assert_eq!(url_encode("a b\nc"), "a%20b%0Ac");
        assert_eq!(url_encode("問題報告"), "%E5%95%8F%E9%A1%8C%E5%A0%B1%E5%91%8A");
    }

    #[test]
    fn test_build_issue_body_includes_env_and_logs() {
        let state = IssueReportState {
            open: true,
            kind: IssueKind::Bug,
            title: "テスト".to_string(),
            description: "再現手順テスト".to_string(),
            include_env_and_logs: true,
            status_message: None,
        };
        let cfg = AppProjectConfig::default();
        let logs = vec!["log1".to_string(), "log2".to_string()];
        let body = build_issue_body(&state, &cfg, &logs, 10);
        assert!(body.contains("再現手順テスト"));
        assert!(body.contains("AltairCAM: v"));
        assert!(body.contains("log1"));
        assert!(body.contains("log2"));
    }
}
