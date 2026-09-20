

import { ShowLogIcon } from '@/components/Icons/CustomIcons';
import { Button } from 'antd';
import React from 'react';

type ShowLogBtnProps = {
  onClick: () => void;
  disabled?: boolean;
};
//查看日志
export const ShowLogBtn: React.FC<ShowLogBtnProps> = (props) => {
  const { onClick, disabled = false } = props;
  return (
    <>
      <Button
        className={'options-button'}
        size="small"
        title="查看日志"
        disabled={disabled}
        icon={<ShowLogIcon />}
        onClick={() => onClick()}
      />
    </>
  );
};
