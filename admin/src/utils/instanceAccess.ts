import { IntanceStatus, ReplicaStatus } from '@/constants/intance';

type ReplicaLike = {
  replica_status?: string | null;
};

type InstanceLike = {
  status?: string | null;
  replica_data_source?: ReplicaLike[] | null;
};

const norm = (v?: string | null) => String(v || '').trim().toUpperCase();

/** 至少有一个 READY 副本（兼容大小写 / 枚举名） */
export const hasReadyReplica = (data?: InstanceLike | null): boolean =>
  (data?.replica_data_source || []).some((r) => norm(r?.replica_status) === ReplicaStatus.READY);

/**
 * 控制台统一：仅 READY 且存在 READY 副本时可对话。
 * 部署失败无副本时实例可存在，但不能对话。
 */
export const canChatInstance = (data?: InstanceLike | null): boolean =>
  norm(data?.status) === IntanceStatus.READY && hasReadyReplica(data);

/** 预热：实例 READY 即可 */
export const canWarmupInstance = (data?: InstanceLike | null): boolean =>
  norm(data?.status) === IntanceStatus.READY;

/** 详情请求 path（uid 可能含 `.` `@` 等，统一编码） */
export const instanceDetailPath = (modelUid: string) =>
  `/models/instances/${encodeURIComponent(modelUid)}`;
