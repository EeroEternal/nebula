import { UserSwitchOutlined } from '@ant-design/icons';
import { Button } from 'antd';
import React from 'react';

type AssignButtonProps = {
  onClick: () => void;
  title?: string;
};

export const AssignBtn: React.FC<AssignButtonProps> = (props) => {
  const { onClick, title } = props;

  return (
    // <Button
    //   className={'options-button'}
    //   title={title}
    //   icon={<UserSwitchOutlined className={'blue-icon'} />}
    //   onClick={onClick}
    // />
    <Button className={'options-button'} type="link" size="small" onClick={() => onClick()}>
      分配
    </Button>
  );
};
