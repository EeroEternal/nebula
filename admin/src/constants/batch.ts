export const batchStatus = [
  {
    //0
    label: '已删除',
    value: 'deleted',
  },
  {
    //1
    label: '验证中',
    value: 'validating',
  },
  {
    //2
    label: '等待中',
    value: 'waiting',
  },
  {
    //3
    label: '运行中',
    value: 'running',
  },
  {
    //4
    label: '已完成',
    value: 'completed',
  },
  {
    //5
    label: '取消中',
    value: 'cancelling',
  },
  {
    //6
    label: '已取消',
    value: 'cancelled',
  },
  {
    //7
    label: '失败',
    value: 'failed',
  },
];

export enum BatchStatus {
  /** 已删除 */
  deleted = 0,
  /** 验证中 */
  validating = 1,
  /** 等待中 */
  waiting = 2,
  /** 运行中 */
  running = 3,
  /** 已完成 */
  completed = 4,
  /** 取消中 */
  cancelling = 5,
  /** 已取消 */
  canceled = 6,
  /** 失败 */
  failed = 7,
}

export const BATCH_STATUS_COLORS = {
  [BatchStatus.deleted]: 'default',
  [BatchStatus.validating]: 'processing',
  [BatchStatus.waiting]: 'processing',
  [BatchStatus.running]: 'processing',
  [BatchStatus.completed]: 'success',
  [BatchStatus.cancelling]: 'warning',
  [BatchStatus.canceled]: 'default',
  [BatchStatus.failed]: 'error',
};
