import type { ReactNode } from 'react';

/**
 * 中文主指标名 + 右下角英文简称（小字）。
 * 不再使用悬浮 tip。
 */
const MetricLabel = ({
  zh,
  en,
  className,
}: {
  zh: ReactNode;
  en: string;
  className?: string;
}) => (
  <span
    className={
      className ||
      'inline-flex w-full min-w-[4.5rem] flex-col items-stretch gap-0.5 leading-tight'
    }
  >
    <span className="text-inherit">{zh}</span>
    <span className="self-end text-[10px] leading-none text-muted font-normal tracking-wide">
      {en}
    </span>
  </span>
);

export default MetricLabel;

/** 副本级 model_uid（如 qwen3.5-0）→ 逻辑实例名 */
export function normalizeInstanceModel(model: string): string {
  const s = String(model || '').trim();
  if (!s) return s;
  return s.replace(/-\d+$/, '');
}
