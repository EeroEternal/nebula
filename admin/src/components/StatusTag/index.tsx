import type { ReactNode } from 'react';
import classNames from 'classnames';

type Tone = 'success' | 'warning' | 'error' | 'info' | 'neutral' | 'navy';

const TONE_CLASS: Record<Tone, string> = {
  success: 'bg-[var(--c-success-bg)] text-[var(--c-success)]',
  warning: 'bg-[var(--c-warning-bg)] text-[var(--c-warning)]',
  error: 'bg-[var(--c-error-bg)] text-[var(--c-error)]',
  info: 'bg-[var(--c-info-bg)] text-[var(--c-info)]',
  navy: 'bg-[var(--c-primary-light)] text-[var(--c-primary)]',
  neutral: 'bg-[var(--c-surface-2)] text-[color:var(--c-ink-2)]',
};

const DOT_CLASS: Record<Tone, string> = {
  success: 'bg-[var(--c-success)]',
  warning: 'bg-[var(--c-warning)]',
  error: 'bg-[var(--c-error)]',
  info: 'bg-[var(--c-info)]',
  navy: 'bg-[var(--c-primary)]',
  neutral: 'bg-[var(--c-ink-3)]',
};

type Props = {
  tone?: Tone;
  /** 状态 Tag 默认带圆点；类型/中性标签可关 */
  dot?: boolean;
  children: ReactNode;
  className?: string;
};

/** 设计规范 §5.4：浅底 + pill + 可选色点；色值只用 Token */
const StatusTag: React.FC<Props> = ({
  tone = 'neutral',
  dot = true,
  children,
  className,
}) => (
  <span
    className={classNames(
      'inline-flex items-center gap-1.5 rounded-pill px-2.5 py-[3px]',
      'text-[11px] font-semibold leading-[1.6] w-fit',
      TONE_CLASS[tone],
      className,
    )}
  >
    {dot ? (
      <span
        className={classNames(
          'inline-block h-1.5 w-1.5 shrink-0 rounded-full',
          DOT_CLASS[tone],
        )}
        aria-hidden
      />
    ) : null}
    {children}
  </span>
);

export default StatusTag;
export type { Tone as StatusTagTone };
