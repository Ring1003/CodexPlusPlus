# cc-switch 共存兼容分析

> **日期**：2026-07-12
> **作者**：MengJie
> **性质**：纯分析文档，不含实现代码。决策已定为「感知 + 回滚即可，默认共享 `~/.codex`」。

## 一、背景与问题

我日常用 [cc-switch](https://github.com/farion1231/cc-switch) 管理 Claude Code，同时也用它配过 Codex 的 provider。现在想用 CodexPlusPlus 专门管理 Codex。

核心问题：**两个工具都把 `~/.codex/config.toml` 和 `~/.codex/auth.json` 当成自己的「live 输出目标」，且互不知晓对方存在。** 本文档分析冲突细节，并给出「在 CodexPlusPlus 单方面（不改 cc-switch）」能做的优化方案。

**决策结论**（已与作者确认）：
- 兼容深度 = **感知 + 回滚**（不碰核心 apply 写入语义）
- 目录策略 = **默认共享 `~/.codex`**（不引入独立 home）

## 二、cc-switch 对 Codex 的写入机制（调研结论）

> 证据来自 `farion1231/cc-switch` 仓库源码（`src-tauri/src/` 下 codex_config.rs / services/provider/ 等）。以下每条均有源码佐证。

### 2.1 写入目标

| 文件 | 路径 | 说明 |
|---|---|---|
| `config.toml` | `get_codex_config_dir().join("config.toml")` | 路由/endpoint/model/bearer token |
| `auth.json` | `get_codex_config_dir().join("auth.json")` | `OPENAI_API_KEY` / ChatGPT 登录缓存 |
| `cc-switch-model-catalog.json` | `get_codex_config_dir().join("cc-switch-model-catalog.json")` | cc-switch 自己的 catalog 投影 |

**关键点：cc-switch 不读 `CODEX_HOME` 环境变量**。它用自己的设置项 `AppSettings.codex_config_dir`（默认 `~/.codex`，持久化在 `~/.cc-switch/settings.json`）。所以即使用户设了 `CODEX_HOME`，cc-switch 也只认自己的设置项。

### 2.2 写入语义：整体覆盖，不保留未知字段

cc-switch 切换 provider 时的写入链路：

```
ProviderService::switch
 → write_live_with_common_config        (live.rs)
   → build_effective_settings_with_common_config   // config 字符串 + snippet 合并
   → write_live_snapshot(app_type=Codex)           (live.rs)
     → codex_config::write_codex_provider_live_with_catalog
       → prepare_codex_config_text_with_model_catalog  // DocumentMut 注入 catalog 字段
       → write_codex_live_atomic(auth, config_text)
         → write_text_file(&config_path, &cfg_text)    // ★ 字符串整体写入
```

**最终落盘走的是字符串整体写入**——传入的字符串 = DB provider 的 config TOML + common-config 合并 + catalog 注入。**磁盘上原有的、DB snapshot 里没有的字段会被丢弃**。

源码注释明确承认（`services/provider/mod.rs` `reapply_current_codex_official_live`）：
> "重写 live 会整体替换 config.toml（有意设计），[mcp_servers] 随之丢失，写完必须立刻从 DB 重新投影启用的 MCP。"

### 2.3 cc-switch 写入 config.toml 的字段清单

| 字段 | cc-switch 是否写 | 备注 |
|---|---|---|
| `model_provider` | **写** | 常量 `CC_SWITCH_CODEX_MODEL_PROVIDER_ID = "custom"` |
| `[model_providers.*]` | **写** | 用户 TOML 原文（name/base_url/wire_api 等） |
| `model` | **写** | 用户 TOML 原文 |
| `model_catalog_json` | **写**（自己的 sentinel） | `= "cc-switch-model-catalog.json"` |
| `web_search` | **条件写** | 仅 `web_search = "disabled"` 是它的 sentinel |
| `experimental_bearer_token` | **写** | 特定第三方 provider 路径 |
| `model_context_window` | 不主动写 | 用户 TOML 里有则保留；catalog 生成时会**读**它做 fallback |
| `model_auto_compact_token_limit` | 不主动写 | 全文无此字段写入逻辑 |
| `[features]` | 不主动写 | 用户 TOML 里有则保留 |
| `[mcp_servers]` | 切换后从 DB 重新投影 | 不在 provider snapshot 里 |
| `[skills]/[plugins]/[marketplaces]` | 不写 | skill 走文件系统 |

### 2.4 common-config 自动同步的排除清单

cc-switch 切换 codex provider 前，会提取当前 live config 成「共享片段」（`extract_codex_common_config`）。**排除清单**（由测试 `extract_codex_common_config_strips_provider_fields_and_injected_artifacts` 权威定义）：

- **顶层键排除**：`model_provider`、`model`、`wire_api`、`experimental_bearer_token`、`model_catalog_json`、`web_search`（仅当值为 `"disabled"`）
- **整表排除**：`[model_providers]`、`[mcp_servers]`、`[mcp.servers]`

**CodexPlusPlus 关心的字段判断**：

| 字段 | 是否会被 common-config 传播 | 依据 |
|---|---|---|
| `model_context_window` | **会传播**（不在排除清单） | commit `473c2aaa` 只排除了 routing 字段 |
| `model_auto_compact_token_limit` | **会传播**（不在排除清单） | 同上 |
| `model_catalog_json` | **不会传播**（显式排除） | 测试断言 `!extracted.contains("model_catalog_json")` |

**风险**：若用户在 cc-switch 启用了 Codex common-config，CodexPlusPlus 写的 `model_context_window` 可能被当共享配置扩散到所有 provider。

### 2.5 文件锁 / 写入标记

- **无 OS 级 flock**。并发保护只有进程内 `tokio::sync::Mutex`（`proxy/switch_lock.rs`，按 app_type 一把锁），**不同进程之间无保护**。
- **无所有权注释/时间戳/版本戳**。cc-switch 不在文件里声明「这是我写的」。
- **唯一可识别 sentinel**：
  1. `model_catalog_json = "cc-switch-model-catalog.json"`（固定文件名）
  2. `web_search = "disabled"`（仅当值等于这个字符串）

### 2.6 保护外部字段的机制：无

cc-switch 没有任何「字段保护清单」配置项。用户无法声明「这些字段不要动」。唯一的「保留」发生在首次 import（`import_default_config`，一次性把 live 读进 DB），切换 provider 后即整体覆盖。

### 2.7 热切换 / failover 是否静默重写

| 场景 | 是否写 config.toml |
|---|---|
| 用户手动切换 provider | **写**（整体覆盖） |
| Failover（故障转移） | **不写**（只动代理内存路由 + DB 指针） |
| Takeover 开（代理接管） | **写**（改写成指向本地代理） |
| Takeover 关 | **写**（从 backup 恢复） |
| 接管期间编辑 provider | **不写**（写 DB backup） |

**结论**：CodexPlusPlus 真正要警惕的是「用户手动切 provider」和「takeover 开/关」。Failover 本身不碰 config.toml。

### 2.8 cc-switch 的回读机制

- 启动时 `import_default_config` 会把 live config 整个读进 DB 作为 default provider（**收编外部写入**）。
- 编辑当前 provider 时 `strip_common_config_from_live_settings` 从 live 回填到 DB。

## 三、CodexPlusPlus 的读写链路（调研结论）

> 证据来自本仓库源码，关键位置 `crates/codex-plus-core/src/`。

### 3.1 所有写入 config.toml / auth.json 的入口

**唯一汇聚点**：`write_codex_live_atomic`（`relay_config.rs:1046`）。

直接调用它的公开函数：
- `apply_relay_config_to_home_with_protocol`（`relay_config.rs:243`）—— 老式中转注入
- `apply_pure_api_config_to_home_with_protocol`（`relay_config.rs:468`）—— 纯 API 注入
- `apply_relay_files_to_home_with_computer_use_guard`（`relay_config.rs:295`）—— 完整 config+auth 写入
- `apply_relay_config_file_to_home`（`relay_config.rs:446`）—— 只写 config.toml
- `clear_relay_config_to_home_with_auth_and_computer_use_guard`（`relay_config.rs:608`）—— 清除注入

经由 `apply_relay_files_to_home` 间接写入的高层入口：
- `apply_relay_profile_to_home_with_switch_rules_and_computer_use_guard`（`relay_config.rs:385`）—— **切换供应商主入口**

Tauri 命令层（UI 按钮真实落点，`apps/codex-plus-manager/src-tauri/src/commands.rs`）：
- `switch_relay_profile`（`commands.rs:1851`）—— 切换供应商下拉框
- `apply_relay_injection`（`commands.rs:2498`）—— 应用中转按钮
- `apply_pure_api_injection`（`commands.rs:2633`）—— 纯 API 注入按钮
- `clear_relay_injection`（`commands.rs:2734`）—— 清除注入按钮

**隐蔽写入点**：`launcher.rs:537`，每次启动 Codex 都自动调 `apply_relay_profile_to_home_with_switch_rules_and_computer_use_guard`。**这是 cc-switch 冲突的高风险时机**（用户可能刚用 cc-switch 切完，启动 codex 又被 CodexPlusPlus 盖回去，且无 UI 感知）。

### 3.2 写入前的备份机制

`create_live_backup`（`relay_config.rs:2301`）：
- 备份目录：`{home}/backups/codex-plus-live-{timestamp_millis}/`
- 目录内放写入前的 `config.toml` / `auth.json` 原始副本
- **只保留上一版，不剪枝，不轮转**
- `restore_optional_file`（`relay_config.rs:2290`）只在写入失败时自动回滚，不用于「检测外部修改后回滚」

### 3.3 CodexPlusPlus 写 config.toml 的字段集

**根级字段（整体覆盖）**：
- `model`（`relay_config.rs:1964`）
- `model_provider`（`relay_config.rs:1748`）
- `model_context_window`（`relay_config.rs:1429`）
- `model_auto_compact_token_limit`（`relay_config.rs:1432`）
- `model_catalog_json`（`relay_config.rs:1476`，相对路径 `model-catalogs/{id}.json`）

**表字段**：
- `model_providers.{id}`（整体覆盖当前 provider 的表）
- **`retain_only_provider_table`（`relay_config.rs:2236`）会删掉所有非当前 provider 的子表**，包括 legacy `CodexPlusPlus`/`CodexPP`——这是与 cc-switch 冲突的核心点

**保留合并（不覆盖）的字段**：
- `merge_common_config_into_config`（`relay_config.rs:759`）：common config 合并
- `preserve_unmanaged_live_context_entries`（`relay_config.rs:871`）：保护 live 里 mcp/skills/plugins 的非受管条目——**现有的「尊重外部编辑」钩子，但不覆盖 model_providers**
- `preserve_live_marketplace_configs`（`relay_config.rs:1127`）：保留 marketplaces 表

**auth.json**：整体覆盖（`relay_config.rs:1108`）。

### 3.4 backfill 回读链路

`backfill_relay_profile_from_home_with_common`（`relay_config.rs:686`）：从 live config 读回，**会覆盖 profile 快照**。包括：
- `restore_profile_provider_id_for_backfill`（`relay_config.rs:1751`）：保留外部 provider id
- `restore_profile_auth_from_live_config`（`relay_config.rs:1793`）：从 live 回填 token
- `sync_profile_mode_from_backfilled_live`（`relay_config.rs:1836`）：自动调整 relay_mode

**测试 1914/1972/2014/2098 锁定了「尊重外部编辑」语义**——CodexPlusPlus 已有意识地「外部改了 live 就认」。这意味着 cc-switch 写的内容会被 CodexPlusPlus 回吸收进 profile 快照。

### 3.5 文件锁 / 写入标记：无

- `atomic_write`（`settings.rs:1169`）只是 write-temp-then-rename，**无 flock，无冲突检测**。
- 仓库唯一的 fs2 锁在 `ports.rs:228`，仅用于 loopback 端口守卫，与 config.toml 无关。
- `write_codex_live_atomic` **不在 config.toml 里写任何版本号/时间戳/ownership 注释**。

### 3.6 「外部工具接管」开关：无

全仓搜索无 `co-managed`/`external manager`/`ownership`/`takeover`（computer_use_guard 除外）等概念。

已有的 `ccs_import.rs`（`crates/codex-plus-core/src/ccs_import.rs`）是**单向一次性导入**：从 `~/.cc-switch/cc-switch.db` 读 provider 转 RelayProfile，**不监听、不协调**。

### 3.7 catalog 文件的独立性

- catalog 文件：`{home}/model-catalogs/{sanitized_id}.json`（`relay_config.rs:1469`），由 `std::fs::write` 直接写，**不走 write_codex_live_atomic，无备份，无锁**。
- cc-switch 不动 `model-catalogs/` 目录（它只懂 config.toml 的标准字段）。
- **catalog 文件本身天然安全；唯一脆弱点是 config.toml 里的指针 key**（属于 3.3 的整体覆盖字段）。

### 3.8 约束改动的关键测试

| 测试 | 行号 | 约束 |
|---|---|---|
| `apply_relay_files_switches_complete_config_and_auth_json` | 522 | config+auth 完全替换语义、backup 格式 |
| `apply_relay_profile_to_home_with_switch_rules_preserves_unmanaged_live_context_entries` | 2479 | preserve_unmanaged 行为 |
| `backfill_current_profile_preserves_external_live_provider_id_edit_before_switch` | 1914 | 尊重外部 provider id |
| `backfill_official_profile_promotes_external_pure_api_live_edit_before_switch` | 1972 | 同上 |
| `backfill_official_profile_does_not_promote_codex_plus_switch_live_config` | 2056 | 区分自己写 vs 外部 |
| `apply_relay_profile_preserves_user_model_catalog_json` | 1054 | catalog 指针保护 |

**结论**：所有改动**必须 feature flag 包裹**，默认行为不变，才能不挂这一片测试。`BackendSettings`（`settings.rs`）是成熟的开关位（已有 `provider_sync_enabled`/`relay_profiles_enabled` 先例）。

## 四、冲突点汇总表

| 冲突维度 | cc-switch | CodexPlusPlus | 冲突性质 |
|---|---|---|---|
| 写入目录 | `codex_config_dir`（默认 `~/.codex`，不读 `CODEX_HOME`） | `CODEX_HOME` 或 `~/.codex` | 默认一致；用户改一边会错开 |
| config.toml 语义 | 整体覆盖 | 整体覆盖 | **硬冲突**，last-writer-wins |
| config.toml 字段 | provider/model/catalog(sentinel)/web_search | model/provider/context-limits/catalog/features | 部分重叠（provider/model/catalog） |
| common-config 传播 | `model_context_window`/`model_auto_compact_token_limit` 会传播 | 无此机制 | CodexPlusPlus 字段可能被扩散 |
| auth.json | 整体覆盖 | 整体覆盖 | **硬冲突**，零和 |
| failover | 不写 config.toml | — | 无害 |
| 手动切换 / takeover | 会写 | launcher 自动注入也会写 | 主冲突源 |
| 回读 | import 会收编 live | backfill 会吸收 live | 互相污染 SSOT |
| catalog 文件 | `cc-switch-model-catalog.json` | `model-catalogs/{id}.json` | 文件独立，指针互斥 |

## 五、优化方案（按作者选定的「感知 + 回滚」深度）

### 方案 P0：零代码缓解（文档/约定层）

**P0.1 职责分工约定**：在 README/AGENTS.md 写明「cc-switch 只管 Claude Code，Codex 交给 CodexPlusPlus」。cc-switch 的 codex 是 opt-in 的（需用户主动加 provider），不配置即不冲突。

**P0.2 目录隔离建议**：若用户愿意，可让 CodexPlusPlus 用独立 `CODEX_HOME`（如 `~/.codex-cpp`），与 cc-switch 管的 `~/.codex` 物理隔离——这是唯一 100% 消除冲突的方案，但用户需用对应 `CODEX_HOME` 启动 codex。

> 作者已选「默认共享 `~/.codex`」，故 P0.2 仅作建议项保留。

### 方案 P1：感知与提示（不改写入语义，最小侵入）★选定

**P1.1 写入指纹基线**
- **钩子点**：`relay_switch::switch_relay_profile_in_home`（`relay_switch.rs:17`）入口 + `write_codex_live_atomic`（`relay_config.rs:1046`）
- **做法**：每次 CodexPlusPlus 写入后，把「写入时间 + config.toml 的 hash/mtime」存进 `BackendSettings`
- **复用**：借鉴 `provider_sync.rs:379` 的 managedBy 元数据模式
- **测试影响**：无（纯新增字段，默认值兼容）

**P1.2 backfill 时识别外部写入**
- **钩子点**：`backfill_relay_profile_from_home_with_common`（`relay_config.rs:686`）顶部
- **做法**：读 live config 后对比指纹基线，不一致即标记 `external_manager_detected`
- **识别信号（启发式，任一触发）**：
  1. provider id 不在 CodexPlusPlus 已知 profile 集合
  2. 存在 cc-switch sentinel：`model_catalog_json = "cc-switch-model-catalog.json"` 或 `web_search = "disabled"`
  3. `~/.cc-switch/cc-switch.db` 的 mtime 在最近 N 秒内
- **UI 表现**：顶部警告条「检测到 cc-switch 可能修改了 codex 配置，CodexPlusPlus 的部分设置可能已失效」
- **测试约束**：**必须不破坏**测试 1914/1972/2014/2098——只加「标记」，不改「吸收」行为

**P1.3 RelayStatus 增加外部管理检测字段**
- **钩子点**：`relay_status_from_home`（`relay_config.rs:161`）
- **做法**：给 `RelayStatus` 加 `external_manager_detected: bool`，供 UI 实时显示警告条
- **测试影响**：`reports_relay_configured_when_required_keys_exist`（`relay_config.rs:232`）等状态测试需补默认值

### 方案 P3：回滚与恢复（事后补救）★选定

**P3.1 backup metadata.json**
- **钩子点**：`create_live_backup`（`relay_config.rs:2301`）
- **做法**：备份目录加 `metadata.json`，记录：写入时间、写入者版本、profile id、触发命令、写入前 config 的 hash
- **借鉴**：`provider_sync.rs:379` 的 managedBy 模式（已有测试覆盖）
- **测试影响**：测试 546 只断言 backup 目录含 config/auth，加 metadata.json 不破坏

**P3.2 手动回滚 API**
- **钩子点**：把 `restore_optional_file`（`relay_config.rs:2290`）包成公开的 `rollback_to_backup(home, backup_dir)`
- **UI**：「恢复到上次 CodexPlusPlus 写入前的状态」按钮
- **场景**：冲突发生后，用户可一键回到 CodexPlusPlus 上次写入前的快照

**P3.3 备份剪枝 + 外部修改快照**
- **现状**：`relay_config.rs` 内不剪枝，备份无限累积
- **做法**：仿 `provider_sync.rs:1105 prune_backups`；检测到外部修改时额外存 `backups/external-detected-{ts}/` 快照

### 选定方案的 MVP 范围

**MVP = P1（感知）+ P3（回滚）**，对应原方案表的 P1+P3 两档：
- 用户能看到冲突警告（P1.2/P1.3 UI 警告条）
- 用户能在冲突后回滚（P3.2 一键回滚按钮）
- 基础设施齐备（P1.1 指纹 + P3.1 metadata）
- **不碰核心 apply 写入语义**，所有改动 feature flag 默认关闭，现有测试不受影响

### 未选定（仅记录，本期不做）

| 方案 | 说明 | 未选原因 |
|---|---|---|
| P2 launcher 守卫 | 在 `launcher.rs:537` 启动注入前加外部检测 | 会动 launcher 测试矩阵，本期深度不足 |
| P4 provider 表保护 | 扩展 `preserve_unmanaged_*` 到 model_providers | 改核心 apply 语义，测试约束多 |
| P5 auth.json merge | 改 auth 写入为 merge 模式 | token 易错乱，高风险 |

## 六、可行性总评：中-高

**利好**：
1. 所有写入汇聚到单一函数 `write_codex_live_atomic`，拦截点集中
2. 已有「尊重外部编辑」哲学（`preserve_unmanaged_*`、backfill 尊重外部 provider id）——兼容 cc-switch 是自然延伸
3. 已有可借鉴基础设施：`BackendSettings` 开关位、`provider_sync.rs` 锁+metadata 模式
4. catalog 文件天然不受 cc-switch 影响

**风险**：
1. 无文件锁、无作者标记——检测「cc-switch 刚写过」只能靠 mtime + 内容启发式，不够可靠
2. auth.json 是零和冲突点（双方都整体覆盖），但本期不做 auth merge，规避此风险
3. launcher 自动注入是冲突重灾区，但本期不做 launcher 守卫，仅在文档提示

## 七、关键文件路径汇总

- `crates/codex-plus-core/src/relay_config.rs` — 主战场，所有读写逻辑（2624 行）
- `crates/codex-plus-core/src/relay_switch.rs` — 切换编排
- `crates/codex-plus-core/src/settings.rs` — `atomic_write` + `BackendSettings` 开关位
- `crates/codex-plus-core/src/codex_home.rs` — home 解析
- `crates/codex-plus-core/src/ccs_import.rs` — 已有 cc-switch 单向导入
- `crates/codex-plus-core/src/launcher.rs` — 隐蔽自动注入点（本期不改）
- `crates/codex-plus-data/src/provider_sync.rs` — 可借鉴的锁 + backup metadata 模式
- `apps/codex-plus-manager/src-tauri/src/commands.rs` — Tauri UI 命令层
- `crates/codex-plus-core/tests/relay_config.rs` — 测试约束（3316 行）

## 八、附录：cc-switch 可识别 sentinel 清单

供 P1.2 识别逻辑使用：
1. `model_catalog_json = "cc-switch-model-catalog.json"`（config.toml 顶层键，固定文件名）
2. `web_search = "disabled"`（仅当值严格等于此字符串）
3. `~/.cc-switch/cc-switch.db` 存在且 mtime 较新
4. `~/.cc-switch/settings.json` 存在（cc-switch 已安装配置过）

> 注：cc-switch 不留 ownership 注释，无法 100% 可靠区分「cc-switch 写的」vs「用户手改的」。P1 的警告条定位为「可能」而非「确定」。
