import { l } from '@/utils/intl';
import { Button } from 'antd';
import React from 'react';

type ReadButtonProps = {
  onClick: () => void;
  disabled?: boolean;
};

export const ReadBtn: React.FC<ReadButtonProps> = (props) => {
  const { onClick, disabled = false } = props;
  // large、middle、small
  return (
    // <Button
    //   className={'options-button'}
    //   icon={<EyeTwoTone />}
    //   disabled={disabled}
    //   title={l('global.actions.detail')}
    //   onClick={() => onClick()}
    // />
    <Button className={'options-button'} type="link" size="small" onClick={() => onClick()}>
      {l('global.actions.detail')}
    </Button>
  );
};
