import classNames from 'classnames';
import { FC, ReactNode } from 'react';

export type PillTabOption = {
  label: ReactNode;
  value: string | number;
  disabled?: boolean;
  /** 可点击但文案灰色（如实例未就绪时的「模型日志」） */
  muted?: boolean;
};

type Props = {
  options: PillTabOption[];
  value?: string | number;
  onChange?: (value: string | number) => void;
  className?: string;
  /** 占满宽度均分（对齐 Ant Segmented block） */
  block?: boolean;
  /**
   * pill：灰底槽 + 白底选中（列表类型切换）
   * underline：底部分割线 + 主色下划线（详情子页，贴合页面底色）
   */
  variant?: 'pill' | 'underline';
  'aria-label'?: string;
};

/** 页面级 Tab：pill / underline 两套，均走 design token */
const PillTabs: FC<Props> = ({
  options,
  value,
  onChange,
  className,
  block = false,
  variant = 'pill',
  'aria-label': ariaLabel,
}) => {
  const isUnderline = variant === 'underline';
  return (
    <div
      className={classNames(
        'inline-flex min-w-min',
        isUnderline
          ? 'gap-1 border-b border-[color:var(--c-border-light)]'
          : 'gap-0.5 rounded-md bg-[var(--c-surface-2)] p-[3px]',
        block && 'w-full',
        className,
      )}
      role="tablist"
      aria-label={ariaLabel}
    >
      {options.map((opt) => {
        const active = value === opt.value;
        return (
          <button
            key={String(opt.value)}
            type="button"
            role="tab"
            aria-selected={active}
            disabled={opt.disabled}
            className={classNames(
              'shrink-0 text-[13px] font-semibold cursor-pointer transition-colors disabled:opacity-45 disabled:cursor-not-allowed',
              block && 'flex-1 text-center',
              isUnderline
                ? classNames(
                    '-mb-px border-0 border-b-2 bg-transparent px-3 py-2.5 rounded-none',
                    active
                      ? opt.muted && !opt.disabled
                        ? 'border-[color:var(--c-ink-3)] text-[color:var(--c-ink-3)]'
                        : 'border-[var(--c-primary)] text-[var(--c-primary)]'
                      : opt.muted && !opt.disabled
                        ? 'border-transparent text-[color:var(--c-ink-3)] hover:text-[color:var(--c-ink-2)]'
                        : 'border-transparent text-[color:var(--c-ink-2)] hover:text-[var(--c-primary)]',
                  )
                : classNames(
                    'rounded-[var(--r-sm)] px-3 py-1.5 border-0',
                    active
                      ? opt.muted && !opt.disabled
                        ? 'bg-[var(--c-surface)] text-[color:var(--c-ink-3)] shadow-[var(--shadow-card)]'
                        : 'bg-[var(--c-surface)] text-[var(--c-primary)] shadow-[var(--shadow-card)]'
                      : opt.muted && !opt.disabled
                        ? 'bg-transparent text-[color:var(--c-ink-3)] hover:text-[color:var(--c-ink-2)]'
                        : 'bg-transparent text-[color:var(--c-ink-2)] hover:text-[var(--c-primary)]',
                  ),
            )}
            onClick={() => {
              if (!opt.disabled && opt.value !== value) onChange?.(opt.value);
            }}
          >
            {opt.label}
          </button>
        );
      })}
    </div>
  );
};

export default PillTabs;
