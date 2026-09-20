import { useMemo } from 'react';
import { history, useLocation } from '@umijs/max';
import { PageContainer, PillTabs } from '@/components';
import { l } from '@/utils/intl';
import ComputePanel from './ComputePanel';
import HardwarePanel from './HardwarePanel';
import { MonitorIcons } from '../icons';

type ClusterTab = 'hardware' | 'nodes';

/** 节点默认（左一）；卡健康 = /monitor/cluster/hardware */
const TAB_PATH: Record<ClusterTab, string> = {
  nodes: '/monitor/cluster',
  hardware: '/monitor/cluster/hardware',
};

function tabFromPath(pathname: string): ClusterTab {
  if (pathname.endsWith('/hardware')) return 'hardware';
  // /monitor/cluster、/nodes、旧 /gpu → 节点
  return 'nodes';
}

/** 监控运维 · 集群监控（节点 / 卡健康） */
const ClusterMonitor = () => {
  const location = useLocation();
  const tab = useMemo(() => tabFromPath(location.pathname), [location.pathname]);
  const NodesIcon = MonitorIcons.nodes;
  const HardwareIcon = MonitorIcons.hardware;

  return (
    <PageContainer
      title={l('menu.monitor.cluster')}
      subTitle={
        tab === 'nodes'
          ? l('monitor.cluster.subTitle.gpu')
          : l('monitor.cluster.subTitle.hardware',
            )
      }
    >
      <div className="flex flex-col gap-4">
        <PillTabs
          aria-label={l('monitor.cluster.tabs')}
          value={tab}
          options={[
            {
              value: 'nodes',
              label: (
                <span className="inline-flex items-center gap-1.5">
                  <NodesIcon size={14} aria-hidden />
                  {l('monitor.cluster.tab.nodes')}
                </span>
              ),
            },
            {
              value: 'hardware',
              label: (
                <span className="inline-flex items-center gap-1.5">
                  <HardwareIcon size={14} aria-hidden />
                  {l('monitor.cluster.tab.hardware')}
                  <span className="text-[10px] font-normal text-muted">
                    {l('monitor.cluster.hardware.nvidiaOnlyShort')}
                  </span>
                </span>
              ),
            },
          ]}
          onChange={(key) => history.push(TAB_PATH[key as ClusterTab])}
        />
        {tab === 'nodes' && <ComputePanel />}
        {tab === 'hardware' && <HardwarePanel />}
      </div>
    </PageContainer>
  );
};

export default ClusterMonitor;
