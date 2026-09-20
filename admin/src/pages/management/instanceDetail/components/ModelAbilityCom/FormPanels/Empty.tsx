import { Inbox } from 'lucide-react';
import { FC } from 'react';

import { l } from '@/utils/intl';

const Empty: FC = () => {
  return (
    <div className="h-full flex flex-col items-center justify-center text-muted">
      <Inbox size={60} className="mb-3 opacity-50" strokeWidth={0.8} />
      <p className="text-sm">{l('models.instances.detail.formPanelEmpty')}</p>
    </div>
  );
};
export default Empty;
