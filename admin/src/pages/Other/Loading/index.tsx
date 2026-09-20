

import { l } from '@/utils/intl';
import { PageLoading } from '@ant-design/pro-components';
import React from 'react';

type LoadingProps = {
  loading: boolean;
};

export const Loading: React.FC<LoadingProps> = ({ loading }) => {
  return (
    <>
      <PageLoading spinning={loading} tip='加载中...' size='large' />
    </>
  );
};
