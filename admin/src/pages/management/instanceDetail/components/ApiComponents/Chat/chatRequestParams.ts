import type { ChatExtendParams } from './ExtendParamsModal';

const KW_MAX_TOKEN_KEYS = ['max_model_len', 'max-model-len', 'max_tokens'];

function positiveInt(v: unknown): number | undefined {
  const n = Number(v);
  if (!Number.isFinite(n) || n <= 0) {
    return undefined;
  }
  return Math.floor(n);
}

/** 运行上限优先实例 kwargs（如 vLLM max_model_len），再退目录 context_length。 */
export function resolveChatMaxTokens(detail: {
  context_length?: number;
  kwargs?: Record<string, unknown> | null;
}): number {
  const kw = detail.kwargs || {};
  for (const key of KW_MAX_TOKEN_KEYS) {
    const n = positiveInt(kw[key]);
    if (n) {
      return n;
    }
  }
  return positiveInt(detail.context_length) || 8192;
}

/** 对话请求只带有值的采样字段；0–1 的 top_k 滑条按 top_p 发给引擎。 */
export function buildChatRequestParams(
  params: ChatExtendParams,
  opts: { showThinking: boolean; maxTokensCap: number },
): Record<string, unknown> {
  const out: Record<string, unknown> = {};
  if (params.temperature != null) {
    out.temperature = params.temperature;
  }
  const maxTokens = params.max_tokens ?? Math.min(512, opts.maxTokensCap);
  if (maxTokens != null) {
    out.max_tokens = maxTokens;
  }
  if (params.top_k != null) {
    out.top_p = params.top_k;
  }
  if (opts.showThinking) {
    out.enable_thinking = params.enable_thinking ?? true;
  }
  if (params.lora_name) {
    out.lora_name = params.lora_name;
  }
  if (params.tools) {
    out.tools = params.tools;
  }
  return out;
}
