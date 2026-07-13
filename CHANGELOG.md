# 更新日志

## 1.0.8 - 2026-07-13

跨供应商模型路由（实验性）+ 预设清理。

- **跨供应商模型路由（feature flag，默认关闭）**：在 Codex 增强页开启「跨供应商模型路由」后，注入脚本 patch fetch，第三方模型（带供应商前缀如 `DeepSeek / deepseek-v4-pro`）的 API 请求自动改写到本地代理，代理按前缀路由到对应供应商上游。官方模型（无前缀）直连 api.openai.com 不受影响。
  - 代理层新增 `find_relay_by_provider_prefix`（按供应商前缀查找 profile）+ `relay_profile_api_key_for_proxy`（统一提取 api_key）
  - 代理收到带前缀的 model 后剥离前缀，转发纯模型名给上游
  - 官方请求零代理介入，token 不碰代理进程
- **预设清理**：去除所有中转站预配置，仅保留 OpenAI 官方、DeepSeek、智谱 GLM、Kimi Code + 一个通用的 OpenAI 兼容接口预设。

## 1.0.7 - 2026-07-13

修复 Tauri 打包后概览页「静默启动入口」「管理工具入口」显示缺失的问题。

- **entrypoint_candidates**：macOS 候选列表新增「当前 exe 所在的 .app」（productName 派生，自适应），Tauri single-bundle 模式下 silent 和 manager 都指向同一个 `CodexPlusPlus.app`，两项均显示已安装。
- **default_install_root**：macOS 路径推断新增对当前 exe 所在 `.app` 的识别，不再只认旧的 `Codex++.app` / `Codex++ 管理工具.app`。

## 1.0.6 - 2026-07-13

修复 v1.0.5 Tauri bundler 改造后遗留的打包逻辑问题。

- **P0 修复 macOS companion 路径解析**：Tauri single-bundle 模式下（launcher 作为 sidecar 与 manager 同在 `<productName>.app/Contents/MacOS/` 内），旧的 `macos_companion_binary_from_exe` 假设两个独立 `.app`，导致打包后找不到 launcher。重写为：优先在当前 MacOS 目录用 `find_sidecar_binary` 找 sidecar；manager 用 productName 派生名（`<app_name>.app` 去后缀）定位。
- **P0 修复 launcher 找 manager**：Tauri 打包后 manager 可执行名是 productName 派生（`CodexPlusPlus`），不是旧的 `codex-plus-plus-manager`。从 `.app` 名推导 productName 作为候选。
- **P2 UI 文案**：AboutScreen 标题「GitHub Release 更新」→「在线升级」；删除死字段（资源/进度 Metric，Tauri updater 格式不再有 assetName）；placeholder 改为「在线升级会自动下载并安装，无需手动操作」；notice 标题同步。

## 1.0.5 - 2026-07-12

把更新机制从「下载全量安装包重新安装」改为 Tauri 官方 updater 在线升级。

- **在线升级**：检测到新版本后，直接在后台下载更新并自动安装，无需手动走安装向导。
- **自动重启**：更新安装完成后自动重启 manager 应用，重启后即生效为新版本。
- **真实下载进度**：前端进度条改为监听后端真实下载字节进度事件，不再是假进度。
- **ed25519 签名校验**：更新包用 Tauri 自带 ed25519 签名（免费，非系统代码签名），客户端校验签名防篡改。
- **launcher 作为 sidecar**：launcher (codex-plus-plus) 改为 manager 的 Tauri sidecar (externalBin) 打进同一个安装包，companion 路径解析适配 sidecar triple 后缀。
- **CI 改用 Tauri bundler**：Windows/macOS 构建从手写 NSIS/DMG 脚本迁移到 `tauri build`，latest.json 改为 Tauri updater 标准格式（含 platforms/signature/pub_date）。
- 文案：「下载并运行安装包」→「在线升级」。

注意：本次改动不考虑存量用户迁移（安装目录结构变化）。新版本首次需手动下载安装一次，之后即可在线升级。

## 1.0.4 - 2026-07-12

校准国内三家头部厂商 Coding Plan 预设的官方信息。

- **智谱 GLM**：默认模型 `glm-5.1` → `glm-5.2`（2026/6/13 全量开放给 Coding Plan 用户，支持 1M 上下文）；模型列表补全 glm-5 / glm-4.7 / glm-4.6；apiKeyUrl 改为 GLM Coding 官方页。Base URL `https://open.bigmodel.cn/api/coding/paas/v4`（Coding Plan 专用端点）保持不变。
- **Kimi Code**：从开放平台按量计费端点修正为 **Kimi Code 会员权益（Coding Plan）专用端点** `https://api.kimi.com/coding/v1`；模型改为官方统一的 `kimi-for-coding`（+高速版 `kimi-for-coding-highspeed`），后端随官方旗舰模型自动升级。名称从 "Kimi" 改为 "Kimi Code"。
- **DeepSeek**：核对无误，保持不变（base_url `https://api.deepseek.com`，模型 `deepseek-v4-flash` / `deepseek-v4-pro`，V4 支持 1M 上下文，兼容 OpenAI ChatCompletions）。

## 1.0.3 - 2026-07-12

cc-switch 共存兼容功能版本，在与 cc-switch 同时管理 codex 时提供感知与回滚能力。

- **cc-switch 兼容感知**：检测 cc-switch（或其他外部工具）对 `~/.codex/config.toml` 的修改，在供应商列表顶部显示玻璃化警告条。
- **写入指纹机制**：CodexPlusPlus 每次写入后记录 config.toml 的时间戳与内容哈希到独立指纹文件，用于识别后续的外部篡改。
- **手动回滚**：检测到外部覆盖后，警告条提供一键回滚到上次 CodexPlusPlus 写入前状态的入口；回滚前会自动创建「回滚前快照」备份。
- **backup metadata.json**：备份目录新增元数据文件，记录写入者、触发命令和内容哈希，便于诊断。
- **feature flag**：新增 `ccSwitchCompatEnabled` 开关（默认关闭），在「Codex 增强」页可开启；关闭时所有现有行为完全不变。
- 识别信号包括：cc-switch 的 catalog 指针 / web_search sentinel、指纹哈希不一致、mtime 变化、cc-switch 数据库近期活动。

## 1.0.2 - 2026-07-12

- 会话分页修复。

## 1.0.1 - 2026-07-12

fork 仓库（Ring1003/CodexPlusPlus）首个独立版本，基于 BigPizzaV3/CodexPlusPlus 净化与重构。

- **净化**：移除原仓库内置的全部中转站 / 赞助商广告模块、远程广告列表（BigPizzaV3/Ad-List）、交流群 / 频道（QQ群 / 微信群 / Telegram / Discord）、个人收款码与友情链接。
- **安全审计**：完成原作者代码后门检查，未发现远程上报 / 隐藏外联 / 可疑代码。
- **项目地址**：全部源码、文档、自动更新源迁移至 Ring1003/CodexPlusPlus。
- **UI 重设计**：前端管理工具采用 macOS 26 液态玻璃（Liquid Glass）风格——半透明毛玻璃面板、动态光晕背景、光泽边框、柔化漫射阴影。
- 保留 Codex++ 核心增强能力：中转注入、会话管理、模型粒度上下文窗口、增强功能、脚本市场、Zed 远程等。

## 1.2.22 - 2026-06-28

- 修复启动 Codex 时会自动应用当前供应商配置的问题；现在只有手动点击“使用/切换供应商”才会切换供应商配置。
- 保留已开启的自动会话同步、插件市场配置修复、Computer Use guard 和历史模型名清理启动流程。

## 1.2.21 - 2026-06-28

- Codex 增强新增「插件列表全量展示」开关，进入插件页后自动连续展开「更多」入口。
- 自动展开支持「查看 ... 以及另外 N 个」和英文「View/Show ... and N more」按钮文案，减少插件市场分批展示时的重复点击。
- 自动展开默认开启，可在 Codex 增强页独立关闭；关闭后会停止后续自动展开任务。

## 1.2.20 - 2026-06-27

- 模型列表改为逐行控件：每行同时编辑模型名和上下文窗口，减少模型与窗口配置错位。
- 新增本地会话多选、全选、清空选择与批量删除；批量删除会逐项统计成功和失败。
- 修复供应商详情切换时模型行数据可能沿用上一供应商的问题。
- 修复从上游获取模型时未使用当前编辑中供应商配置的问题。
- 修复批量删除确认框中的会话预览换行显示。
- 修复 Windows 缺少 `sh` 时上游 worktree 远端脚本语法测试失败的问题。
- 更新聚合供应商设置 roundtrip 测试，使其匹配保存时的规范化行为。

## 1.2.18 - 2026-06-25

- 模型列表改为左右双输入框：左侧填模型名，右侧填上下文窗口（如 `1M`、`200K` 或 `1000000`），右侧留空则使用 Codex 默认长度。
- 存储层新增 `model_windows` JSON map，与 `model_list` 彻底分离；Codex 客户端只使用无后缀模型名，避免模型选择器出现带后缀的历史项。
- 旧版 `deepseek-v4-flash[1M]` 格式在 settings 加载/保存时自动迁移到新格式。
- 启动时自动清理历史 session 数据库与 Local Storage 中残留的带后缀模型名。
- 修复 model 为空时从 `model_list` 首条无后缀 slug 回退写入 `config.toml` 的问题。
- 修复本 profile 生成的 `model_catalog_json` 在配置未变更时不会重新生成的问题。

## 1.2.4 - 2026-06-08

- 新增 Zed 远程项目记录能力，支持维护 Codex++ 可识别的远程项目最近列表，并为远程工作区打开提供更稳定的回退策略。
- 修复供应商同步在存在多条 `session_meta` 记录时只处理部分会话元数据的问题。
- 修复 Windows 单实例启动保护，在默认端口被异常占用时改用更稳健的锁与端口回退逻辑，降低无法启动的概率。
- 限制 Codex 快速服务档位只对支持的模型生效，避免不兼容模型收到无效配置。
- 修复 macOS DMG 打包和 bundle 结构，恢复 launcher / manager 二进制重命名逻辑。
- 补充混合登录中继模式文档说明。
- 版本号更新到 `1.2.4`，同步 Rust workspace、Tauri、前端 package 和后端展示版本。

## 1.1.8 - 2026-05-26

- 新增上游分支 worktree 支持，可从上游仓库/分支创建和选择独立工作区。
- 新增上游分支列表获取、默认值处理、远端解析和 worktree 创建相关接口与测试。
- 优化供应商同步逻辑，保留 rollout 文件 mtime，减少同步后不必要的会话状态变化。
- 新增独立的「工具与插件」页面，用于统一管理 Codex++ / Codex 的 MCP、skills、plugins，不再绑定到单个供应商。
- 切换供应商时会合并当前启用的工具与插件配置，同时避免把供应商专属配置误写入通用配置。
- 工具与插件列表改为从当前 Codex 配置实时读取启用状态，支持直接开关和删除条目。
- 调整通用配置提取逻辑，改为手动提取，减少自动覆盖和配置污染。
- 修复供应商切换隔离问题，避免 `model_catalog_json`、旧 `model_provider`、历史 provider 表和旧 `auth.json` 被带到新供应商。
- 修复纯 API 模式下 `auth.json` 没有写入 API Key 的问题，并固定供应商 provider 名称为 `CodexPlusPlus`。
- 优化模型目录写入方式，支持与原始模型目录合并，并在预览中显示真实路径。
- 供应商配置页新增模型插入方式、模型列表、上下文大小、压缩上下文大小、目标功能等配置项。
- 官方模式下隐藏仅混入 API Key 场景使用的模型列表和模型插入方式。
- 将 Base URL、API Key、上游协议移动到模型列表之前，测试模型和上下文选项收进「更多选项」。
- 修复 `model_reasoning_effort`、`plan_mode_reasoning_effort` 重复写入导致 TOML 解析失败的问题。
- 修复重复插件表、空配置体、布尔值解析等导致配置文件解析失败的问题。
- 优化供应商详情页布局，保持顶部返回和提示区域固定，增大默认窗口尺寸并减少顶部缝隙。
- 移除脚本安装时的 checksum 阻断，避免市场脚本校验不一致导致安装失败。
- 清理关于页和状态页中不需要展示的登录、当前供应商、配置文件路径等信息。
- 调整提示信息居中显示，避免遮挡重启按钮。
- 更新讨论群二维码、README 说明和 macOS DMG 打包脚本。
