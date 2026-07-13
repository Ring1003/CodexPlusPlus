/**
 * CodexPlusPlus 供应商预设
 * 仅保留国内主流大厂官方 Coding Plan + OpenAI 官方。
 */

export type PresetCategory = "official" | "aggregator" | "third_party" | "cn_official";

export type RelayProtocol = "responses" | "chatCompletions";

export interface ProviderPreset {
  id: string;
  name: string;
  websiteUrl?: string;
  apiKeyUrl?: string;
  category: PresetCategory;
  baseUrl: string;
  protocol: RelayProtocol;
  model: string;
  modelList?: string[];
}

/**
 * 预设列表。选择任一预设会自动填充：
 * - name     → 供应商名称
 * - baseUrl  → API 端点
 * - protocol → responses / chatCompletions（根据上游实际协议）
 * - model    → 默认模型名
 * - modelList → 可选模型清单（换行分隔）
 */
export const PRESETS: ProviderPreset[] = [
  // ── 官方 ──
  {
    id: "openai",
    name: "OpenAI Official",
    category: "official",
    baseUrl: "https://api.openai.com/v1",
    protocol: "responses",
    model: "gpt-5.5",
    websiteUrl: "https://chatgpt.com/codex",
  },

  // ── 中国官方 ──
  {
    id: "deepseek",
    name: "DeepSeek",
    websiteUrl: "https://platform.deepseek.com",
    apiKeyUrl: "https://platform.deepseek.com/api_keys",
    category: "cn_official",
    baseUrl: "https://api.deepseek.com",
    protocol: "chatCompletions",
    model: "deepseek-v4-flash",
    modelList: ["deepseek-v4-flash", "deepseek-v4-pro"],
  },
  {
    id: "zhipu-glm",
    name: "Zhipu GLM",
    websiteUrl: "https://open.bigmodel.cn",
    apiKeyUrl: "https://www.bigmodel.cn/glm-coding",
    category: "cn_official",
    baseUrl: "https://open.bigmodel.cn/api/coding/paas/v4",
    protocol: "chatCompletions",
    model: "glm-5.2",
    modelList: ["glm-5.2", "glm-5", "glm-4.7", "glm-4.6"],
  },
  {
    id: "kimi",
    name: "Kimi Code",
    websiteUrl: "https://www.kimi.com/code/docs/",
    apiKeyUrl: "https://www.kimi.com/code/console/api-keys",
    category: "cn_official",
    // Kimi Code 会员权益（Coding Plan）专用端点，与开放平台按量计费的 api.moonshot.cn 不同
    baseUrl: "https://api.kimi.com/coding/v1",
    protocol: "chatCompletions",
    // 模型 ID 固定为 kimi-for-coding，后端随官方旗舰模型自动升级
    model: "kimi-for-coding",
    modelList: ["kimi-for-coding", "kimi-for-coding-highspeed"],
  },

  // ── 自建/中转（OpenAI 兼容接口） ──
  {
    id: "custom-openai-compatible",
    name: "OpenAI 兼容接口",
    category: "aggregator",
    baseUrl: "https://your-endpoint.com/v1",
    protocol: "chatCompletions",
    model: "gpt-5.5",
    modelList: ["gpt-5.5"],
    websiteUrl: "",
    apiKeyUrl: "",
  },
];
