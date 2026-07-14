use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

pub mod macos;
pub mod windows;

pub const SILENT_NAME: &str = "Codex++";
pub const MANAGER_NAME: &str = "Codex++ 管理工具";
pub const SILENT_BINARY: &str = "codex-plus-plus";
pub const MANAGER_BINARY: &str = "codex-plus-plus-manager";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct InstallOptions {
    #[serde(default)]
    pub install_root: Option<PathBuf>,
    #[serde(default)]
    pub launcher_path: Option<PathBuf>,
    #[serde(default)]
    pub manager_path: Option<PathBuf>,
    #[serde(default)]
    pub remove_owned_data: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ShortcutState {
    pub installed: bool,
    pub path: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct EntryPointState {
    pub silent_shortcut: ShortcutState,
    pub management_shortcut: ShortcutState,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct InstallActionResult {
    pub status: String,
    pub message: String,
    pub silent_shortcut: ShortcutState,
    pub management_shortcut: ShortcutState,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MacosAppBundle {
    pub app_path: PathBuf,
    pub info_plist: String,
    pub launch_script: String,
    pub binary_source: Option<PathBuf>,
    pub binary_target_name: Option<String>,
}

impl ShortcutState {
    pub fn missing(path: Option<PathBuf>) -> Self {
        Self {
            installed: false,
            path: path.map(|path| path.to_string_lossy().to_string()),
        }
    }

    pub fn from_candidates(candidates: Vec<PathBuf>) -> Self {
        if let Some(path) = candidates.iter().find(|path| path.exists()) {
            return Self {
                installed: true,
                path: Some(path.to_string_lossy().to_string()),
            };
        }
        Self::missing(candidates.into_iter().next())
    }
}

pub fn shortcut_names() -> (&'static str, &'static str) {
    ("Codex++.lnk", "Codex++ 管理工具.lnk")
}

pub fn app_bundle_names() -> (&'static str, &'static str) {
    ("Codex++.app", "Codex++ 管理工具.app")
}

pub fn inspect_entrypoints() -> EntryPointState {
    let root = default_install_root();
    EntryPointState {
        silent_shortcut: ShortcutState::from_candidates(entrypoint_candidates(&root, false)),
        management_shortcut: ShortcutState::from_candidates(entrypoint_candidates(&root, true)),
    }
}

pub fn install_entrypoints(options: &InstallOptions) -> InstallActionResult {
    let result = platform_install(options);
    action_result(result, "入口已安装。")
}

pub fn uninstall_entrypoints(options: &InstallOptions) -> InstallActionResult {
    let result = platform_uninstall(options);
    if result.is_ok() && options.remove_owned_data {
        let _ = remove_owned_data();
    }
    action_result(result, "入口已卸载。")
}

pub fn repair_entrypoints(options: &InstallOptions) -> InstallActionResult {
    let result = platform_install(options);
    action_result(result, "入口已修复。")
}

pub fn build_windows_entrypoint_plan(options: &InstallOptions) -> windows::WindowsEntrypointPlan {
    windows::build_windows_entrypoint_plan(options)
}

pub fn build_macos_app_bundle(options: &InstallOptions, manager: bool) -> MacosAppBundle {
    macos::build_app_bundle(options, manager)
}

pub fn remove_owned_data() -> std::io::Result<()> {
    let dir = crate::paths::default_app_state_dir();
    if dir.exists() {
        std::fs::remove_dir_all(dir)?;
    }
    Ok(())
}

pub fn default_install_root() -> Option<PathBuf> {
    #[cfg(windows)]
    {
        return crate::windows_integration::desktop_dir().or_else(|| {
            directories::UserDirs::new().and_then(|dirs| dirs.desktop_dir().map(PathBuf::from))
        });
    }

    #[cfg(target_os = "macos")]
    {
        let sys_apps = PathBuf::from("/Applications");
        // 旧双-.app 模式 或 Tauri single-bundle 模式（当前 exe 所在 .app）
        if sys_apps.join(format!("{SILENT_NAME}.app")).exists()
            || sys_apps.join(format!("{MANAGER_NAME}.app")).exists()
        {
            return Some(sys_apps);
        }
        if let Ok(exe) = std::env::current_exe() {
            // Tauri single-bundle：检查当前 exe 所在的 .app 是否在 /Applications
            if let Some((dir, app_name)) = macos_applications_dir_and_app_name_from_exe(&exe) {
                if is_macos_applications_dir(&dir) {
                    return Some(dir);
                }
                // 也检查 .app 是否已装到 /Applications
                if sys_apps.join(&app_name).exists() {
                    return Some(sys_apps);
                }
            }
            if let Some(dir) = macos_applications_dir_from_exe(&exe) {
                if is_macos_applications_dir(&dir) {
                    return Some(dir);
                }
            }
        }
        return Some(sys_apps);
    }

    #[cfg(not(any(windows, target_os = "macos")))]
    {
        directories::UserDirs::new().and_then(|dirs| dirs.desktop_dir().map(PathBuf::from))
    }
}

pub fn default_install_root_strategy() -> &'static str {
    if cfg!(windows) {
        "windows-known-folder"
    } else if cfg!(target_os = "macos") {
        "macos-applications"
    } else {
        "user-dirs-desktop"
    }
}

fn platform_install(options: &InstallOptions) -> anyhow::Result<()> {
    #[cfg(windows)]
    {
        windows::install_shortcuts(options)
    }

    #[cfg(target_os = "macos")]
    {
        macos::install_app_bundles(options)
    }

    #[cfg(not(any(windows, target_os = "macos")))]
    {
        let _ = options;
        anyhow::bail!("当前平台暂不支持安装 Codex++ 入口")
    }
}

fn platform_uninstall(options: &InstallOptions) -> anyhow::Result<()> {
    #[cfg(windows)]
    {
        windows::uninstall_shortcuts(options)
    }

    #[cfg(target_os = "macos")]
    {
        macos::uninstall_app_bundles(options)
    }

    #[cfg(not(any(windows, target_os = "macos")))]
    {
        let _ = options;
        anyhow::bail!("当前平台暂不支持卸载 Codex++ 入口")
    }
}

fn action_result(result: anyhow::Result<()>, success_message: &str) -> InstallActionResult {
    let state = inspect_entrypoints();
    match result {
        Ok(()) => InstallActionResult {
            status: "ok".to_string(),
            message: success_message.to_string(),
            silent_shortcut: state.silent_shortcut,
            management_shortcut: state.management_shortcut,
        },
        Err(error) => InstallActionResult {
            status: "failed".to_string(),
            message: error.to_string(),
            silent_shortcut: state.silent_shortcut,
            management_shortcut: state.management_shortcut,
        },
    }
}

fn entrypoint_candidates(root: &Option<PathBuf>, manager: bool) -> Vec<PathBuf> {
    let Some(root) = root else {
        return Vec::new();
    };
    let name = if manager { MANAGER_NAME } else { SILENT_NAME };
    if cfg!(windows) {
        vec![root.join(format!("{name}.lnk"))]
    } else if cfg!(target_os = "macos") {
        // Tauri single-bundle 模式下只有一个 <productName>.app，silent 和 manager 都指向它。
        // 候选顺序：当前 exe 所在的 .app（productName 派生，自适应）→ 旧的双 .app 名
        let mut candidates = vec![root.join(format!("{name}.app"))];
        if let Ok(exe) = std::env::current_exe() {
            if let Some((_, app_name)) = macos_applications_dir_and_app_name_from_exe(&exe) {
                let current_app = root.join(&app_name);
                if current_app != candidates[0] {
                    candidates.insert(0, current_app);
                }
            }
        }
        candidates
    } else {
        vec![root.join(format!("{name}.desktop"))]
    }
}

pub fn option_or_current_exe(value: &Option<PathBuf>, binary: &str) -> PathBuf {
    if let Some(value) = value {
        return value.clone();
    }
    let exe = std::env::current_exe().unwrap_or_else(|_| PathBuf::from("."));
    companion_binary_path_from_exe(&exe, binary)
}

pub fn companion_binary_path(binary: &str) -> PathBuf {
    let exe = std::env::current_exe().unwrap_or_else(|_| PathBuf::from("."));
    companion_binary_path_from_exe(&exe, binary)
}

pub fn companion_binary_path_from_exe(exe: &Path, binary: &str) -> PathBuf {
    let dir = exe.parent().unwrap_or_else(|| Path::new("."));
    let suffix = if cfg!(windows) { ".exe" } else { "" };
    if let Some(bundle_binary) = macos_companion_binary_from_exe(exe, binary) {
        return bundle_binary;
    }
    // 1. 优先找无 triple 后缀的同目录兄弟（传统安装方式）
    let same_bundle = dir.join(binary);
    if same_bundle.exists() {
        return same_bundle;
    }
    // 2. Tauri sidecar fallback：找带 target triple 后缀的版本
    //    sidecar 文件名格式：<binary>-<target-triple>[.exe]
    //    例如 codex-plus-plus-aarch64-apple-darwin / codex-plus-plus-x86_64-pc-windows-msvc.exe
    if let Some(sidecar_path) = find_sidecar_binary(dir, binary, suffix) {
        return sidecar_path;
    }
    dir.join(format!("{binary}{suffix}"))
}

/// 在目录下查找 Tauri sidecar 二进制（带 target triple 后缀）。
/// Tauri externalBin 打包后会按 host triple 重命名，运行时通过本函数回退定位。
/// triple 格式如：aarch64-apple-darwin / x86_64-pc-windows-msvc
fn find_sidecar_binary(dir: &Path, binary: &str, suffix: &str) -> Option<PathBuf> {
    let prefix = format!("{binary}-");
    let entries = std::fs::read_dir(dir).ok()?;
    for entry in entries.flatten() {
        let file_name = entry.file_name();
        let Some(name) = file_name.to_str() else { continue };
        // 匹配 <binary>-<triple>[.exe]
        if !name.starts_with(&prefix) {
            continue;
        }
        let after_prefix = name.strip_prefix(&prefix).unwrap_or("");
        // triple 含连字符（如 arch-vendor-os）；排除 .sig/.txt 等非二进制
        if suffix.is_empty() {
            // 非 Windows：完整文件名就是 <binary>-<triple>，无扩展名
            if after_prefix.contains('-') && !after_prefix.contains('.') {
                return Some(entry.path());
            }
        } else {
            // Windows：<binary>-<triple>.exe
            if let Some(triple) = after_prefix.strip_suffix(suffix) {
                if triple.contains('-') {
                    return Some(entry.path());
                }
            }
        }
    }
    None
}

fn macos_companion_binary_from_exe(exe: &Path, binary: &str) -> Option<PathBuf> {
    let (applications_dir, app_name) = macos_applications_dir_and_app_name_from_exe(exe)?;
    // single-bundle 模式：Tauri 打包后 exe 在 <productName>.app/Contents/MacOS/ 内，
    // launcher 作为 sidecar 也在同目录（带 target triple 后缀），manager 就是 productName 派生名。
    // 兼容旧的双 .app 模式：Codex++.app / Codex++ 管理工具.app（独立安装）。
    if binary == SILENT_BINARY {
        // launcher 的多种可能位置，按优先级查找
        // 1. 当前 bundle 内的 sidecar（Tauri single-bundle 模式）
        if let Some(sidecar) = find_sidecar_in_macos_dir(exe) {
            return Some(sidecar);
        }
        // 2. 旧双-bundle 模式：Codex++.app
        if app_name == format!("{SILENT_NAME}.app") {
            return Some(macos_preferred_bundle_binary(
                exe,
                SILENT_BINARY,
                "CodexPlusPlus",
            ));
        }
        // 3. Tauri single-bundle 模式：launcher 作为 sidecar 在 CodexPlusPlus.app 内
        let tauri_app_macos = applications_dir
            .join(&app_name)
            .join("Contents")
            .join("MacOS");
        if let Some(sidecar) = find_sidecar_binary(&tauri_app_macos, SILENT_BINARY, "") {
            if sidecar.exists() {
                return Some(sidecar);
            }
        }
        // 4. 从 /Applications/Codex++.app 查找（旧独立安装）
        //    即使文件不存在也返回最可能的路径（兼容测试假设和首次安装场景）
        let macos = applications_dir
            .join(format!("{SILENT_NAME}.app"))
            .join("Contents")
            .join("MacOS");
        return Some(
            macos
                .join(SILENT_BINARY)
                .exists()
                .then(|| macos.join(SILENT_BINARY))
                .unwrap_or_else(|| macos.join("CodexPlusPlus")),
        );
    }
    if binary == MANAGER_BINARY {
        // manager 的多种可能位置
        // 1. Tauri single-bundle 模式：manager 可执行名 = productName = <app_name 去掉 .app>
        //    launcher (sidecar) 和 manager 都在 <productName>.app/Contents/MacOS/ 内
        let macos_dir = exe.parent();
        if let Some(dir) = macos_dir {
            // productName 派生名：app_name 是 <productName>.app
            let product_name = app_name.strip_suffix(".app").unwrap_or(&app_name);
            let manager_candidate = dir.join(product_name);
            if manager_candidate.exists() {
                return Some(manager_candidate);
            }
            // 也尝试旧的 manager 二进制名（开发模式）
            let legacy_candidate = dir.join(MANAGER_BINARY);
            if legacy_candidate.exists() {
                return Some(legacy_candidate);
            }
        }
        // 2. 旧双-bundle 模式：Codex++ 管理工具.app
        if app_name == format!("{MANAGER_NAME}.app") {
            return Some(macos_preferred_bundle_binary(
                exe,
                MANAGER_BINARY,
                "CodexPlusPlusManager",
            ));
        }
        // 3. 从 /Applications/Codex++ 管理工具.app 查找（旧独立安装）
        //    即使文件不存在也返回最可能的路径（兼容测试假设）
        let macos = applications_dir
            .join(format!("{MANAGER_NAME}.app"))
            .join("Contents")
            .join("MacOS");
        return Some(
            macos
                .join(MANAGER_BINARY)
                .exists()
                .then(|| macos.join(MANAGER_BINARY))
                .unwrap_or_else(|| macos.join("CodexPlusPlusManager")),
        );
    }
    None
}

/// 在 macOS bundle 的 MacOS 目录内查找 sidecar（带 target triple 后缀）。
/// 用于 Tauri single-bundle 模式：launcher 作为 sidecar 与 manager 同目录。
fn find_sidecar_in_macos_dir(exe: &Path) -> Option<PathBuf> {
    let macos_dir = exe.parent()?;
    find_sidecar_binary(macos_dir, SILENT_BINARY, "")
}

fn macos_preferred_bundle_binary(
    exe: &Path,
    sidecar_name: &str,
    bundle_executable_name: &str,
) -> PathBuf {
    let macos = exe.parent().unwrap_or_else(|| Path::new("."));
    let sidecar = macos.join(sidecar_name);
    if sidecar.exists() {
        return sidecar;
    }
    let bundle_executable = macos.join(bundle_executable_name);
    if bundle_executable.exists() {
        return bundle_executable;
    }
    exe.to_path_buf()
}

#[cfg(target_os = "macos")]
fn macos_applications_dir_from_exe(exe: &Path) -> Option<PathBuf> {
    macos_applications_dir_and_app_name_from_exe(exe).map(|(dir, _)| dir)
}

fn macos_applications_dir_and_app_name_from_exe(exe: &Path) -> Option<(PathBuf, String)> {
    let mut path = exe;
    while let Some(parent) = path.parent() {
        if path.extension().and_then(|extension| extension.to_str()) == Some("app") {
            let app_name = path.file_name()?.to_string_lossy().to_string();
            return Some((parent.to_path_buf(), app_name));
        }
        path = parent;
    }
    None
}

#[cfg(target_os = "macos")]
fn is_macos_applications_dir(path: &Path) -> bool {
    if path == Path::new("/Applications") {
        return true;
    }
    directories::BaseDirs::new()
        .map(|dirs| path == dirs.home_dir().join("Applications"))
        .unwrap_or(false)
}

pub(crate) fn install_root_or_default(options: &InstallOptions) -> PathBuf {
    options
        .install_root
        .clone()
        .or_else(default_install_root)
        .unwrap_or_else(|| PathBuf::from("."))
}
