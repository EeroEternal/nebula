

import { TabsItemType, TaskDataType } from '@/pages/DataStudio/model';
import { Tab } from '@/pages/DataStudio/route';
import { Button } from 'antd';
import React from 'react';

export type CircleButtonProps = {
  icon: React.ReactNode;
  loading?: boolean;
  onClick?: () => void;
  title?: string;
  key?: string;
  href?: string;
};
export type CircleBottomButtonProps = {
  icon: React.ReactNode;
  loading?: boolean;
  onClick?: (
    tabs: Tab[],
    key: string,
    data: TaskDataType | undefined,
    refresh: () => void,
  ) => Promise<void>;
  title?: string;
  key?: string;
};
export type CircleDataStudioButtonProps = {
  icon: React.ReactNode;
  loading?: boolean;
  onClick?: (panes: TabsItemType[], activeKey: string) => void;
  title?: string;
  key?: string;
  isShow?: boolean;
};

export const CircleBtn: React.FC<CircleButtonProps> = (props) => {
  const { onClick, title, icon, loading, href } = props;

  return (
    <Button
      title={title}
      loading={loading}
      icon={icon}
      block
      type={'text'}
      shape={'circle'}
      onClick={onClick}
      href={href}
      download=''
    />
  );
};
