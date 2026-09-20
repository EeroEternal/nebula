import { Empty } from 'antd';
import { FC, ReactNode } from 'react';
import classnames from 'classnames';
import { l } from '@/utils/intl';

interface DataEmptyProps {
  className?: string;
  hideIcon?: boolean;
  emptyText?: React.ReactNode;
  customIcon?: React.ReactNode;
  /** 可选标题；不传则只显示 emptyText / description */
  title?: React.ReactNode;
  description?: React.ReactNode;
  action?: ReactNode;
}

const DataEmpty: FC<DataEmptyProps> = ({
  className = '',
  hideIcon = false,
  emptyText = '',
  customIcon,
  title,
  description,
  action,
}) => {
  const body = description || emptyText || (!title ? l('global.data.empty') : '');
  return (
    <div
      className={classnames(
        'm-[0px] flex flex-col justify-center items-center gap-y-2 py-4',
        className,
      )}
    >
      {!hideIcon && <div>{customIcon || Empty.PRESENTED_IMAGE_SIMPLE}</div>}
      {title && <div className="text-default text-base font-medium">{title}</div>}
      {body ? <div className="text-muted text-sm text-center">{body}</div> : null}
      {action && <div className="mt-2">{action}</div>}
    </div>
  );
};
export default DataEmpty;
