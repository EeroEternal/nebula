

import { ButtonRoute } from '@/types/Studio/data';
import { Button } from 'antd';
import React, { useState } from 'react';

export const LoadingBtn: React.FC<ButtonRoute> = (route) => {
  const { click, title, icon, hotKeyDesc, props } = route;
  const [loading, setLoading] = useState(false);

  const handleClick = async (event: React.MouseEvent<HTMLElement, MouseEvent>) => {
    if (click) {
      setLoading(true);
      const result = await click();
      if (result instanceof Promise) {
        await result;
      }
      setLoading(false);
    }
  };

  return (
    <Button
      key={title}
      size={'small'}
      type={'text'}
      icon={icon}
      title={hotKeyDesc}
      loading={loading}
      onClick={(event) => handleClick(event)}
      {...props}
    >
      {title}
    </Button>
  );
};
