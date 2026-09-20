import { Tooltip } from 'antd';
import type { TooltipProps } from 'antd';
import { FC } from 'react';

interface ActionWithTipsProps {
  children: React.ReactNode;
}
const ActionWithTips: FC<ActionWithTipsProps & TooltipProps> = ({ children, ...reset }) => {
  return (
    <Tooltip {...reset}>
      <span className="cursor-pointer text-primary hover:text-primary/60">{children}</span>
    </Tooltip>
  );
};
export default ActionWithTips;
