
import { PlusOutlined } from '@ant-design/icons';
import { Button } from 'antd';
import React from 'react';

type CreateButtonProps = {
  onClick: () => void;
};

export const CreateBtn: React.FC<CreateButtonProps> = (props) => {
  const { onClick } = props;

  return (
    <Button type='primary' onClick={onClick}>
      <PlusOutlined />
      新建
    </Button>
  );
};
