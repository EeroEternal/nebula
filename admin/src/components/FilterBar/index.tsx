import classNames from 'classnames';
import { FC, PropsWithChildren, ReactNode } from 'react';

type Props = PropsWithChildren<{
  className?: string;
  /** 行末操作区（按钮等） */
  trailing?: ReactNode;
}>;

/** Demo 风格筛选/工具条：白底、浅描边、轻阴影 */
const FilterBar: FC<Props> = ({ children, className, trailing }) => {
  return (
    <div
      className={classNames(
        'w-full rounded-lg border border-[color:var(--c-border-light)] bg-[var(--c-surface)] shadow-card px-3 py-2.5 flex flex-wrap items-end gap-x-3 gap-y-2',
        className,
      )}
    >
      {children}
      {trailing ? <div className="ml-auto flex items-center gap-2 shrink-0">{trailing}</div> : null}
    </div>
  );
};

export default FilterBar;
