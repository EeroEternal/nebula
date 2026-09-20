import { ProCard } from '@ant-design/pro-components';
import type { ProCardProps } from '@ant-design/pro-components';
import React, { FC } from 'react';
import { ChevronRight, ChevronDown } from 'lucide-react';

interface ProCardForConfigProps extends ProCardProps {
  children: React.ReactNode;
}
const ProCardForConfig: FC<ProCardForConfigProps> = ({ children, ...reset }) => {
  const { collapsed = false, title } = reset;
  return (
    <ProCard
      {...reset}
      bordered
      collapsible
      collapsed={collapsed}
      collapsibleIconRender={() => null}
      headStyle={{ padding: '12px 8px' }}
      bodyStyle={{ padding: 4 }}
      title={
        <div className="flex items-center text-sm cursor-pointer">
          {collapsed ? <ChevronRight size={16} /> : <ChevronDown size={16} />}
          {title}
        </div>
      }
    >
      {children}
    </ProCard>
  );
};
export default ProCardForConfig;
