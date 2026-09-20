import { useRef } from 'react';
import dayjs from 'dayjs';

import { l } from '@/utils/intl';
import useAutoCancelRequest from '@/hooks/useAutoCancelRequest';
import request from '@/utils/request';
import { transformRate } from '@/utils';
import type { OverviewResponse } from '@/types/Public/data';
import { PageContainer } from '@/components';

import DeviceOverview from './components/DeviceOverview';
import ModelInstancss from './components/ModelInstancss';
import ModelUsageChart from './components/ModelUsageChart';

const Dashboard = () => {
  const lastUpdateTimeRef = useRef('-');
  const { data } = useAutoCancelRequest<{ data: OverviewResponse }>(() => request('/overview'), {
    pollingInterval: 2000,
    onSuccess: () => (lastUpdateTimeRef.current = dayjs().format('HH:mm:ss')),
  });
  const {
    data_source = [],
    gpu_used_rate = 0,
    device_info = [],
    tokens_usage = [],
    instance_source = [],
  } = data?.data?.data || {};

  return (
    <PageContainer
      title={l('menu.dashboard')}
      subTitle={`${l('dashboard.pageSubTitle')} · ${l('global.lastUpdateTime')}: ${
        lastUpdateTimeRef.current
      }`}
    >
      <div className="grid gap-6 lg:grid-cols-3">
        <DeviceOverview deviceInfo={device_info} avgGpuUsage={transformRate(gpu_used_rate)} />
        <ModelInstancss instancesSource={instance_source} />
      </div>
      <ModelUsageChart
        modelCallsData={data_source}
        tokenTrendData={tokens_usage}
        modelUids={instance_source.map((i) => i.model_uid).filter(Boolean)}
      />
    </PageContainer>
  );
};

export default Dashboard;
