import { Button } from 'antd';
import { FC, ReactNode } from 'react';
import classNames from 'classnames';
import { AlertCircle, RefreshCw } from 'lucide-react';
import { l } from '@/utils/intl';

export interface ErrorStateProps {
  className?: string;
  title?: ReactNode;
  description?: ReactNode;
  /** 自定义主操作；与 onRetry 二选一即可 */
  action?: ReactNode;
  /** 提供时渲染默认「重试」按钮 */
  onRetry?: () => void;
  retryText?: ReactNode;
  hideIcon?: boolean;
}

const ErrorState: FC<ErrorStateProps> = ({
  className = '',
  title,
  description,
  action,
  onRetry,
  retryText,
  hideIcon = false,
}) => {
  const heading = title ?? l('global.error.title');
  const body = description ?? l('global.error.description');
  const retryLabel = retryText ?? l('global.actions.retry');

  const resolvedAction =
    action ??
    (onRetry ? (
      <Button type="primary" icon={<RefreshCw size={14} />} onClick={onRetry}>
        {retryLabel}
      </Button>
    ) : null);

  return (
    <div
      role="alert"
      className={classNames(
        'm-0 flex flex-col items-center justify-center gap-y-2 py-10 px-4',
        className,
      )}
    >
      {!hideIcon && (
        <div className="mb-1 text-error">
          <AlertCircle size={40} strokeWidth={1.5} aria-hidden />
        </div>
      )}
      <div className="text-default text-base font-medium text-center">{heading}</div>
      {body && <div className="text-muted text-sm text-center max-w-md">{body}</div>}
      {resolvedAction && <div className="mt-2">{resolvedAction}</div>}
    </div>
  );
};

export default ErrorState;
