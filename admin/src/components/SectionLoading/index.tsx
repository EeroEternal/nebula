import { FC } from 'react';
import classNames from 'classnames';
import { Loader2 } from 'lucide-react';

interface SectionLoadingProps {
  className?: string;
  /** Loader 尺寸，默认 32 */
  size?: number;
}

/** 区块内转圈加载；整页骨架请用 PageSkeleton */
const SectionLoading: FC<SectionLoadingProps> = ({ className = '', size = 32 }) => {
  return (
    <div
      className={classNames('flex items-center justify-center py-20', className)}
      role="status"
      aria-live="polite"
      aria-busy="true"
    >
      <Loader2 size={size} className="animate-spin text-muted" aria-hidden />
    </div>
  );
};

export default SectionLoading;
