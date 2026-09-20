export const taskStatus = [
  {
    label: '待处理',
    value: 'pending',
    desc: '任务已创建，等待调度开始执行',
  },
  {
    label: '已调度',
    value: 'scheduled',
    desc: '任务已被调度，分配了资源，准备开始执行',
  },
  {
    label: '运行中',
    value: 'running',
    desc: '任务正在执行',
  },
  {
    label: '重试中',
    value: 'retrying',
    desc: '任务在失败后自动重试',
  },
  {
    label: '已取消',
    value: 'cancelled',
    desc: '任务被取消，停止执行',
  },
  {
    label: '已完成',
    value: 'finished',
    desc: '任务成功执行完毕',
  },
  {
    label: '失败',
    value: 'failed',
    desc: '任务执行失败，遇到错误或异常',
  },
];
export const TASK_STATUS_COLORS = {
  pending: 'processing',
  scheduled: 'processing',
  running: 'processing',
  retrying: 'warning',
  cancelled: 'error',
  finished: 'success',
  failed: 'error',
};
// 删除
export const taskStatusConfig = {
  pending: {
    color: 'processing',
  },
  scheduled: {
    color: 'processing',
  },
  running: {
    color: 'processing',
  },
  retrying: {
    color: 'warning',
  },
  cancelled: {
    color: 'error',
  },
  finished: {
    color: 'success',
  },
  failed: {
    color: 'error',
  },
};
/** 挂载路径 */
export const DATASET_DIR = '/opt/projects/LLaMA-Factory/data';
/** 量化等级 */
export const QUANTIZATION_BIT = {
  none: 'none',
  '4': '4',
  '8': '8',
};
/** RoPE 插值方法 */
export const ROPE_SCALING = {
  none: 'none',
  linear: 'linear',
  dynamic: 'dynamic',
};
/** 加速方式 */
export const BOOSTER = {
  none: 'none',
  flashattn2: 'flashattn2',
  unsloth: 'unsloth',
};
/** 训练阶段 */
export const TRAINING_STAGE = {
  sft: 'sft',
  pt: 'pt',
  rm: 'rm',
  ppo: 'ppo',
  dpo: 'dpo',
  kto: 'kto',
};
/** 微调方法 */
export const FINETUNING_TYPE = {
  lora: 'lora',
  freeze: 'freeze',
  full: 'full',
};
/** BAdam 模式 */
export const BADAM_MODE = {
  layer: 'layer',
  ratio: 'ratio',
};
/** BAdam 切换策略 */
export const BADAM_SWITCH_MODE = {
  ascending: 'ascending',
  descending: 'descending',
  random: 'random',
  fixed: 'fixed',
};
/** 计算类型 */
export const COMPUTE_TYPE = {
  fp16: 'fp16',
  bf16: 'bf16',
  fp32: 'fp32',
  pure_bf16: 'pure_bf16',
};
/** y优化器 */
export const OPTIM = {
  adamw_torch: 'adamw_torch',
  adamw_8bit: 'adamw_8bit',
  adafactor: 'adafactor',
};

/** Zero Stage */
export const ZERO_STAGE_OPTIONS = [
  { label: '2', value: 2 },
  { label: '3', value: 3 },
];
