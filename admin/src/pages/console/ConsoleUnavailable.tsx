import React from 'react';
import { CloudOff } from 'lucide-react';
import { EmptyState } from '@/components';
import { l } from '@/utils/intl';

/** 直达 /console/* 且 Region Console 未启时的空态，不发 /api/console 列表请求。 */
const ConsoleUnavailable: React.FC = () => (
  <div className="p-[16px]">
    <EmptyState
      className="py-20"
      customIcon={<CloudOff size={48} className="text-muted" />}
      title={l('global.console.unavailable')}
      description={l('global.console.unavailableHint')}
    />
  </div>
);

export default ConsoleUnavailable;
