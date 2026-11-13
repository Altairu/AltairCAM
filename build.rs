fn main() {
    if std::env::var("CARGO_CFG_TARGET_OS").unwrap_or_default() == "windows" {
        let mut res = winres::WindowsResource::new();
        if std::path::Path::new("AltairCAM.ico").exists() {
            res.set_icon("AltairCAM.ico");
        }
        if let Err(e) = res.compile() {
            eprintln!("winres warning: {}", e);
        }
    }
}
