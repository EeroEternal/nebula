
import { StartIcon, StopIcon } from '@/components/Icons/CustomIcons';
import { Button } from 'antd';
import React from 'react';

type RunningBtnProps = {
  onClick: () => void;
  isRunning?: boolean;
  title?: string;
};

export const RunningBtn: React.FC<RunningBtnProps> = (props) => {
  const { onClick, isRunning = false, title = '启动' } = props;

  return (
    <Button
      type="link" size="small"
      className={'options-button'}
      title={title}
      icon={isRunning ? <StopIcon /> : <StartIcon />}
      onClick={() => onClick()}
    />
  );
};
