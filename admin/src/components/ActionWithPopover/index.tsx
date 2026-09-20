import { Popover } from 'antd';
import type { PopoverProps } from 'antd';
import { FC, PropsWithChildren } from 'react';

const ActionWithPopover: FC<PropsWithChildren<PopoverProps>> = ({ children, ...reset }) => {
  return (
    <Popover classNames={{ body: '!px-3 !py-1.5 !rounded-md border' }} {...reset}>
      <span className="cursor-pointer">{children}</span>
    </Popover>
  );
};
export default ActionWithPopover;
