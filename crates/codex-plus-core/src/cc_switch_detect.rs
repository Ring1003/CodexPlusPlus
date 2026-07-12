//! cc-switch 外部管理检测模块
//!
//! 通过启发式信号判断 live config.toml 是否被 cc-switch（或其他外部工具）修改，
//! 用于在 CodexPlusPlus 被覆盖后向用户发出警告。
//!
//! 检测维度（任一命中即视为检测到外部修改）：
//! 1. 写入指纹存在但 live config 的 mtime 比指纹新
//! 2. 写入指纹存在但 live config 的内容哈希与指纹不一致
//! 3. live config 含 cc-switch 的 sentinel 字段（catalog 指针或 web_search=disabled）
//! 4. cc-switch 的数据库文件 `~/.cc-switch/cc-switch.db` 最近被修改（300 秒内）

use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

use crate::write_fingerprint::{WriteFingerprint, compute_config_hash, load_write_fingerprint};

/// cc-switch 在 config.toml 里留下的 catalog 指针 sentinel（固定文件名）
const CC_SWITCH_CATALOG_SENTINEL: &str = "cc-switch-model-catalog.json";

/// cc-switch 在 config.toml 里留下的 web_search sentinel（仅当值严格匹配）
const CC_SWITCH_WEB_SEARCH_SENTINEL: &str = "web_search = \"disabled\"";

/// cc-switch 数据库文件相对 home 目录的路径
const CC_SWITCH_DB_RELATIVE: &str = ".cc-switch/cc-switch.db";

/// cc-switch 数据库 mtime 判定窗口（秒）：最近 N 秒内被修改视为活动迹象
const CC_SWITCH_RECENT_ACTIVITY_WINDOW_SECS: u64 = 300;

/// 外部管理检测的结果
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExternalManagerDetection {
    /// 是否检测到外部工具（cc-switch）的修改
    pub detected: bool,
    /// 命中的检测原因（人类可读，用于 UI 展示）
    pub reasons: Vec<String>,
}

impl ExternalManagerDetection {
    /// 无外部修改的空结果
    pub fn none() -> Self {
        Self {
            detected: false,
            reasons: Vec::new(),
        }
    }
}

/// 执行外部管理检测
///
/// 读取 live config.toml 和写入指纹对比，同时检查 cc-switch 的 sentinel 和数据库活动。
/// `home` 是 codex 的 home 目录（通常是 `~/.codex`）。
pub fn detect_external_manager(home: &Path) -> ExternalManagerDetection {
    detect_external_manager_with_options(home, true)
}

/// 带配置的检测入口（测试用：`check_cc_switch_db=false` 可跳过真实 db 检查避免 flaky）
pub fn detect_external_manager_with_options(
    home: &Path,
    check_cc_switch_db: bool,
) -> ExternalManagerDetection {
    let mut reasons = Vec::new();

    // 信号 1 & 2：与写入指纹对比（mtime 和 hash）
    let fingerprint = match load_write_fingerprint() {
        Ok(Some(fp)) => Some(fp),
        Ok(None) => None,
        // 指纹读取失败不阻塞检测，继续其他信号
        Err(_) => None,
    };

    let live_config_text = std::fs::read_to_string(home.join("config.toml")).unwrap_or_default();

    if let Some(ref fp) = fingerprint {
        check_fingerprint_signals(home, fp, &live_config_text, &mut reasons);
    }

    // 信号 3：cc-switch sentinel 字段
    check_sentinel_signals(&live_config_text, &mut reasons);

    // 信号 4：cc-switch 数据库最近活动（测试时可关闭，避免开发者机器装了 cc-switch 导致 flaky）
    if check_cc_switch_db {
        check_cc_switch_db_activity(&mut reasons);
    }

    let detected = !reasons.is_empty();
    ExternalManagerDetection { detected, reasons }
}

/// 对比指纹：mtime 和 hash 信号
fn check_fingerprint_signals(
    home: &Path,
    fingerprint: &WriteFingerprint,
    live_config_text: &str,
    reasons: &mut Vec<String>,
) {
    // 信号 1：live config 的 mtime 比指纹记录的新
    if let Some(live_mtime) = config_mtime_secs(home) {
        // mtime 精度到秒，指纹也存秒；live 比指纹新即被外部动过
        if live_mtime > fingerprint.config_mtime_secs {
            reasons.push("config.toml 的修改时间晚于 CodexPlusPlus 上次写入".to_string());
        }
    }

    // 信号 2：live config 的 hash 与指纹不一致
    let live_hash = compute_config_hash(live_config_text);
    if live_hash != fingerprint.config_hash {
        reasons.push("config.toml 的内容与 CodexPlusPlus 上次写入不一致".to_string());
    }
}

/// 检查 config.toml 文本里的 cc-switch sentinel
fn check_sentinel_signals(live_config_text: &str, reasons: &mut Vec<String>) {
    if live_config_text.contains(CC_SWITCH_CATALOG_SENTINEL) {
        reasons.push(format!(
            "config.toml 含 cc-switch 的 catalog 指针（{CC_SWITCH_CATALOG_SENTINEL}）"
        ));
    }
    if live_config_text.contains(CC_SWITCH_WEB_SEARCH_SENTINEL) {
        reasons.push("config.toml 含 cc-switch 的 web_search=disabled sentinel".to_string());
    }
}

/// 检查 `~/.cc-switch/cc-switch.db` 是否最近被修改
fn check_cc_switch_db_activity(reasons: &mut Vec<String>) {
    let db_path = home_cc_switch_db_path();
    let Some(modified) = file_modified_secs(&db_path) else {
        return;
    };
    let now = now_secs();
    if now >= modified && now - modified <= CC_SWITCH_RECENT_ACTIVITY_WINDOW_SECS {
        reasons.push(format!(
            "cc-switch 数据库（~/{CC_SWITCH_DB_RELATIVE}）在最近 {} 秒内被修改",
            CC_SWITCH_RECENT_ACTIVITY_WINDOW_SECS
        ));
    }
}

fn home_cc_switch_db_path() -> std::path::PathBuf {
    // 优先用 directories 拿 home，回退到字面相对路径
    directories::BaseDirs::new()
        .map(|dirs| dirs.home_dir().join(CC_SWITCH_DB_RELATIVE))
        .unwrap_or_else(|| std::path::PathBuf::from(CC_SWITCH_DB_RELATIVE))
}

fn file_modified_secs(path: &Path) -> Option<u64> {
    let metadata = std::fs::metadata(path).ok()?;
    metadata.modified().ok()?.duration_since(UNIX_EPOCH).ok()?.as_secs()
}

fn config_mtime_secs(home: &Path) -> Option<u64> {
    file_modified_secs(&home.join("config.toml"))
}

fn now_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::write_fingerprint::{clear_write_fingerprint, save_write_fingerprint};
    use std::fs;
    use tempfile::tempdir;

    /// 构造一个干净的检测基线：清掉指纹、live config 无 sentinel
    fn cleanup() {
        let _ = clear_write_fingerprint();
    }

    #[test]
    fn detect_returns_none_when_no_signals() {
        // 没有指纹、没有 sentinel、cc-switch.db 检查关闭 → 不应检测到
        cleanup();
        let temp = tempdir().expect("创建临时目录失败");
        let home = temp.path();
        fs::write(home.join("config.toml"), "model = \"gpt-5\"\n").expect("写 config 失败");

        // 用 with_options 关闭 cc-switch.db 检查，避免开发者机器装了 cc-switch 导致 flaky
        let detection = detect_external_manager_with_options(home, false);
        assert!(
            !detection.detected,
            "无任何信号时不应检测到外部管理，detection = {detection:?}"
        );
        cleanup();
    }

    #[test]
    fn detect_flags_cc_switch_catalog_sentinel() {
        cleanup();
        let temp = tempdir().expect("创建临时目录失败");
        let home = temp.path();
        // 含 cc-switch catalog sentinel
        let config = format!(
            "model = \"x\"\nmodel_catalog_json = \"{CC_SWITCH_CATALOG_SENTINEL}\"\n"
        );
        fs::write(home.join("config.toml"), config).expect("写 config 失败");

        let detection = detect_external_manager(home);
        assert!(detection.detected, "应检测到 cc-switch catalog sentinel");
        assert!(
            detection.reasons.iter().any(|r| r.contains("catalog 指针")),
            "原因应含 catalog 指针：{:?}",
            detection.reasons
        );
        cleanup();
    }

    #[test]
    fn detect_flags_cc_switch_web_search_sentinel() {
        cleanup();
        let temp = tempdir().expect("创建临时目录失败");
        let home = temp.path();
        let config = "model = \"x\"\nweb_search = \"disabled\"\n";
        fs::write(home.join("config.toml"), config).expect("写 config 失败");

        let detection = detect_external_manager(home);
        assert!(detection.detected, "应检测到 web_search sentinel");
        assert!(
            detection.reasons.iter().any(|r| r.contains("web_search")),
            "原因应含 web_search：{:?}",
            detection.reasons
        );
        cleanup();
    }

    #[test]
    fn detect_flags_hash_mismatch() {
        cleanup();
        let temp = tempdir().expect("创建临时目录失败");
        let home = temp.path();

        // 先写入指纹（内容 A）
        let original = "model = \"gpt-5\"\n";
        fs::write(home.join("config.toml"), original).expect("写 config 失败");
        let fp = WriteFingerprint {
            written_at_millis: 1,
            config_hash: compute_config_hash(original),
            config_mtime_secs: 0, // 故意写 0，让 mtime 信号也命中
        };
        save_write_fingerprint(&fp).expect("保存指纹失败");

        // 模拟外部修改：改成内容 B
        fs::write(home.join("config.toml"), "model = \"gpt-6\"\n").expect("改 config 失败");

        let detection = detect_external_manager(home);
        assert!(detection.detected, "应检测到 hash 不一致");
        assert!(
            detection.reasons.iter().any(|r| r.contains("内容与 CodexPlusPlus 上次写入不一致")),
            "原因应含 hash 不一致：{:?}",
            detection.reasons
        );
        cleanup();
    }

    #[test]
    fn detect_returns_empty_when_fingerprint_missing() {
        cleanup();
        let temp = tempdir().expect("创建临时目录失败");
        let home = temp.path();
        fs::write(home.join("config.toml"), "model = \"gpt-5\"\n").expect("写 config 失败");

        let detection = detect_external_manager(home);
        // 没指纹、没 sentinel、没 cc-switch.db → 不应报 hash/mtime 信号
        let has_fingerprint_signal = detection
            .reasons
            .iter()
            .any(|r| r.contains("CodexPlusPlus 上次写入"));
        assert!(!has_fingerprint_signal, "无指纹时不应报指纹信号：{detection:?}");
        cleanup();
    }

    #[test]
    fn external_manager_detection_none_is_empty() {
        let empty = ExternalManagerDetection::none();
        assert!(!empty.detected);
        assert!(empty.reasons.is_empty());
    }
}
