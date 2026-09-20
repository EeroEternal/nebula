import {
  imageMatches,
  localImageBelongsToEngine,
  parseImageTag,
} from '@/components/DeployModelInstance/engineVersionOptions';
import type {
  EngineImageSpec,
  EngineNodeImages,
  EngineNodeInfo,
  EnginePullProgress,
  EngineRuntimeKind,
} from '@/services/engineImages';

export const SUPERVISOR_NODE_ID = 'supervisor';
export const VIEW_CATALOG = 'catalog';
export const VIEW_REMOTE = 'remote';

/** 引擎展示/筛选固定顺序（模型仓库部署下拉同此） */
export const ENGINE_DISPLAY_ORDER = [
  'vllm',
  'vllm-ascend',
  'transformers',
  'mindie',
  'sglang',
  'lmdeploy',
  'llama.cpp',
  'mlx',
  'tei',
  'whisper',
] as const;

/**
 * 无 /engines 内置规格时（audio/image/video 等），按模态优先展示的引擎目录键。
 * 与 backend engine catalog / engine-gap 对齐。
 */
export const PREFERRED_CATALOG_ENGINES_BY_TYPE: Record<string, string[]> = {
  audio: ['whisper'],
  image: [],
  video: [],
  embedding: ['tei', 'vllm', 'transformers', 'sglang', 'lmdeploy'],
  rerank: ['tei', 'vllm', 'transformers'],
};

export const engineSortRank = (engine: string) => {
  const key = (engine || '').toLowerCase();
  const idx = (ENGINE_DISPLAY_ORDER as readonly string[]).indexOf(key);
  return idx === -1 ? 1000 : idx;
};

export const compareEngines = (a: string, b: string) => {
  const d = engineSortRank(a) - engineSortRank(b);
  if (d !== 0) return d;
  return (a || '').localeCompare(b || '');
};

export const updatedAtMs = (raw?: string | null) => {
  if (!raw) return 0;
  const t = Date.parse(raw);
  return Number.isNaN(t) ? 0 : t;
};

export const engineTagTone = (
  engine: string,
): 'navy' | 'neutral' => {
  const key = (engine || '').toLowerCase();
  // 引擎类型用品牌中性 Tag，不用 success/warning/info 语义色做区分
  if (
    key === 'vllm' ||
    key === 'vllm-ascend' ||
    key === 'sglang' ||
    key === 'mindie' ||
    key === 'lmdeploy' ||
    key === 'transformers' ||
    key === 'llama.cpp' ||
    key === 'llamacpp' ||
    key === 'mlx' ||
    key === 'tei' ||
    key === 'whisper' ||
    key === 'diffusers'
  ) {
    return 'navy';
  }
  return 'neutral';
};

/** @deprecated Prefer StatusTag + engineTagTone — Ant color names kept for legacy */
export const engineTagColor = (engine: string) => {
  const tone = engineTagTone(engine);
  if (tone === 'navy') return 'blue';
  return 'default';
};

export const engineTagLabel = (engine: string) => {
  const key = (engine || '').toLowerCase();
  if (key === 'vllm') return 'vLLM';
  if (key === 'vllm-ascend') return 'vLLM-Ascend';
  if (key === 'sglang') return 'SGLang';
  if (key === 'mindie') return 'MindIE';
  if (key === 'lmdeploy') return 'LMDeploy';
  if (key === 'transformers') return 'Transformers';
  if (key === 'llama.cpp' || key === 'llamacpp') return 'llama.cpp';
  if (key === 'mlx') return 'MLX';
  if (key === 'tei') return 'TEI';
  if (key === 'whisper') return 'Whisper';
  if (key === 'diffusers') return 'Diffusers';
  return engine;
};

/**
 * 容器镜像引擎预设（点选即填充引擎名 + 默认仓库）。
 * 与 LLM 引擎对齐；本地型引擎请用「自定义」。
 */
export const BUILTIN_ENGINE_PRESETS: {
  engine: string;
  repo: string;
  label: string;
}[] = [
  { engine: 'vllm', repo: 'vllm/vllm-openai', label: 'vLLM' },
  { engine: 'vllm-ascend', repo: 'quay.io/ascend/vllm-ascend', label: 'vLLM-Ascend' },
  { engine: 'transformers', repo: 'ghcr.io/huggingface/text-generation-inference', label: 'Transformers' },
  { engine: 'mindie', repo: 'ascendhub/mindie', label: 'MindIE' },
  { engine: 'sglang', repo: 'lmsysorg/sglang', label: 'SGLang' },
  { engine: 'lmdeploy', repo: 'openmmlab/lmdeploy', label: 'LMDeploy' },
  { engine: 'llama.cpp', repo: 'ghcr.io/ggerganov/llama.cpp', label: 'llama.cpp' },
  { engine: 'mlx', repo: 'powerllm/mlx-lm-server', label: 'MLX' },
  { engine: 'tei', repo: 'ghcr.io/huggingface/text-embeddings-inference', label: 'TEI' },
  { engine: 'whisper', repo: 'fedirz/faster-whisper-server', label: 'Whisper' },
];

export const formatSize = (bytes?: number | null) => {
  if (bytes == null || Number.isNaN(bytes)) return '—';
  if (bytes < 1024) return `${bytes} B`;
  if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`;
  if (bytes < 1024 * 1024 * 1024)
    return `${(bytes / (1024 * 1024)).toFixed(1)} MB`;
  return `${(bytes / (1024 * 1024 * 1024)).toFixed(2)} GB`;
};

export const statusColor = (status?: string) => {
  switch (status) {
    case 'succeeded':
      return 'success';
    case 'running':
    case 'pending':
      return 'processing';
    case 'failed':
      return 'error';
    case 'cancelled':
      return 'default';
    default:
      return 'default';
  }
};

export const nodeLabel = (node: EngineNodeInfo | EngineNodeImages) => {
  if ('label' in node && node.label) {
    const label = node.label;
    if (
      label === 'Supervisor' ||
      label === SUPERVISOR_NODE_ID ||
      label.toLowerCase() === 'supervisor'
    ) {
      return '';
    }
    if (label.startsWith('127.0.0.1:') || label.startsWith('0.0.0.0:')) {
      return label.replace(/^0\.0\.0\.0/, '127.0.0.1');
    }
    return label;
  }
  if (node.role === 'supervisor') return '';
  return node.ip_address || node.node_id;
};

export const roleLabelKey = (role?: string) =>
  role === 'supervisor'
    ? 'models.engines.roleSupervisor'
    : 'models.engines.roleWorker';

/** 节点展示统一用 supervisor / worker 标识 */
export const displayNodeName = (
  node: EngineNodeInfo | EngineNodeImages,
  _lGet?: (id: string, fallback?: string) => string,
) => {
  const role = node.role === 'supervisor' ? 'supervisor' : 'worker';
  const name = nodeLabel(node);
  return name ? `${role} · ${name}` : role;
};

export const imagesOnNode = (
  nodes: EngineNodeImages[],
  nodeId: string,
  flatFallback: string[] = [],
): Set<string> => {
  const node = nodes.find((n) => n.node_id === nodeId);
  if (node) return new Set(node.images || []);
  if (nodeId === SUPERVISOR_NODE_ID) {
    const sup = nodes.find(
      (n) => n.node_id === SUPERVISOR_NODE_ID || n.role === 'supervisor',
    );
    if (sup) return new Set(sup.images || []);
  }
  return new Set(flatFallback);
};

/** Whether a node inventory contains image (loose ref match). */
export const nodeInventoryHasImage = (
  nodes: EngineNodeImages[],
  nodeId: string,
  image: string,
  flatFallback: string[] = [],
): boolean => {
  if (!image) return false;
  const set = imagesOnNode(nodes, nodeId, flatFallback);
  return Array.from(set).some((img) => imageMatches(img, image));
};

/** Loose presence check (docker.io/ / private-mirror aliases). */
export const nodeHasImage = (
  node: EngineNodeImages | undefined,
  image: string,
): boolean => {
  if (!node || node.error || !image) return false;
  return (node.images || []).some((img) => imageMatches(img, image));
};

export const anyNodeHasImage = (
  nodes: EngineNodeImages[],
  image: string,
): boolean => nodes.some((n) => nodeHasImage(n, image));

/** 目标状态与下载/删除/迁移同一套节点（含 Supervisor；排除探活失败的节点） */
export const coverageForImage = (
  nodes: EngineNodeImages[],
  image: string,
): { ready: number; total: number } => {
  const targets = nodes.filter((n) => !n.error);
  const total = targets.length;
  const ready = targets.filter((n) => nodeHasImage(n, image)).length;
  return { ready, total };
};

/**
 * Infer engine key for a local image via registry map / catalog short name.
 * Prefer registry keys that belong to the image; fall back to catalog engines.
 */
export const inferEngineForLocalImage = (
  image: string,
  catalog: EngineImageSpec[],
  registryRepos?: Record<string, string>,
): string | null => {
  const repos = registryRepos || {};
  // 长 key 优先，避免 vllm-ascend 被当成 vllm
  const keys = Object.keys(repos).sort((a, b) => {
    const ld = b.length - a.length;
    if (ld !== 0) return ld;
    return compareEngines(a, b);
  });
  for (const engine of keys) {
    if (localImageBelongsToEngine(image, engine, catalog, repos)) {
      return engine.toLowerCase();
    }
  }
  const fromCatalog = new Set(
    (catalog || []).map((c) => (c.engine || '').toLowerCase()).filter(Boolean),
  );
  for (const engine of Array.from(fromCatalog).sort(compareEngines)) {
    if (localImageBelongsToEngine(image, engine, catalog, repos)) {
      return engine;
    }
  }
  return null;
};

export const localImageVersion = (image: string) => parseImageTag(image);

export type PullDraft = {
  engine: string;
  version: string;
  image: string;
  description?: string;
  fromRemote?: boolean;
};

export type MigrateDraft = {
  image: string;
  source: string;
};

export const taskBusyKey = (t: Pick<EnginePullProgress, 'engine' | 'version' | 'image' | 'target'>) =>
  `${(t.engine || '').toLowerCase()}@${t.version || ''}@${t.image || ''}@${t.target || ''}`;

export const runtimeOptions: { label: string; value: EngineRuntimeKind }[] = [
  { label: 'Docker', value: 'docker' },
  { label: 'Kubernetes', value: 'k8s' },
];

export const formatBytes = (n?: number | null): string => {
  const v = Number(n || 0);
  if (!Number.isFinite(v) || v <= 0) return '0 B';
  const units = ['B', 'KB', 'MB', 'GB', 'TB'];
  let x = v;
  let i = 0;
  while (x >= 1024 && i < units.length - 1) {
    x /= 1024;
    i += 1;
  }
  return `${x < 10 && i > 0 ? x.toFixed(1) : Math.round(x)} ${units[i]}`;
};

export const catalogRowKey = (row: EngineImageSpec) =>
  `${row.engine}@${row.version}`;
