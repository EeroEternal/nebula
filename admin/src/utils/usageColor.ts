/**
 * GPU / 资源利用率进度条颜色（design.md §1.4 / §5.6）
 * < 80% primary · 80–95% warning · > 95% error
 */
export type UsageLevel = 'normal' | 'warning' | 'critical';

export function getUsageLevel(percent: number): UsageLevel {
  if (percent > 95) return 'critical';
  if (percent >= 80) return 'warning';
  return 'normal';
}

export function getUsageStrokeColor(percent: number): string {
  const level = getUsageLevel(percent);
  if (level === 'critical') return 'var(--c-error)';
  if (level === 'warning') return 'var(--c-warning)';
  return 'var(--c-primary)';
}

/** 严重告警时给 Progress 附加脉冲类名 */
export function getUsageProgressClassName(percent: number): string {
  return getUsageLevel(percent) === 'critical' ? 'progress-critical' : '';
}
