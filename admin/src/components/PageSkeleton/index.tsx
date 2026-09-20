import { Skeleton } from 'antd';
import { FC } from 'react';
import classNames from 'classnames';

export interface PageSkeletonProps {
  className?: string;
  /** 骨架行数，默认 4 */
  rows?: number;
  /** 是否显示头像占位，默认 false */
  avatar?: boolean;
}

/**
 * 整页 / 大块内容的骨架占位。
 * 区块内简单转圈请用 SectionLoading。
 */
const PageSkeleton: FC<PageSkeletonProps> = ({ className = '', rows = 4, avatar = false }) => {
  return (
    <div className={classNames('w-full py-6 px-1', className)} aria-busy="true" aria-live="polite">
      <Skeleton active avatar={avatar} paragraph={{ rows }} title={{ width: '40%' }} />
    </div>
  );
};

export default PageSkeleton;
