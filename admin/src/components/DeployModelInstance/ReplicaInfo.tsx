import { Button, Form, message, Tooltip, Tag } from 'antd';
import type { SelectProps } from 'antd';
import { Trash2, Plus, CircleQuestionMark } from 'lucide-react';
import { DeviceStatus, IconButton } from '@/components';
import { DeviceInfo } from '@/types/Public/data';
import { calculatePercentage, sleep } from '@/utils';
import { l, lGet } from '@/utils/intl';
import { REPLICA_ROLE_OPTIONS } from '@/constants/modelData';
import type { FormInstance, FormListActionType } from '@ant-design/pro-components';
import {
  ProCard,
  ProFormDigit,
  ProFormList,
  ProFormSelect,
  ProFormText,
} from '@ant-design/pro-components';
import classNames from 'classnames';
import { isNumber, size } from 'lodash';
import { FC, useEffect, useMemo, useRef } from 'react';
import { useK8sRuntime } from '@/hooks/useK8sRuntime';

const initSpecifyDevicesValue = [
  {
    worker_ip: 'auto',
    worker_name: undefined,
    n_gpu: 'auto',
    gpu_idx: [],
    gpu_mem_gb: undefined,
    gpu_cores: undefined,
    gpu_type: undefined,
    model_path: undefined,
    role: undefined,
  },
];
export const initReplicaConfigValue = [{ replica_uid: '', devices: initSpecifyDevicesValue }];

interface BaseProps {
  form: FormInstance;
  devices: DeviceInfo[];
}

interface ReplicaInfoProps {
  modelName: string;
  modalBodyRef?: HTMLDivElement | null;
  isReplicaOut?: boolean;
  /** Prefill devices[].model_path (e.g. scale-out from existing instance) */
  defaultModelPath?: string;
}
interface SpecifyDevicesItem {
  worker_ip: string;
  n_gpu: string;
  gpu_idx: string[];
  gpu_mem_gb?: number;
  gpu_cores?: number;
  gpu_type?: string;
  model_path: string;
}
interface WorkerIPOptionsItem {
  label: string;
  value: string;
  name?: string;
  status?: DeviceInfo['status'];
}
const DeviceFormListGroup: FC<BaseProps & { replicaIndex: number; devicesIndex: number }> = ({
  form,
  devices,
  replicaIndex,
  devicesIndex,
}) => {
  const workerIpValue = Form.useWatch(
    ['replica_config', replicaIndex, 'devices', devicesIndex, 'worker_ip'],
    form,
  );

  const workerIpOptions: WorkerIPOptionsItem[] = useMemo(
    () => [
      { label: 'auto', value: 'auto' },
      ...devices.map((item) => ({
        label: item.worker_address,
        value: item.worker_address,
        name: item.name,
        status: item.status,
        disabled: item.status === 'expired',
      })),
    ],
    [devices],
  );
  const { isK8s, runtime } = useK8sRuntime();
  const workerIpIsAuto = workerIpValue === 'auto';
  const { gpus } = useMemo(
    () => devices.find((item) => item.worker_address === workerIpValue) || ({} as DeviceInfo),
    [devices, workerIpValue],
  );

  const nGpuOptions = useMemo(() => {
    if (workerIpIsAuto) return [{ label: 'auto', value: 'auto' }];
    return [
      { label: 'auto', value: 'auto' },
      ...Object.entries(gpus || {}).map(([, info], index) => ({
        label: String(index + 1),
        value: String(index + 1),
        disabled: info.status === 'expired',
      })),
    ];
  }, [workerIpIsAuto, gpus]);

  const selectedWorker = devices.find((item) => item.worker_address === workerIpValue);
  const isNpu = (selectedWorker?.accelerator_kind || '').toLowerCase() === 'npu';
  const showHami = Boolean(isK8s && runtime?.hami_enabled && !isNpu);
  const productOptions = useMemo(() => {
    const names = new Set<string>();
    for (const d of devices) {
      if ((d.accelerator_kind || '').toLowerCase() === 'npu') continue;
      Object.values(d.gpus || {}).forEach((g) => {
        if (g?.name) names.add(String(g.name));
      });
    }
    return [
      { label: l('models.deploy.replicaConfig.gpuType.any'), value: '' },
      ...Array.from(names).map((n) => ({ label: n, value: n })),
    ];
  }, [devices]);

  const GpuIdxsOptions = useMemo(() => {
    return workerIpIsAuto
      ? []
      : Object.entries(gpus || {}).map(([, info], index) => ({
          label: index,
          value: index,
          disabled: info.status === 'expired',
          vramPercent: calculatePercentage(info.mem_used, info.mem_total),
        }));
  }, [workerIpIsAuto, gpus]);

  const handleWorkerIPChange: SelectProps['onChange'] = (value) => {
    form.setFieldValue(
      ['replica_config', replicaIndex, 'devices', devicesIndex, 'worker_name'],
      value === 'auto' ? undefined : workerIpOptions.find((item) => item.value === value)?.name,
    );
    form.setFieldValue(
      ['replica_config', replicaIndex, 'devices', devicesIndex, 'n_gpu'],
      showHami ? 1 : 'auto',
    );
    form.setFieldValue(['replica_config', replicaIndex, 'devices', devicesIndex, 'gpu_idx'], []);
  };
  useEffect(() => {
    if (!showHami) return;
    const path = ['replica_config', replicaIndex, 'devices', devicesIndex, 'n_gpu'];
    const cur = form.getFieldValue(path);
    if (cur === 'auto' || cur == null || cur === '') {
      form.setFieldValue(path, 1);
    }
    const coresPath = ['replica_config', replicaIndex, 'devices', devicesIndex, 'gpu_cores'];
    if (form.getFieldValue(coresPath) == null) {
      form.setFieldValue(coresPath, 100);
    }
  }, [showHami, form, replicaIndex, devicesIndex]);

  const handleNGpuChange: SelectProps['onChange'] = () => {
    form.setFieldValue(['replica_config', replicaIndex, 'devices', devicesIndex, 'gpu_idx'], []);
  };
  const hasAutoConflict = (value: string) => {
    if (value !== 'auto') return false;
    const devicesValue = form.getFieldValue(['replica_config', replicaIndex, 'devices']) || [];
    return devicesValue.some(
      (item: SpecifyDevicesItem, i: number) => i !== devicesIndex && item.worker_ip !== 'auto',
    );
  };
  // 具体 Worker IP，不能与 auto 混用
  const workerIpValidator = (_rule: unknown, value: string) =>
    hasAutoConflict(value)
      ? Promise.reject(new Error(lGet('models.deploy.replicaConfig.workerIp.rules') as string))
      : Promise.resolve();
  const fillFullCard = () => {
    const first = Object.values(gpus || {})[0] as { mem_total?: number } | undefined;
    const bytes = Number(first?.mem_total || 0);
    const gb = bytes > 0 ? Math.max(1, Math.round(bytes / 1024 ** 3)) : undefined;
    const path = ['replica_config', replicaIndex, 'devices', devicesIndex];
    if (gb) form.setFieldValue([...path, 'gpu_mem_gb'], gb);
    form.setFieldValue([...path, 'gpu_cores'], 100);
  };
  const workerIpField = (
    <ProFormSelect
      name="worker_ip"
      label={l('models.deploy.replicaConfig.workerIp')}
      allowClear={false}
      options={workerIpOptions}
      fieldProps={{
        popupMatchSelectWidth: 340,
        optionRender: (option) => (
          <div className="flex justify-between items-center gap-x-[8px]" key={option.value}>
            <span>
              {option.value !== 'auto' ? `${option.label} (${option.data.name})` : option.label}
            </span>
            {option.data.status && <DeviceStatus status={option.data.status} />}
          </div>
        ),
        onChange: handleWorkerIPChange,
      }}
      rules={[{ validator: workerIpValidator }]}
    />
  );
  const gpuIdxField = (advanced: boolean) => (
    <ProFormSelect
      name="gpu_idx"
      label={
        advanced
          ? l('models.deploy.replicaConfig.gpuIdxAdvanced')
          : l('models.deploy.replicaConfig.gpuIdx')
      }
      options={GpuIdxsOptions}
      disabled={workerIpIsAuto}
      mode="multiple"
      fieldProps={{
        maxTagCount: 'responsive',
        optionRender: (option) => (
          <div className="flex justify-between items-center gap-x-[8px]" key={option.value}>
            <span>{option.label}</span>
            {option.data.disabled ? (
              <Tooltip title={l('global.license.disabledTips')}>
                <Tag className="mr-0" color="warning">
                  {l('monitoring.deviceInfo.gpu.status.unauthorized')}{' '}
                  <CircleQuestionMark size={12} />
                </Tag>
              </Tooltip>
            ) : (
              <span className={option.data.vramPercent > 90 ? 'text-error' : 'text-success'}>
                {option.data.vramPercent}%
              </span>
            )}
          </div>
        ),
      }}
    />
  );
  if (showHami) {
    return (
      <div key={devicesIndex} className="w-full space-y-1">
        <div className="grid grid-cols-12 gap-x-2 items-end">
          <div className="col-span-2">{workerIpField}</div>
          <div className="col-span-1">
            <ProFormDigit
              name="n_gpu"
              label={l('models.deploy.replicaConfig.nGpu')}
              min={1}
              fieldProps={{ precision: 0 }}
              initialValue={1}
            />
          </div>
          <div className="col-span-2">
            <ProFormDigit
              name="gpu_mem_gb"
              label={l('models.deploy.replicaConfig.gpuMem')}
              min={1}
              fieldProps={{ precision: 0, addonAfter: 'GB' }}
            />
          </div>
          <div className="col-span-1">
            <ProFormDigit
              name="gpu_cores"
              label={l('models.deploy.replicaConfig.gpuCores', 'GPU Core')}
              min={1}
              max={100}
              fieldProps={{ precision: 0 }}
              initialValue={100}
            />
          </div>
          <div className="col-span-2">
            <ProFormSelect
              name="gpu_type"
              label={l('models.deploy.replicaConfig.gpuType')}
              options={productOptions}
              allowClear
            />
          </div>
          <div className="col-span-3">{gpuIdxField(true)}</div>
          <div className="col-span-1 pb-2">
            <Button type="default" size="small" className="w-full" onClick={fillFullCard}>
              {l('models.deploy.replicaConfig.fillFull')}
            </Button>
          </div>
        </div>
        <div className="grid grid-cols-12 gap-x-2">
          <div className="col-span-10">
            <ProFormText
              label={l('models.deploy.replicaConfig.modelPath')}
              name="model_path"
              placeholder={l('models.deploy.replicaConfig.modelPath.placeholder')}
            />
          </div>
          <div className="col-span-2">
            <ProFormSelect
              name="role"
              label={l('models.deploy.replicaConfig.role')}
              options={REPLICA_ROLE_OPTIONS}
            />
          </div>
        </div>
        <ProFormText hidden name="worker_name" />
      </div>
    );
  }
  return (
    <div key={devicesIndex} className="grid grid-cols-[158px_108px_0.3fr_1fr_108px]">
      {workerIpField}
      <ProFormSelect
        name="n_gpu"
        label={l('models.deploy.replicaConfig.nGpu')}
        tooltip={lGet('models.deploy.replicaConfig.nGpu.npuHint',
        )}
        allowClear={false}
        options={nGpuOptions}
        disabled={workerIpIsAuto}
        onChange={handleNGpuChange}
      />
      {gpuIdxField(false)}
      <ProFormText
        label={l('models.deploy.replicaConfig.modelPath')}
        name="model_path"
        placeholder={l('models.deploy.replicaConfig.modelPath.placeholder')}
      />
      <ProFormSelect
        name="role"
        label={l('models.deploy.replicaConfig.role')}
        options={REPLICA_ROLE_OPTIONS}
      />
      <ProFormText hidden name="worker_name" />
    </div>
  );
};
const DeviceFormList: FC<
  BaseProps & { replicaIndex: number; defaultModelPath?: string }
> = ({ form, devices, replicaIndex, defaultModelPath }) => {
  const beforeAddRow = async () => {
    const specifyDevicesValue = form.getFieldValue([
      'replica_config',
      replicaIndex,
      'devices',
    ]) as SpecifyDevicesItem[];
    const hasAuto = (specifyDevicesValue || []).some((item) => item.worker_ip === 'auto');
    // 有auto,则不能在继续添加设备（具体 Worker IP，不能与 auto 混用）
    if (hasAuto) {
      message.warning(lGet('models.deploy.replicaConfig.addDevices.tips'));
      return false;
    }
    message.info(
      lGet('models.deploy.replicaConfig.addDevices.distributedTips',
      ),
    );
    return true;
  };
  return (
    <ProFormList
      name="devices"
      className="form-list-explain-error"
      copyIconProps={false}
      creatorRecord={{
        worker_ip: undefined,
        n_gpu: undefined,
        gpu_idx: [],
        model_path: defaultModelPath || undefined,
      }}
      actionGuard={{
        beforeAddRow,
      }}
      creatorButtonProps={{
        creatorButtonText: l('models.deploy.replicaConfig.addDevices'),
      }}
      actionRender={(field, action, _, count) => {
        return [
          <IconButton
            key="delete"
            onClick={() => {
              if (count === 1) return;
              action.remove(field.name);
            }}
            className="-mt-1 !h-6 !w-6 "
          >
            <Trash2 size={16} className="group-hover:text-danger text-muted" />
          </IconButton>,
        ];
      }}
      containerStyle={{ width: '100%' }}
    >
      {(_, devicesIndex) => (
        <DeviceFormListGroup
          devices={devices}
          form={form}
          replicaIndex={replicaIndex}
          devicesIndex={devicesIndex}
        />
      )}
    </ProFormList>
  );
};
const ReplicaInfo: FC<BaseProps & ReplicaInfoProps> = ({
  form,
  devices,
  modalBodyRef,
  modelName,
  isReplicaOut = false,
  defaultModelPath,
}) => {
  const replicaCardRef = useRef<FormListActionType>();
  const replicaValue = Form.useWatch('replica', form);
  const blankReplicaConfig = () => [
    {
      replica_uid: '',
      devices: [
        {
          worker_ip: 'auto',
          worker_name: undefined,
          n_gpu: 'auto',
          gpu_idx: [],
          model_path: defaultModelPath || undefined,
          role: undefined,
        },
      ],
    },
  ];
  const modalScroll = async (delta: number) => {
    if (!modalBodyRef) return;
    await sleep(200);
    modalBodyRef.scrollTo({
      top: modalBodyRef.scrollTop + delta,
      behavior: 'smooth',
    });
  };
  const handleAddReplicaCard = () => {
    replicaCardRef.current?.add(blankReplicaConfig()[0]);
    const count = size(replicaCardRef.current?.getList()) || 0;
    form.setFieldValue('replica', count);
    modalScroll(202);
  };
  const handDeleteReplicaCard = (index: number, count: number) => {
    replicaCardRef.current?.remove(index);
    // 删除副本card， 对应同步副本数量的值
    form.setFieldValue('replica', count - 1);
  };
  const handleReplica = (value: number | null) => {
    if (!isNumber(value)) return;
    const replicaConfig = form.getFieldValue('replica_config');
    if (value === size(replicaConfig)) return;
    if (value > size(replicaConfig)) {
      // 若当前输入的副本数量大于副本card的数量，则对应在生成差额的副本card
      Array.from({ length: value - size(replicaConfig) }).forEach(() => {
        replicaCardRef.current?.add(blankReplicaConfig()[0]);
      });
      modalScroll(202);
    } else if (value < size(replicaConfig)) {
      // 若当前输入的副本数量小于副本card的数量，则从副本card的数组从后往前删，删除对应的差额
      Array.from({ length: size(replicaConfig) - value }).forEach((_, i) => {
        replicaCardRef.current?.remove(size(replicaConfig) - 1 - i); // 从后往前删
      });
    }
  };
  return (
    <>
      {isReplicaOut ? (
        <div className="flex w-full items-center">
          <ProFormDigit
            label={l('models.deploy.replicaConfig.count')}
            name="replica"
            min={1}
            placeholder={l('models.deploy.replicaConfig.count')}
            colProps={{ span: 12 }}
            fieldProps={{
              precision: 0,
              onChange: (value) => handleReplica(value),
            }}
            tooltip={isReplicaOut ? l('models.instances.scaleOut.tips') : undefined}
            rules={[{ required: true }]}
          />
          <Button
            type="primary"
            shape="circle"
            size="small"
            className="mt-1.5"
            icon={<Plus size={16} />}
            onClick={handleAddReplicaCard}
          />
        </div>
      ) : (
        <div className="ml-4 flex w-full items-center justify-between mb-2">
          <div className="font-medium shrink-0 text-base">{l('models.deploy.replicaConfig')}</div>
          <div className="flex items-center">
            <span className="shrink-0 ant-form-item-label">
              {l('models.deploy.replicaConfig.count')}
            </span>
            <ProFormDigit
              noStyle
              name="replica"
              min={0}
              placeholder={l('models.deploy.replicaConfig.count')}
              fieldProps={{
                precision: 0,
                style: { width: 120 },
                onChange: (value) => handleReplica(value),
              }}
              rules={[{ required: true }]}
            />
          </div>
        </div>
      )}

      <ProFormList
        initialValue={defaultModelPath ? blankReplicaConfig() : initReplicaConfigValue}
        name="replica_config"
        actionRef={replicaCardRef}
        className={classNames('relative', {
          'h-0 overflow-hidden': replicaValue === 0,
        })}
        copyIconProps={false}
        creatorButtonProps={false}
        itemRender={({ listDom }, { index, fields }) => {
          return (
            <ProCard
              bordered
              style={{
                marginBlockEnd: 8,
              }}
              bodyStyle={{ padding: '16px 12px 0' }}
              className="relative"
            >
              {listDom}
              {(isReplicaOut ? size(fields) > 1 : true) && (
                <IconButton
                  key="delete"
                  onClick={() => handDeleteReplicaCard(index, size(fields))}
                  className="mt-[1px] !h-6 !w-6 absolute top-2 right-2.5"
                >
                  <Trash2 size={16} className="group-hover:text-danger text-muted" />
                </IconButton>
              )}
            </ProCard>
          );
        }}
        containerStyle={{ width: '100%' }}
      >
        {(_, replicaIndex) => (
          <>
            <ProFormText
              name="replica_uid"
              label={l('models.deploy.replicaConfig.name')}
              colProps={{ span: 12 }}
              tooltip={l('models.deploy.replicaConfig.name.tips')}
              placeholder={`${modelName}-${replicaIndex + 1}`}
            />
            <DeviceFormList
              replicaIndex={replicaIndex}
              form={form}
              devices={devices}
              defaultModelPath={defaultModelPath}
            />
          </>
        )}
      </ProFormList>
    </>
  );
};
export default ReplicaInfo;
