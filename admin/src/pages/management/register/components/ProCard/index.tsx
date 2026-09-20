import { ProCard } from '@ant-design/pro-components';
import type { ProCardProps } from '@ant-design/pro-components';
import { FC, PropsWithChildren } from 'react';
import classNames from 'classnames';

const ComCard: FC<PropsWithChildren<ProCardProps>> = ({
  children,
  className,
  headStyle = {},
  bodyStyle = {},
  ...reset
}) => {
  return (
    <ProCard
      headStyle={{ padding: '12px 24px', ...headStyle }}
      bodyStyle={{ padding: '0 24px', ...bodyStyle }}
      className={classNames('w-full rounded-2xl border border-border/50', className)}
      {...reset}
    >
      {children}
    </ProCard>
  );
};
export default ComCard;
