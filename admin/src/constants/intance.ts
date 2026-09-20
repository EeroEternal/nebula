import type { BadgeProps, TagProps } from 'antd';

export enum IntanceStatus {
  /** 正在启动 */
  CREATING = 'CREATING',
  /** 正在更新 */
  UPDATING = 'UPDATING',
  /** 就绪 */
  READY = 'READY',
  /** 错误 */
  ERROR = 'ERROR',
  /** 正在删除 */
  TERMINATING = 'TERMINATING',
  /** 正在恢复 */
  RECOVERING = 'RECOVERING',
  /** 下线 */
  OFFLINE = 'OFFLINE',
  /** 挂起 */
  SUSPEND = 'SUSPEND',
}
/** 实例状态badge颜色 */
export const INTANCE_STATUS_COLOR: Record<string, BadgeProps['status']> = {
  [IntanceStatus.CREATING]: 'processing',
  [IntanceStatus.UPDATING]: 'processing',
  [IntanceStatus.READY]: 'success',
  [IntanceStatus.ERROR]: 'error',
  [IntanceStatus.TERMINATING]: 'error',
  [IntanceStatus.RECOVERING]: 'processing',
  [IntanceStatus.OFFLINE]: 'default',
  [IntanceStatus.SUSPEND]: 'processing',
};

/** Tag color：与 Badge status 对齐，避免 Tag 收到非法 color 时不渲染文案 */
export const INTANCE_STATUS_TAG_COLOR: Record<string, TagProps['color']> = {
  [IntanceStatus.CREATING]: 'processing',
  [IntanceStatus.UPDATING]: 'processing',
  [IntanceStatus.READY]: 'success',
  [IntanceStatus.ERROR]: 'error',
  [IntanceStatus.TERMINATING]: 'error',
  [IntanceStatus.RECOVERING]: 'processing',
  [IntanceStatus.OFFLINE]: 'default',
  [IntanceStatus.SUSPEND]: 'processing',
};

export enum ReplicaStatus {
  /** 正在初始化 */
  INITIALIZING = 'INITIALIZING',
  /** 就绪 */
  READY = 'READY',
  /** 正在重新恢复 */
  RECOVERING = 'RECOVERING',
  /** 下线 */
  OFFLINE = 'OFFLINE',
  /** 失败 */
  FAILED = 'FAILED',
  /** 未知 / 后端未回填 */
  UNKNOWN = 'UNKNOWN',
}
/** 副本状态badge颜色 */
export const REPLICA_STATUS_COLOR: Record<string, BadgeProps['status']> = {
  [ReplicaStatus.INITIALIZING]: 'processing',
  [ReplicaStatus.READY]: 'success',
  [ReplicaStatus.RECOVERING]: 'warning',
  [ReplicaStatus.OFFLINE]: 'default',
  [ReplicaStatus.FAILED]: 'error',
  [ReplicaStatus.UNKNOWN]: 'default',
  // 实例 ERROR 时偶发带上来的别名
  ERROR: 'error',
};

/** 规范化副本状态文案 key：null/空 → UNKNOWN；实例 ERROR 时按 FAILED */
export const normalizeReplicaStatus = (
  status?: string | null,
  instanceStatus?: string | null,
): string => {
  const raw = status == null ? '' : String(status).trim();
  if (!raw || raw === 'null' || raw === 'undefined') {
    return instanceStatus === 'ERROR' || instanceStatus === IntanceStatus.ERROR
      ? ReplicaStatus.FAILED
      : ReplicaStatus.UNKNOWN;
  }
  if (raw === 'ERROR') return ReplicaStatus.FAILED;
  return raw;
};

/** 与 ModelType 取值对齐；刻意不引用 modelData，避免与 intl 形成循环依赖 */
export const INTANCE_MODEL_TYPE: Record<string, TagProps['color']> = {
  LLM: 'blue',
  embedding: 'cyan',
  image: 'blue',
  rerank: 'gold',
  audio: 'processing',
  video: 'red',
};
