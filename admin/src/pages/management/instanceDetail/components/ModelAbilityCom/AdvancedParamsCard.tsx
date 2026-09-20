import { ProCard } from '@ant-design/pro-components';
import { FC, PropsWithChildren, useState } from 'react';
import { ChevronRight, ChevronDown } from 'lucide-react';
import { l } from '@/utils/intl';

const AdvancedParamsCard: FC<PropsWithChildren> = ({ children }) => {
  const [collapsed, setCollapsed] = useState(true);

  const onCollapse = (collapsed: boolean) => {
    setCollapsed(collapsed);
  };
  return (
    <ProCard
      collapsible
      collapsed={collapsed}
      onCollapse={onCollapse}
      collapsibleIconRender={() => null}
      title={
        <div className="flex items-center cursor-pointer text-sm w-full">
          {collapsed ? <ChevronRight size={18} /> : <ChevronDown size={18} />}
          {l('models.instances.detail.AdvancedParams')}
        </div>
      }
      headStyle={{ padding: '0 0 16px' }}
      bodyStyle={{ padding: 0 }}
    >
      {children}
    </ProCard>
  );
};

export default AdvancedParamsCard;
