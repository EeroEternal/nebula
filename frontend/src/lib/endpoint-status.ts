export type EndpointStatusTone = 'success' | 'warning' | 'destructive'

export function endpointStatusTone(status: string): EndpointStatusTone {
  const s = status.toLowerCase()
  if (s === 'ready') return 'success'
  if (s === 'unhealthy' || s === 'failed') return 'destructive'
  return 'warning'
}

export function isEngineAlertCritical(alertType: string): boolean {
  return alertType === 'oom_killed'
    || alertType === 'container_exited'
    || alertType === 'health_probe_failed'
}

export function engineAlertLabel(alertType: string, t: (key: string) => string): string {
  switch (alertType) {
    case 'oom_killed': return t('alerts.engine.oomKilled')
    case 'container_exited': return t('alerts.engine.containerExited')
    case 'gpu_memory_pressure': return t('alerts.engine.gpuMemoryPressure')
    case 'kv_cache_high': return t('alerts.engine.kvCacheHigh')
    case 'health_probe_failed': return t('alerts.engine.healthProbeFailed')
    default: return alertType
  }
}

/** Format node + GPU assignment for endpoint/placement rows. */
export function formatNodeGpu(
  nodeId: string,
  assignment?: { gpu_index?: number | null; gpu_indices?: number[] | null } | null,
): string {
  const indices = assignment?.gpu_indices?.length
    ? assignment.gpu_indices.join(',')
    : assignment?.gpu_index != null
      ? String(assignment.gpu_index)
      : null
  return indices ? `${nodeId} · GPU ${indices}` : nodeId
}
