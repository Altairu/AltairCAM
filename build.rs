use std::process::Command;

fn detect_version() -> String {
    // 1. GitHub Actionsなどのタグ参照環境変数があれば最優先で使用
    if let Ok(ref_name) = std::env::var("GITHUB_REF_NAME") {
        let trimmed = ref_name.trim().trim_start_matches(['v', 'V']);
        if !trimmed.is_empty() && trimmed.chars().next().is_some_and(|c| c.is_ascii_digit()) {
            return trimmed.to_string();
        }
    }

    // 2. 現在のHEADに完全一致するGitタグがあれば使用
    if let Ok(output) = Command::new("git")
        .args(["describe", "--tags", "--exact-match"])
        .output()
    {
        if output.status.success() {
            let tag = String::from_utf8_lossy(&output.stdout);
            let trimmed = tag.trim().trim_start_matches(['v', 'V']);
            if !trimmed.is_empty() && trimmed.chars().next().is_some_and(|c| c.is_ascii_digit()) {
                return trimmed.to_string();
            }
        }
    }

    // 3. フォールバックとしてCargo.tomlのバージョンを使用
    std::env::var("CARGO_PKG_VERSION").unwrap_or_else(|_| "2.0.1".to_string())
}

fn main() {
    println!("cargo:rerun-if-env-changed=GITHUB_REF_NAME");
    println!("cargo:rerun-if-changed=.git/HEAD");
    println!("cargo:rerun-if-changed=.git/refs/tags");

    let version = detect_version();
    println!("cargo:rustc-env=APP_VERSION={}", version);

    if std::env::var("CARGO_CFG_TARGET_OS").unwrap_or_default() == "windows" {
        let mut res = winres::WindowsResource::new();
        if std::path::Path::new("AltairCAM.ico").exists() {
            res.set_icon("AltairCAM.ico");
        }
        res.set("ProductVersion", &version);
        res.set("FileVersion", &version);
        if let Err(e) = res.compile() {
            eprintln!("winres warning: {}", e);
        }
    }
}
