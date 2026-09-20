import { useMemo } from 'react';
import { history, useLocation } from '@umijs/max';
import { l } from '@/utils/intl';
import { PageContainer, PillTabs } from '@/components';
import OperationsLog from './components/OperationsLog';
import NewServiceLog from './components/NewServiceLog';

type TabKey = 'service' | 'operations';

const Log = () => {
  const location = useLocation();
  const activeTabKey = useMemo<TabKey>(() => {
    const q = new URLSearchParams(location.search).get('tab');
    return q === 'operations' ? 'operations' : 'service';
  }, [location.search]);

  const contents = {
    service: <NewServiceLog />,
    operations: <OperationsLog />,
  };

  return (
    <PageContainer title={l('menu.monitor.logs')}>
      <div className="flex flex-col w-full gap-4">
        <PillTabs
          value={activeTabKey}
          options={[
            { value: 'service', label: l('monitor.logs.service') },
            { value: 'operations', label: l('monitor.logs.operation') },
          ]}
          onChange={(key) => {
            const next = key as TabKey;
            const params = new URLSearchParams(location.search);
            if (next === 'operations') {
              params.set('tab', 'operations');
              params.delete('request_id');
            } else {
              params.delete('tab');
            }
            const search = params.toString();
            history.replace({
              pathname: '/monitor/logs',
              search: search || undefined,
            });
          }}
        />
        <div>{contents[activeTabKey]}</div>
      </div>
    </PageContainer>
  );
};
export default Log;
