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
    body: Option<String>,
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
        release_notes: release.body.unwrap_or_default(),
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

    let temp_new_path: PathBuf = env::temp_dir().join(format!("altaircam_update_{}.exe", std::process::id()));
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
    if cfg!(target_os = "windows") && !bytes.starts_with(b"MZ") {
        return Err("ダウンロードされたファイルが有効なWindows実行ファイルではありません".to_string());
    }

    fs::write(&temp_new_path, &bytes)
        .map_err(|e| format!("一時ファイルの書き込みに失敗しました: {}", e))?;

    if backup_old_path.exists() {
        let _ = fs::remove_file(&backup_old_path);
    }

    // 実行中のexeをリネームし、新しいexeを配置
    match fs::rename(&current_exe, &backup_old_path) {
        Ok(()) => {
            // temp_dir と exe_dir が別ドライブの場合に備えて copy + remove を使用
            if let Err(e) = fs::copy(&temp_new_path, &current_exe) {
                let _ = fs::rename(&backup_old_path, &current_exe);
                let _ = fs::remove_file(&temp_new_path);
                return Err(format!("新しい実行ファイルの配置に失敗しました: {}", e));
            }
            let _ = fs::remove_file(&temp_new_path);

            // 新しいプロセスを起動して現在プロセスを終了
            Command::new(&current_exe)
                .spawn()
                .map_err(|e| format!("再起動に失敗しました: {}", e))?;

            std::process::exit(0);
        }
        Err(e) if e.kind() == std::io::ErrorKind::PermissionDenied && cfg!(target_os = "windows") => {
            // Program Files 等で権限が必要な場合は管理者権限昇格で差し替える
            install_elevated_windows(&temp_new_path, &current_exe)?;
            std::process::exit(0);
        }
        Err(e) => {
            let _ = fs::remove_file(&temp_new_path);
            return Err(format!("既存ファイルの退避に失敗しました（権限を確認してください）: {}", e));
        }
    }
}

#[cfg(target_os = "windows")]
fn install_elevated_windows(temp_new_path: &std::path::Path, current_exe: &std::path::Path) -> Result<(), String> {
    let pid = std::process::id();
    let src = temp_new_path.to_string_lossy().replace('\'', "''");
    let dst = current_exe.to_string_lossy().replace('\'', "''");
    let ps_script = format!(
        "Wait-Process -Id {pid} -ErrorAction SilentlyContinue; \
         Start-Sleep -Milliseconds 300; \
         Copy-Item -LiteralPath '{src}' -Destination '{dst}' -Force; \
         Remove-Item -LiteralPath '{src}' -Force -ErrorAction SilentlyContinue; \
         Start-Process -FilePath '{dst}'"
    );
    let escaped_script = ps_script.replace('"', "\\\"");
    let status = Command::new("powershell")
        .args([
            "-NoProfile",
            "-WindowStyle",
            "Hidden",
            "-Command",
            &format!(
                "Start-Process powershell -Verb RunAs -WindowStyle Hidden -ArgumentList '-NoProfile -ExecutionPolicy Bypass -Command \"{escaped_script}\"'"
            ),
        ])
        .status()
        .map_err(|e| format!("管理者権限での更新起動に失敗しました: {}", e))?;

    if !status.success() {
        let _ = fs::remove_file(temp_new_path);
        return Err("管理者権限の昇格がキャンセルされたか失敗しました".to_string());
    }
    Ok(())
}

#[cfg(not(target_os = "windows"))]
fn install_elevated_windows(_temp_new_path: &std::path::Path, _current_exe: &std::path::Path) -> Result<(), String> {
    Err("権限が不足しています".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_github_api_fetch() {
        // 実際にGitHub APIへ接続してパースがエラーにならないか確認
        let res = check_for_updates();
        println!("check_for_updates result: {:?}", res);
        assert!(res.is_ok(), "GitHub API チェックがエラーになりました: {:?}", res);
    }

    #[test]
    fn test_github_asset_download_header() {
        let url = "https://github.com/Altairu/AltairCAM/releases/download/v2.0.2/AltairCAM-windows-x86_64.exe";
        let mut response = ureq::get(url)
            .header("User-Agent", &format!("AltairCAM/{}", env!("APP_VERSION")))
            .call()
            .expect("v2.0.2 アセットへの接続に失敗");
        let mut buf = [0u8; 64];
        let n = response
            .body_mut()
            .as_reader()
            .read(&mut buf)
            .expect("ヘッダー読み取りに失敗");
        assert!(n >= 2, "読み取りサイズが不足: {}", n);
        assert_eq!(&buf[0..2], b"MZ", "PEヘッダー(MZ)ではありません: {:?}", &buf[0..n]);
    }
}
