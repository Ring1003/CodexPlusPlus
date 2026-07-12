//! 写入指纹模块
//!
//! 记录 CodexPlusPlus 上次写入 `config.toml` 的时间戳、内容哈希和文件 mtime，
//! 用于后续检测 live config 是否被外部工具（如 cc-switch）篡改。
//!
//! 指纹存储在独立文件 `~/.codex-session-delete/write-fingerprint.json`，
//! 不污染 BackendSettings，用 `atomic_write` 保证原子性。

use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};

use anyhow::Context;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::paths;

/// 写入指纹：记录 CodexPlusPlus 上次写入 config.toml 的状态快照
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WriteFingerprint {
    /// 写入时刻（毫秒，UNIX_EPOCH 起）
    pub written_at_millis: u128,
    /// 写入后 config.toml 内容的 SHA256 前 16 个十六进制字符
    pub config_hash: String,
    /// 写入后 config.toml 的 mtime（秒，UNIX_EPOCH 起）
    pub config_mtime_secs: u64,
}

/// 默认指纹文件路径：`~/.codex-session-delete/write-fingerprint.json`
pub fn default_write_fingerprint_path() -> PathBuf {
    paths::default_app_state_dir().join("write-fingerprint.json")
}

// ── 测试串行锁 ──
// 所有涉及指纹文件的测试（含 cc_switch_detect、tests/relay_config）必须持有此锁，
// 串行执行，避免并行测试互相覆盖/清理全局指纹文件。
// 注意：不用 #[cfg(test)]，因为集成测试（tests/ 目录）是独立 crate，
// 在 codex_plus_core 非 test cfg 编译时也要能访问此 API。
#[doc(hidden)]
pub static FINGERPRINT_TEST_LOCK: Mutex<()> = Mutex::new(());

/// 测试用：获取指纹测试串行锁的 guard。供跨模块测试共享。
#[doc(hidden)]
pub fn lock_fingerprint_for_tests() -> std::sync::MutexGuard<'static, ()> {
    FINGERPRINT_TEST_LOCK
        .lock()
        .unwrap_or_else(|poison| poison.into_inner())
}

/// 计算 config.toml 内容的哈希（SHA256 前 16 个十六进制字符）
///
/// 截断到 16 字符（64 位）足以识别内容变化，避免指纹文件过大。
pub fn compute_config_hash(text: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(text.as_bytes());
    let digest = hasher.finalize();
    hex_short(&digest)
}

fn hex_short(bytes: &[u8]) -> String {
    // 取前 8 字节（16 个十六进制字符）
    bytes
        .iter()
        .take(8)
        .map(|byte| format!("{byte:02x}"))
        .collect::<String>()
}

/// 读取 home 目录下 config.toml 的 mtime（秒）
fn config_mtime_secs(home: &Path) -> Option<u64> {
    let config_path = home.join("config.toml");
    let metadata = std::fs::metadata(&config_path).ok()?;
    Some(
        metadata
            .modified()
            .ok()?
            .duration_since(UNIX_EPOCH)
            .ok()?
            .as_secs(),
    )
}

/// 记录当前 config.toml 的指纹（写入后调用）
///
/// 读取 `home/config.toml` 的内容和 mtime，计算哈希后持久化到指纹文件。
pub fn record_write_fingerprint(home: &Path) -> anyhow::Result<()> {
    let config_path = home.join("config.toml");
    let contents = std::fs::read_to_string(&config_path).unwrap_or_default();
    let fingerprint = WriteFingerprint {
        written_at_millis: timestamp_millis(),
        config_hash: compute_config_hash(&contents),
        config_mtime_secs: config_mtime_secs(home).unwrap_or(0),
    };
    save_write_fingerprint(&fingerprint)
}

/// 加载指纹。文件不存在时返回 None（不视为错误）。
pub fn load_write_fingerprint() -> anyhow::Result<Option<WriteFingerprint>> {
    let path = default_write_fingerprint_path();
    let contents = match std::fs::read_to_string(&path) {
        Ok(contents) => contents,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => {
            return Err(error).with_context(|| format!("读取写入指纹失败：{}", path.display()))
        }
    };
    if contents.trim().is_empty() {
        return Ok(None);
    }
    let fingerprint: WriteFingerprint = serde_json::from_str(&contents)
        .with_context(|| format!("写入指纹 JSON 解析失败：{}", path.display()))?;
    Ok(Some(fingerprint))
}

/// 保存指纹到默认路径
pub fn save_write_fingerprint(fingerprint: &WriteFingerprint) -> anyhow::Result<()> {
    let path = default_write_fingerprint_path();
    let bytes = serde_json::to_vec_pretty(fingerprint)?;
    crate::settings::atomic_write(&path, &bytes)
}

/// 清除指纹文件（停用兼容感知时调用）
pub fn clear_write_fingerprint() -> anyhow::Result<()> {
    let path = default_write_fingerprint_path();
    match std::fs::remove_file(&path) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error.into()),
    }
}

fn timestamp_millis() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::tempdir;

    // 纯哈希计算测试不涉及文件，无需持锁。

    #[test]
    fn compute_config_hash_is_deterministic() {
        // 相同内容应产生相同哈希
        let hash_a = compute_config_hash("model = \"gpt-5\"\n");
        let hash_b = compute_config_hash("model = \"gpt-5\"\n");
        assert_eq!(hash_a, hash_b);
    }

    #[test]
    fn compute_config_hash_differs_on_content_change() {
        // 内容变化应产生不同哈希
        let hash_a = compute_config_hash("model = \"gpt-5\"\n");
        let hash_b = compute_config_hash("model = \"gpt-6\"\n");
        assert_ne!(hash_a, hash_b);
    }

    #[test]
    fn compute_config_hash_is_hex_and_short() {
        // 哈希应为 16 个十六进制字符
        let hash = compute_config_hash("hello");
        assert_eq!(hash.len(), 16);
        assert!(hash.chars().all(|c| c.is_ascii_hexdigit()));
    }

    // 以下测试涉及全局指纹文件，用串行锁避免并行竞争。
    // macOS 上并行竞争导致 flaky，Windows 上路径重定向会失败，故统一用锁串行。

    #[test]
    fn load_write_fingerprint_returns_none_when_missing() {
        let _guard = lock_fingerprint_for_tests();
        let _ = clear_write_fingerprint();
        let result = load_write_fingerprint();
        assert!(result.is_ok());
        assert!(result.unwrap().is_none());
    }

    #[test]
    fn save_and_load_roundtrip() {
        let _guard = lock_fingerprint_for_tests();
        let fingerprint = WriteFingerprint {
            written_at_millis: 1_700_000_000_000,
            config_hash: "abc123def456abc0".to_string(),
            config_mtime_secs: 1_700_000_000,
        };
        save_write_fingerprint(&fingerprint).expect("保存指纹失败");
        let loaded = load_write_fingerprint().expect("加载指纹失败");
        assert_eq!(loaded, Some(fingerprint));
        let _ = clear_write_fingerprint();
    }

    #[test]
    fn record_write_fingerprint_reads_live_config() {
        let _guard = lock_fingerprint_for_tests();
        let temp = tempdir().expect("创建临时目录失败");
        let home = temp.path();
        fs::write(home.join("config.toml"), "model = \"test\"\n").expect("写 config 失败");

        record_write_fingerprint(home).expect("记录指纹失败");

        let loaded = load_write_fingerprint().expect("加载指纹失败");
        let fingerprint = loaded.expect("应有指纹");
        assert_eq!(fingerprint.config_hash, compute_config_hash("model = \"test\"\n"));
        let _ = clear_write_fingerprint();
    }

    #[test]
    fn clear_write_fingerprint_is_idempotent() {
        let _guard = lock_fingerprint_for_tests();
        // 连续清除不应报错
        clear_write_fingerprint().expect("第一次清除失败");
        clear_write_fingerprint().expect("第二次清除失败");
    }
}
