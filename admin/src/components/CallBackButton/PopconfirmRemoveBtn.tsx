import { l } from '@/utils/intl';
import { Button, Popconfirm } from 'antd';
import React from 'react';

type PopconfirmProps = {
  onClick: () => void;
  description: string | React.ReactNode;
  options?: React.ComponentProps<typeof Button>;
  disabled?: boolean;
  buttonIcon?: React.ReactNode;
  title?: string | React.ReactNode;
};

export const PopconfirmDeleteBtn: React.FC<PopconfirmProps> = (props) => {
  const { onClick, description, disabled = false, title } = props;

  return (
    //外面包装一层onclick，阻止冒泡传递事件
    <span onClick={(e) => e.stopPropagation()}>
      <Popconfirm
        placement="topRight"
        title={title ?? l('global.actions.delete')}
        description={<div className={'needWrap'}>{description} </div>}
        onConfirm={onClick}
        disabled={disabled}
        okText={l('global.actions.ok')}
        cancelText={l('global.actions.cancel')}
      >
        {/* <Button
          {...options}
          title={title ?? l('button.delete')}
          key={'DeleteIcon'}
          disabled={disabled}
          htmlType={'submit'}
          autoFocus
          icon={buttonIcon ?? <DangerDeleteIcon />}
        /> */}
        <Button className={'options-button bg-orange-500'} type="link" size="small" danger>
          {l('global.actions.delete')}
        </Button>
      </Popconfirm>
    </span>
  );
};
