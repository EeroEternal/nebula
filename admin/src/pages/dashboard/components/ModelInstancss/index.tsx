import { Card, Button, Tag } from 'antd';
import { Layers, Rocket, Plus, Server, Cpu } from 'lucide-react';
import { FC } from 'react';
import { size } from 'lodash';

import { l } from '@/utils/intl';
import type { OverviewResponse } from '@/types/Public/data';
import { history } from '@umijs/max';
import { IntanceStatus } from '@/constants/intance';
interface ModelInstancssProps {
  instancesSource: OverviewResponse['instance_source'];
}

const ModelInstancss: FC<ModelInstancssProps> = ({ instancesSource }) => {
  const runningModels = instancesSource.filter((item) => item.status === IntanceStatus.READY);

  const renderStatus = (status: IntanceStatus) => {
    if (status === IntanceStatus.READY) {
      return (
        <Tag bordered={false} color="success" className="mr-0">
          {l('dashboard.modelDeployment.running')}
        </Tag>
      );
    }
    if ([IntanceStatus.CREATING, IntanceStatus.UPDATING].includes(status)) {
      return (
        <Tag bordered={false} color="processing" className="mr-0">
          {l(`model.running.status.${status}`)}
        </Tag>
      );
    }
    if (status === IntanceStatus.ERROR) {
      return (
        <Tag bordered={false} color="error" className="mr-0">
          {l(`model.running.status.${status}`)}
        </Tag>
      );
    }
    return (
      <Tag bordered={false} className="mr-0">
        {l(`model.running.status.${status}`)}
      </Tag>
    );
  };

  const handleToModelRepository = () => history.push('/models/repository');
  const handleToModelInstances = (id?: string) =>
    history.push(`/models/instances${id ? `?id=${id}` : ''}`);

  return (
    <Card
      title={
        <div
          className="inline-flex gap-2 items-center cursor-pointer"
          onClick={() => handleToModelInstances()}
        >
          <Layers size={16} className="text-muted" />
          {l('dashboard.modelDeployment')}
        </div>
      }
      extra={
        !!size(runningModels) && (
          <div className="py-0.5 px-2.5 bg-background text-[11px] rounded-lg text-secondary">
            {size(runningModels)} {l('dashboard.modelDeployment.running')}
          </div>
        )
      }
      classNames={{ header: '!border-b-0 !p-6 !pb-4', body: '!pt-0 flex-1 !max-h-72' }}
      className="lg:col-span-2 flex flex-col"
    >
      {size(instancesSource) ? (
        <div>
          <div className="w-full grid grid-cols-2 gap-2">
            {instancesSource.slice(0, 4).map((item) => (
              <div
                className="rounded-lg bg-background/50 p-3 hover:bg-background/70 transition-colors cursor-pointe flex flex-col gap-2 min-w-[74px] cursor-pointer"
                key={item.model_uid}
                onClick={() => handleToModelInstances(item.model_uid)}
              >
                <div className="flex items-center justify-between gap-2">
                  <div className="font-medium text-sm text-default truncate flex-1">
                    {item.model_uid}
                  </div>
                  {renderStatus(item.status as IntanceStatus)}
                </div>
                {/* 副本信息 */}
                {!!size(item.replica_data_source) &&
                  (item.replica_data_source || []).slice(0, 1).map((item) => (
                    <div className="flex flex-col gap-1.5" key={item.replica_model_uid}>
                      <div className="flex items-center gap-1.5 flex-wrap">
                        <Tag className="text-default rounded-lg bg-transparent mr-0">
                          <Server size={12} />
                          {item.replica_model_uid}
                        </Tag>
                        {item.gpu_idx.map((gpu) => (
                          <Tag
                            key={gpu}
                            className="rounded-full mr-0"
                            bordered={false}
                            color="blue"
                          >
                            <Cpu size={12} />
                            <span>GPU</span>
                            {gpu}
                          </Tag>
                        ))}
                      </div>
                      <div className="text-xs text-muted ">
                        {l('dashboard.modelDeployment.workerAddress')}:{item.worker_address}
                      </div>
                    </div>
                  ))}
                {size(item.replica_data_source) > 1 && (
                  <div className="text-xs text-muted hover:text-primary cursor-pointer">
                    {l('dashboard.modelDeployment.replicaInfo')}
                  </div>
                )}
              </div>
            ))}
          </div>
          {size(instancesSource) > 4 && (
            <div className="w-full text-center">
              <span className="text-xs mt-1.5 cursor-pointer text-muted hover:text-primary">
                {l('global.viewmore')}
              </span>
            </div>
          )}
        </div>
      ) : (
        <div className="flex flex-col items-center justify-center h-full py-8 text-center">
          <div className="rounded-full bg-background/80 p-4 mb-4">
            <Rocket className="h-8 w-8 text-muted" />
          </div>
          <h3 className="text-sm font-medium text-default mb-1">
            {l('dashboard.modelDeployment.noModelsDeployed')}
          </h3>
          <p className="text-xs text-muted mb-4 max-w-[220px]">
            {l('dashboard.modelDeployment.noModelsDescription')}
          </p>
          <Button
            type="primary"
            icon={<Plus size={16} />}
            onClick={handleToModelRepository}
            className="h-[36px]"
          >
            {l('dashboard.modelDeployment.deploy')}
          </Button>
        </div>
      )}
    </Card>
  );
};
export default ModelInstancss;
