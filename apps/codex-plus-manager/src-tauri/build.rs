fn main() {
    // 确保 Tauri externalBin 指向的 sidecar 文件在编译期存在。
    // tauri-build 会校验 externalBin 路径，若文件不存在则编译失败。
    // 日常 cargo check/test 时 launcher 尚未编译，此处创建空占位文件让校验通过；
    // CI 打 bundle 前会用真实 launcher 二进制覆盖此占位。
    ensure_sidecar_placeholder();

    let windows = tauri_build::WindowsAttributes::new()
        .app_manifest(include_str!("windows-app-manifest.xml"));
    let attrs = tauri_build::Attributes::new().windows_attributes(windows);
    tauri_build::try_build(attrs).expect("failed to run Tauri build script");
}

/// 为当前编译目标创建 sidecar 占位文件（若不存在）。
/// sidecar 名为 codex-plus-plus-<target-triple>[.exe]，位于 src-tauri 上级目录的 codex-plus-launcher 下。
/// tauri.conf.json 的 externalBin 配置为 "../codex-plus-launcher/codex-plus-plus"。
fn ensure_sidecar_placeholder() {
    let target = std::env::var("TARGET").unwrap_or_default();
    if target.is_empty() {
        return;
    }
    let is_windows = target.contains("windows");
    let ext = if is_windows { ".exe" } else { "" };
    // externalBin 相对路径：src-tauri 上两级到 apps/，再进 codex-plus-launcher
    // CARGO_MANIFEST_DIR 指向 src-tauri 目录
    let manifest_dir = std::env::var("CARGO_MANIFEST_DIR").unwrap_or_else(|_| ".".to_string());
    let launcher_dir = std::path::Path::new(&manifest_dir)
        .join("..")
        .join("..")
        .join("codex-plus-launcher");
    let sidecar_name = format!("codex-plus-plus-{target}{ext}");
    let sidecar_path = launcher_dir.join(&sidecar_name);
    if !sidecar_path.exists() {
        // 创建空占位文件（0 字节），让 tauri-build 的 externalBin 校验通过
        let _ = std::fs::create_dir_all(&launcher_dir);
        let _ = std::fs::File::create(&sidecar_path);
        println!(
            "cargo:warning=created sidecar placeholder at {}",
            sidecar_path.display()
        );
    }
    // 占位文件不应触发 rebuild（内容无关），但路径变化要重新跑
    println!("cargo:rerun-if-changed={}", sidecar_path.display());
}
