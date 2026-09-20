import { Inbox } from 'lucide-react';
import { l } from '@/utils/intl';
import { FC } from 'react';

const Default: FC = () => {
  return (
    <div className="h-full flex flex-col items-center justify-center text-muted">
      <div className="h-20 w-20 rounded-full bg-background-muted/60 flex items-center justify-center mb-4">
        <Inbox className="h-7 w-7 text-muted/50" />
      </div>
      <p className="text-sm">{l('models.instances.detail.resultPanelEmpty')}</p>
    </div>
  );
};
export default Default;
