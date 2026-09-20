import { FC, PropsWithChildren } from 'react';
import { Skeleton } from 'antd';
import { ProBreadcrumb } from '@ant-design/pro-components';
import { ChevronRight } from 'lucide-react';
import classNames from 'classnames';

interface PageHeaderProps {
  showPageHeader?: boolean;
  /** 详情页等建议开启；默认 false */
  showBreadcrumb?: boolean;
  title?: React.ReactNode;
  subTitle?: React.ReactNode;
  extraContent?: React.ReactNode;
  loading?: boolean;
  className?: string;
}

const PageContainer: FC<PropsWithChildren<PageHeaderProps>> = ({
  showPageHeader = true,
  showBreadcrumb = false,
  title,
  subTitle,
  extraContent,
  children,
  loading = false,
  className = '',
}) => {
  return (
    <div className={classNames('w-full min-w-0', className)}>
      {showBreadcrumb && (
        <div className="mb-3 overflow-x-auto">
          <ProBreadcrumb separator={<ChevronRight size={14} />} />
        </div>
      )}
      {showPageHeader && (
        <div className="flex flex-col gap-3 mb-6 md:flex-row md:items-start md:justify-between">
          <div className="min-w-0 shrink">
            <h1 className="text-2xl font-bold text-default !mb-0 truncate">{title}</h1>
            {subTitle && <div className="text-sm text-muted mt-1">{subTitle}</div>}
          </div>
          {extraContent && (
            <div className="flex flex-wrap items-center gap-2 min-w-0 md:justify-end shrink-0">
              {extraContent}
            </div>
          )}
        </div>
      )}
      <Skeleton active loading={loading}>
        {children}
      </Skeleton>
    </div>
  );
};

export default PageContainer;
