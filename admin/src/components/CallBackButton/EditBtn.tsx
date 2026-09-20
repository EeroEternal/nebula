import { l } from '@/utils/intl';
import { Button } from 'antd';
import React from 'react';

type EditButtonProps = {
  onClick: () => void;
  disabled?: boolean;
};

export const EditBtn: React.FC<EditButtonProps> = (props) => {
  const { onClick, disabled = false } = props;
  // large、middle、small
  return (
    // <Button
    //   className={'options-button'}
    //   icon={<EditTwoTone />}
    //   disabled={disabled}
    //   title={l('global.actions.edit')}
    //   onClick={() => onClick()}
    // />
    <Button className={'options-button'} type="link" size="small" onClick={() => onClick()}>
      {l('global.actions.edit')}
    </Button>
  );
};
