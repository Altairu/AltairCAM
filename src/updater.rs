//! GitHub Releases を使った自動更新機能

use semver::Version;
use serde::Deserialize;
use std::env;
use std::fs;
use std::io::Read;
use std::path::PathBuf;
use std::process::Command;

const GITHUB_API_LATEST: &str = "https://api.github.com/repos/Altairu/AltairCAM/releases/latest";

#[derive(Debug, Deserialize)]
struct GitHubRelease {
    tag_name: String,
    html_url: String,
    #[serde(default)]
    body: String,
    #[serde(default)]
    assets: Vec<GitHubAsset>,
}

#[derive(Debug, Deserialize)]
struct GitHubAsset {
    name: String,
    browser_download_url: String,
    size: u64,
}

#[derive(Debug, Clone)]
pub struct UpdateInfo {
    pub current_version: String,
    pub latest_version: String,
    pub release_notes: String,
    pub html_url: String,
    pub exe_download_url: Option<String>,
    #[allow(dead_code)]
    pub asset_size: u64,
}

/// 起動時に前回のアップデートで残った .old ファイルを削除する
pub fn cleanup_old_executable() {
    if let Ok(exe_path) = env::current_exe() {
        let mut old_path = exe_path.clone();
        if let Some(name) = exe_path.file_name().and_then(|n| n.to_str()) {
            old_path.set_file_name(format!("{}.old", name));
            if old_path.exists() {
                let _ = fs::remove_file(&old_path);
            }
        }
    }
}

/// 最新バージョンを確認する
pub fn check_for_updates() -> Result<Option<UpdateInfo>, String> {
    let current_str = env!("APP_VERSION");
    let current_ver = Version::parse(current_str)
        .map_err(|e| format!("現在のバージョン解析エラー: {}", e))?;

    let mut response = ureq::get(GITHUB_API_LATEST)
        .header("User-Agent", &format!("AltairCAM/{}", current_str))
        .header("Accept", "application/vnd.github+json")
        .call()
        .map_err(|e| format!("GitHubへの接続に失敗しました: {}", e))?;

    let release: GitHubRelease = response
        .body_mut()
        .read_json()
        .map_err(|e| format!("リリース情報の解析に失敗しました: {}", e))?;

    let tag_clean = release.tag_name.trim_start_matches(['v', 'V']);
    let latest_ver = Version::parse(tag_clean)
        .map_err(|e| format!("最新バージョン番号({})の解析に失敗しました: {}", release.tag_name, e))?;

    if latest_ver <= current_ver {
        return Ok(None);
    }

    // Windows用の .exe アセットを探す
    let exe_asset = release
        .assets
        .iter()
        .find(|a| a.name.to_lowercase().ends_with(".exe"));

    Ok(Some(UpdateInfo {
        current_version: current_str.to_string(),
        latest_version: latest_ver.to_string(),
        release_notes: release.body,
        html_url: release.html_url,
        exe_download_url: exe_asset.map(|a| a.browser_download_url.clone()),
        asset_size: exe_asset.map(|a| a.size).unwrap_or(0),
    }))
}

/// 更新をダウンロードして実行ファイルを差し替え、再起動する
pub fn download_and_install_update(download_url: &str) -> Result<(), String> {
    let current_exe = env::current_exe()
        .map_err(|e| format!("実行ファイルパスの取得に失敗しました: {}", e))?;
    let exe_dir = current_exe
        .parent()
        .ok_or_else(|| "実行ディレクトリの取得に失敗しました".to_string())?;
    let exe_name = current_exe
        .file_name()
        .and_then(|n| n.to_str())
        .ok_or_else(|| "実行ファイル名の取得に失敗しました".to_string())?;

    let temp_new_path: PathBuf = exe_dir.join(format!("{}.new", exe_name));
    let backup_old_path: PathBuf = exe_dir.join(format!("{}.old", exe_name));

    // 新しいバイナリをダウンロード
    let mut response = ureq::get(download_url)
        .header("User-Agent", &format!("AltairCAM/{}", env!("APP_VERSION")))
        .call()
        .map_err(|e| format!("更新ファイルのダウンロードに失敗しました: {}", e))?;

    let mut bytes = Vec::new();
    response
        .body_mut()
        .as_reader()
        .take(200 * 1024 * 1024) // 上限200MB
        .read_to_end(&mut bytes)
        .map_err(|e| format!("ダウンロードデータの読み取りに失敗しました: {}", e))?;

    if bytes.is_empty() {
        return Err("ダウンロードされたファイルが空です".to_string());
    }

    fs::write(&temp_new_path, &bytes)
        .map_err(|e| format!("一時ファイルの書き込みに失敗しました: {}", e))?;

    if backup_old_path.exists() {
        let _ = fs::remove_file(&backup_old_path);
    }

    // 実行中のexeをリネームし、新しいexeを配置
    if let Err(e) = fs::rename(&current_exe, &backup_old_path) {
        let _ = fs::remove_file(&temp_new_path);
        return Err(format!("既存ファイルの退避に失敗しました（権限を確認してください）: {}", e));
    }

    if let Err(e) = fs::rename(&temp_new_path, &current_exe) {
        // 失敗時は元に戻す
        let _ = fs::rename(&backup_old_path, &current_exe);
        let _ = fs::remove_file(&temp_new_path);
        return Err(format!("新しい実行ファイルの配置に失敗しました: {}", e));
    }

    // 新しいプロセスを起動して現在プロセスを終了
    Command::new(&current_exe)
        .spawn()
        .map_err(|e| format!("再起動に失敗しました: {}", e))?;

    std::process::exit(0);
}
