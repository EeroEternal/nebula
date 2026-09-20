import { ModelAbility } from '@/constants/modelData';
import { IntanceStatus } from '@/constants/intance';

export const THEME = {
  NAV_THEME: 'navTheme',
  dark: 'realDark',
  light: 'light',
};

/**
 * MonacoEditor of CodeEdit's theme
 */
export const CODE_EDIT_THEME = {
  LIGHT: 'light',
  DARK: 'vs-dark',
};

export const MonacoEditorOptions = {
  // selectOnLineNumbers: true,
  renderSideBySide: false, //  side by side
  autoIndent: 'None', //  auto indent
  fontSize: 14, //  font size
  automaticLayout: true, //  auto layout
  scrollBeyondLastLine: false, //is scroll beyond the last line
  autoDetectHighContrast: true, // auto detect high contrast
};

export type BaseBeanColumns = ExcludeNameAndEnableColumns & {
  name: string;
  enabled: boolean;
};

export type ExcludeNameAndEnableColumns = {
  id: number;
  createTime: Date;
  updateTime: Date;
};

export type SuggestionLabel = {
  label: string;
  detail?: string;
  description?: string;
};

export type SuggestionInfo = {
  key: string | number;
  label: SuggestionLabel;
  kind: number;
  insertText: string;
  detail?: string;
};

export interface Cpu {
  memory_available: number;
  memory_total: number;
  memory_used: number;
  total: number;
  usage: number;
}
export interface Gpus {
  [property: string]: {
    mem_free: number;
    mem_total: number;
    mem_used: number;
    mem_usage: number;
    /** 核心利用率：可能是 0–1 或 0–100（与采集源有关） */
    gpu_util?: number;
    /** 摄氏度；未采集为 undefined/null */
    temperature_c?: number | null;
    /** 瓦；未采集为 undefined/null */
    power_w?: number | null;
    name: string;
    status: 'online' | 'offline' | 'expired';
  };
}
/**
 * 集群信息
 */
export interface DeviceInfo {
  cpu: Cpu;
  gpu_count: number;
  gpus: Gpus;
  name: string;
  status: 'online' | 'offline' | 'expired';
  uuid: string;
  worker_address: string;
  accelerator_kind?: string;
  machine_model?: string;
}
/** 概览 */
export interface OverviewResponse {
  batch_list: {
    batch_id: number;
    process: number;
  }[];
  count_traces_total: number;
  cpu_used_rate: number;
  data_source: {
    model: string;
    count_traces: number;
  }[];
  gpu_used_rate: number;
  instance_count: number;
  task_list: {
    status: string;
    task_name: string;
  }[];
  tokens_usage: {
    date: string;
    model: string;
    total_usage: number;
  }[];
  device_info: DeviceInfo[];
  instance_source: ModelsInstancesListItem[];
}

/** 要同步的work_list */
export interface SyncWorkerSource {
  worker_address: string;
  models: {
    model_uid: string;
    model_name: string;
    model_type: string;
    model_version: string;
    replica: number;
    status: 'CREATING' | 'UPDATING' | 'TERMINATING' | 'READY' | 'ERROR';
    gpu_idx: string[];
  }[];
}

/** 节点信息 */
export interface NodeInfoSource {
  index: number;
  /**
   * GPU数量
   */
  gpu_count: number;
  /**
   * GPU类型
   */
  gpu_type: string;
  /**
   * vRAM(Total)
   */
  gpu_vram_total: number;
  /**
   * worker_id，下线，同步等用此字段
   */
  id: string;
  /**
   * IP
   */
  ip_address: string;
  /**
   * 名称
   */
  node_name: string;
  /**
   * 类型
   */
  node_type: string;
  /**
   * worker状态
   */
  worker_status: string;
  /**
   * 主从状态
   */
  is_primary?: boolean;
}

export interface ModelsInstancesListItem {
  error_info?: string;
  gpu_idx?: number[];
  instance_created_ts?: number;
  is_builtin?: boolean;
  model_ability: string[];
  model_engine?: string;
  model_name: string;
  model_type: string;
  model_uid: string;
  model_version: string;
  n_gpu?: string;
  peft_model_config?: {
    lora_list?: {
      lora_name: string;
      local_path: string;
    }[];
  };
  replica: number;
  status: string;
  /** 推理预热状态（与 catalog 权重预热无关） */
  warmup_status?: 'idle' | 'warming' | 'ready' | 'failed' | string;
  warmup_error?: string | null;
  warmup_started_at?: number | null;
  warmup_finished_at?: number | null;
  /** 启动参数（含 engine_version / engine_image） */
  kwargs?: Record<string, unknown>;
  /**
   * 副本信息
   */
  replica_data_source?: ReplicaDataSource[];
  /** 部署配置（含多 worker devices） */
  replica_config?: ReplicaConfigItem[];
  /** 多机联合部署时的 worker 数（PP nnodes） */
  n_worker?: number;
}

export interface ReplicaShardDevice {
  worker_address: string;
  gpu_idx: number[];
  shard?: number | null;
}

export interface ReplicaDataSource {
  gpu_idx: number[];
  replica_model_uid: string;
  worker_address: string;
  replica_status: string;
  /** 同副本多 worker/shard 放置（PP）；优先于单字段 worker_address */
  devices?: ReplicaShardDevice[];
}

interface MessageFileType {
  data: string;
  expires_at: number;
  id: string;
  transcript: string;
}
export interface ChatChoicesMessage {
  content: string;
  role: string;
  audio?: MessageFileType;
  image?: MessageFileType;
  video?: MessageFileType;
  reasoning_content?: string | null;
  reasoning?: string | null;
}
export interface ChatStreamResult {
  created: number;
  id: string;
  model: string;
  object: string;
  choices: {
    index: number;
    finish_reason: string;
    delta: {
      content: string;
      reasoning_content?: string | null;
      reasoning?: string | null;
    };
    message?: ChatChoicesMessage;
  }[];
  usage: {
    completion_tokens: number;
    prompt_tokens: number;
    total_tokens: number;
  };
}
/** 模型版本列表 */
export interface ModelVersionTableListItem {
  model_version: string;
  model_file_location: Record<string, string>;
  cache_status: boolean;
  quantization: string;
  model_format: string;
  model_size_in_billions: string;
  /** 支持该版本的部署引擎列表 */
  model_engine?: string | string[];
}
/** 模型虚拟环境列表 */
export interface ModelVirtualEnvsItem {
  actor_ip_address: string;
  model_engine: string;
  model_name: string;
  path: string;
  python_version: string;
  real_path: string;
}
/** 模型引擎 */
export type ModelEngines = Record<
  string,
  {
    model_name: string;
    model_format?: string;
    model_size_in_billions?: number;
    quantizations?: string[];
    cache_list?: string[];
    dimensions?: number;
    max_tokens?: number;
    cache_status?: boolean;
  }[]
>;

/** 非LLM模型的版本列表 */
export interface ModelVersionListItem {
  cache_status: boolean;
  controlnet: string;
  model_file_location: unknown;
  model_version: string;
}
/** model data */
export interface ModelData {
  version: number;
  context_length: number;
  model_name: string;
  model_lang: string[];
  model_ability: string[];
  model_description: string;
  model_family: string | null;
  model_specs: {
    model_format: string;
    model_size_in_billions: number;
    quantizations: string[];
    model_id: string;
    model_hub: string;
    model_uri: string | null;
    model_revision: string;
    cache_status: boolean;
  }[];
  chat_template: string;
  stop_token_ids: number[];
  stop: string[];
  reasoning_start_tag: string;
  reasoning_end_tag: string;
  language?: string[];
  max_tokens?: number;
  dimensions?: number;
  updated_at: number;
  gguf_quantizations?: string[];
  lightning_versions?: string[];
  is_enterprise: boolean;
  readme: string;
  exist_venv?: boolean;
}
/** 模型仓库列表item */
export interface ModelRepositoryListItem extends ModelData {
  is_builtin: boolean;
  model_version_count: number;
  model_instance_count: number;
  cache_status?: boolean;
  /** 自定义仓聚合列表带上的类型；内置仓由路由 tab 决定 */
  model_type?: string;
  exist_cache?: boolean;
  last_modified?: number;
}
/** 模型详情 */
export interface ModelDetailRes {
  model_data: ModelData;
  download_hubs: string[];
  is_builtin: boolean;
  model_instance_count: number;
  model_version_count: number;
  model_specs: {
    cache_status: number;
    model_format: string;
    model_hub: string;
    model_id: string;
  }[];
  cache_status?: boolean;
}
/** input/output Tokens */
export interface ModelUsageTokens {
  metric: {
    __name__: string;
    format: string;
    instance: string;
    job: string;
    model: string;
    node: string;
    quantization: string;
    type: string;
  };
  values: [number, string][];
}

/** 操作日志 */
export interface OperationsLogItem {
  resourceId: string;
  module: string;
  opType: string;
  operator: string;
  opTime: number;
  ipAddress: string;
}

/** 服务日志 */
export interface ServiceLog {
  logs: string[];
  last_offset: number;
}

export type ValueType = {
  key: string;
  value: string;
  /** 系统自动添加的附加参数（UI 置顶 + 星标；提交时不传给后端） */
  system?: boolean;
};

export interface ReplicaConfigItem {
  replica_uid: string;
  devices: {
    worker_ip: string;
    n_gpu: string;
    gpu_idx: number[];
    model_path: string;
    gpu_mem_gb?: number;
    gpu_mem_mib?: number;
    gpu_cores?: number;
    gpu_type?: string;
  }[];
  status: string;
}
export interface InstanceDetail {
  model_name: string;
  model_type: string;
  model_uid: string;
  context_length?: number;
  model_engine: string;
  model_version: string;
  model_ability: ModelAbility[];
  replica: number;
  status: IntanceStatus;
  instance_created_ts: number;
  gpu_idx: number[];
  is_builtin: boolean;
  error_info: string | null;
  replica_data_source: ReplicaDataSource[];
  replica_config: ReplicaConfigItem[];
  n_worker: number;
  kwargs: Record<string, unknown>;
  /** 与 GET /models/instances 列表同源字段 */
  warmup_status?: 'idle' | 'warming' | 'ready' | 'failed' | string;
  warmup_error?: string | null;
  warmup_started_at?: number | null;
  warmup_finished_at?: number | null;
  peft_model_config?: {
    lora_list?: {
      lora_name: string;
      local_path: string;
    }[];
    image_lora_load_kwargs?: Record<string, unknown>;
    image_lora_fuse_kwargs?: Record<string, unknown>;
  };
}

export interface RoleListItem {
  role: string;
  permissions: {
    page: Record<string, string[]>;
    action: Record<string, string[]>;
  };
  update_ts: number;
}

export interface ModelsEventItem {
  event_type: string;
  event_ts: number;
  event_content: string;
}

export interface FinetuneListItem {
  task_id: number;
  task_name: string;
  task_status: string;
  create_ts: number;
  start_ts: number;
  finish_ts: number;
  worker_ip: string;
  model_config: {
    model_name: string;
    model_version: string;
    model_type: string;
    model_path: string;
    quantization_bit: string;
    rope_scaling: string;
    booster: string;
    visual_inputs: boolean;
    resize_vocab: boolean;
    upcast_layernorm: boolean;
    shift_attn: boolean;
  };
  data_config: {
    dataset: string[];
    template: string;
    dataset_dir: string;
    max_samples: number;
    cutoff_len: number;
    val_size: number;
    packing: boolean;
  };
  output_config: {
    logging_steps: number;
    save_steps: number;
    warmup_steps: number;
    neftune_alpha: number;
    optim: string;
    output_dir: string;
    report_to: boolean;
  };
  train_config: {
    gpu_ids: number[];
    learning_rate: number;
    num_train_epochs: number;
    max_grad_norm: number;
    compute_type: string;
    batch_size: number;
    gradient_accumulation_steps: number;
    lr_scheduler_type: string;
    ds_enable: boolean;
    ds_stage: number;
    ds_offload: boolean;
  };
  tuning_config: {
    training_stage: string;
    finetuning_type: string;
    use_llama_pro: boolean;
  };
  lora_config: {
    lora_rank: number;
    lora_alpha: number;
    lora_dropout: number;
    loraplus_lr_ratio: number;
    lora_target: string;
    additional_target: string;
    create_new_adapter: boolean;
    use_rslora: boolean;
    use_dora: boolean;
  };
  galore_config: {
    use_galore: boolean;
    galore_rank: number;
    galore_update_interval: number;
    galore_scale: number;
    galore_target: string;
  };
  badam_config: {
    use_badam: boolean;
    badam_mode: string;
    badam_switch_mode: string;
    badam_switch_interval: number;
    badam_update_ratio: number;
  };
}
export interface BatchListItem {
  id: number;
  endpoint: string;
  input_file_id: string;
  input_file_url: string;
  completion_window: string;
  status: number;
  output_file_id: string;
  output_file_url: string;
  error_file_id: null;
  error_file_url: null;
  request_total: number;
  request_completed: number;
  request_failed: number;
  created_at: string | null;
  in_progress_at: string;
  expires_at: string | null;
  finalizing_at: string | null;
  completed_at: string | null;
  failed_at: string | null;
  expired_at: string | null;
  cancelled_at: string | null;
  extra: null;
}
