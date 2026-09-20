

import { SWITCH_OPTIONS } from '@/constants';
import { Space, Switch } from 'antd';
import React from 'react';

type EnableSwitchProps = {
  record: { is_active?: boolean };
  onChange: () => void;
  disabled?: boolean;
};
export const EnableSwitchBtn: React.FC<EnableSwitchProps> = (props) => {
  const { record, onChange, disabled = false } = props;

  return (
    <>
      <Space className={'hidden-overflow'}>
        <Switch
          {...SWITCH_OPTIONS()}
          checked={record.is_active}
          disabled={disabled}
          onChange={() => onChange()}
        />
      </Space>
    </>
  );
};
