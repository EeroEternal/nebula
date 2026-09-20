import { useMemo } from 'react';
import { history, useLocation, useModel } from '@umijs/max';
import { Alert, Button } from 'antd';
import { PageContainer, PillTabs } from '@/components';
import { SETTING_MODAL_TABS } from '@/constants';
import { l } from '@/utils/intl';
import UsagePanel from './UsagePanel';
import TraceList from '../traces/TraceList';

type TrafficTab = 'usage' | 'traces';

/**
 * 监控运维 · 对话链路
 * Tab：总览 · 链路
 */
const TrafficPage = () => {
  const location = useLocation();
  const { initialState, setInitialState } = useModel('@@initialState');
  const enableLangfuse = Boolean(initialState?.globalConfig?.enable_langfuse);
  const tab = useMemo<TrafficTab>(() => {
    const params = new URLSearchParams(location.search);
    const q = params.get('tab');
    if (q === 'traces' || params.get('model')) return 'traces';
    return 'usage';
  }, [location.search]);

  const openMonitoringSettings = () => {
    setInitialState((prev) => ({
      ...prev,
      settingModalVisible: true,
      settingModalActiveTab: SETTING_MODAL_TABS.MONITORING,
    } as typeof prev));
  };

  return (
    <PageContainer
      title={l('menu.monitor.traffic')}
      subTitle={l('monitor.traffic.subTitle')}
    >
      <div className="flex flex-col gap-3">
        {!enableLangfuse ? (
          <Alert
            type="info"
            showIcon
            message={l('monitor.traffic.xtraceDisabled.title',
            )}
            description={l('monitor.traffic.xtraceDisabled.desc',
            )}
            action={
              <Button size="small" type="primary" onClick={openMonitoringSettings}>
                {l('monitor.traffic.xtraceDisabled.action')}
              </Button>
            }
          />
        ) : null}
        <PillTabs
          aria-label={l('monitor.traffic.tabs')}
          value={tab}
          options={[
            {
              value: 'usage',
              label: l('monitor.traffic.tab.usage'),
            },
            {
              value: 'traces',
              label: l('monitor.traffic.tab.traces'),
            },
          ]}
          onChange={(key) => {
            const next = key as TrafficTab;
            const params = new URLSearchParams(location.search);
            if (next === 'traces') {
              params.set('tab', 'traces');
            } else {
              params.delete('tab');
              params.delete('model');
            }
            const search = params.toString();
            history.replace({
              pathname: '/monitor/traffic',
              search: search || undefined,
            });
          }}
        />
        {enableLangfuse ? (
          tab === 'usage' ? (
            <UsagePanel />
          ) : (
            <TraceList />
          )
        ) : null}
      </div>
    </PageContainer>
  );
};

export default TrafficPage;
