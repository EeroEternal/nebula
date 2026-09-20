

import { DangerDeleteIcon } from '@/components/Icons/CustomIcons';
import { Button } from 'antd';
import React from 'react';

type NormalDeleteButtonProps = {
  onClick: () => void;
};

export const NormalDeleteBtn: React.FC<NormalDeleteButtonProps> = (props) => {
  const { onClick } = props;

  // return (
  //   <Button
  //     className={'options-button'}
  //     icon={<DangerDeleteIcon />}
  //     title={l('button.delete')}
  //     onClick={() => onClick()}
  //   />
  // );
  // large、middle、small
  return (
    <Button className={'options-button bg-orange-500'} type="link" size="small" onClick={() => onClick()}>
      移除
    </Button>
  );
};
