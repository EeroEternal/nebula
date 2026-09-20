import { useMemo } from 'react';
import { history, useLocation } from '@umijs/max';
import { PageContainer, PillTabs } from '@/components';
import { l } from '@/utils/intl';
import InstanceOpsPanel from './InstanceOpsPanel';
import OverviewPanel from './components/OverviewPanel';

type MonitorTab = 'overview' | 'perf';

/**
 * 模型监控：按分析域分 Tab
 * - 数据总览（默认，左一）：过往服务统计 + 历史均值查询
 * - 实时监控：SSE 实时负载 + 探活/压测
 */
const MonitorInstancesPage = () => {
  const location = useLocation();
  const params = useMemo(() => new URLSearchParams(location.search), [location.search]);
  const rawTab = params.get('tab');
  // 缺省数据总览；?id= 打开总览「查询」抽屉；旧 ?tab=alerts 回落数据总览
  const tab: MonitorTab =
    rawTab === 'perf' || rawTab === 'overview' ? rawTab : 'perf';

  const setTab = (next: MonitorTab) => {
    const p = new URLSearchParams(location.search);
    if (next === 'perf') p.delete('tab');
    else p.set('tab', next);
    if (next !== 'perf') p.delete('id');
    const search = p.toString();
    history.replace({
      pathname: '/monitor/instances',
      search: search || undefined,
    });
  };

  return (
    <PageContainer
      title={l('menu.monitor.instances')}
      subTitle={l('monitor.instances.subTitle')}
    >
      <div className="flex flex-col gap-4">
        <PillTabs
          aria-label={l('monitor.instances.tabs')}
          value={tab}
          options={[
            {
              value: 'perf',
              label: l('monitor.instances.tab.perf'),
            },
            {
              value: 'overview',
              label: l('monitor.instances.tab.overview'),
            },
          ]}
          onChange={(key) => setTab(key as MonitorTab)}
        />
        {tab === 'perf' ? <InstanceOpsPanel embedded /> : null}
        {tab === 'overview' ? <OverviewPanel /> : null}
      </div>
    </PageContainer>
  );
};

export default MonitorInstancesPage;
