import type { FormListActionType, ProFormProps } from '@ant-design/pro-components';
import { ProForm, ProFormList, ProFormText } from '@ant-design/pro-components';
import { useModel } from '@umijs/max';
import { useRequest } from 'ahooks';
import { AutoComplete, Button, Col, Empty, Form, Row, Skeleton } from 'antd';
import { isArray, isEmpty, omitBy, size } from 'lodash';
import { forwardRef, useImperativeHandle, useMemo, useRef, useState } from 'react';
import { Plus, Trash2 } from 'lucide-react';
import classNames from 'classnames';

import { DotPulse, WaveProgressBar, IconButton } from '@/components';
import { MODEL_KWARGS_OPTIONS, ModelAbility } from '@/constants/modelData';
import type { InstanceDetail } from '@/types/Public/data';
import { formatProgress, generateUUID, sleep, transformValueType } from '@/utils';
import { l } from '@/utils/intl';
import request from '@/utils/request';
import Divider from './Divider';
import useModelAbilityConfig from './useModelAbilityConfig';
import type { AbilityApiResult, AbilityRunResult } from '../../../abilityFormTypes';

export interface ModelAbilityComRef {
  resetFields: () => void;
}

const fullCol = {
  xxl: 24,
  xl: 24,
  lg: 24,
  md: 24,
  sm: 24,
  xs: 24,
};
const halfCol = {
  xxl: 12,
  xl: 12,
  lg: 24,
  md: 24,
  sm: 24,
  xs: 24,
};
const ModelAbilityCom = forwardRef<ModelAbilityComRef>((_, ref) => {
  const { selectModelAbility, instanceDetail, replicaId, compareData } = useModel(
    'management.instanceDetail.model',
  );
  const extraConfigRef = useRef<FormListActionType>();
  const [form] = Form.useForm();
  const showCompare = size(compareData) > 1;
  const showProgress = [
    ModelAbility.text2image,
    ModelAbility.image2image,
    ModelAbility.inpainting,
    ModelAbility.text2video,
    ModelAbility.image2video,
    ModelAbility.flf2v,
  ].includes(selectModelAbility);
  const progressRef = useRef<Record<string, number>>({});
  const [results, setResults] = useState<Record<string, AbilityRunResult>>({});
  const [loadingMap, setLoadingMap] = useState<Record<string, boolean>>({});

  const loading = useMemo(() => {
    if (isEmpty(loadingMap) || !loadingMap) return false;
    return Object.values(loadingMap).some((item) => item);
  }, [loadingMap]);
  const transformFormList = (extraConfig: { key: string; value: string }[]) => {
    const kwargs = (extraConfig || [])
      .filter((item) => !isEmpty(item) && item.key && item.value)
      .reduce((acc, item) => {
        acc[item.key] = transformValueType(item.value);
        return acc;
      }, {} as Record<string, unknown>);
    return kwargs;
  };
  const transformKwargs = (initKwargs: Record<string, unknown>) => {
    return omitBy(
      initKwargs,
      (value) =>
        value === null ||
        value === undefined ||
        value === '' ||
        (isArray(value) && value.length === 0),
    );
  };

  const modelAbilityConfig = useModelAbilityConfig({
    form,
    selectModelAbility,
    instanceDetail,
  });

  const { run: getProgress, cancel } = useRequest(
    (params) => request(`/requests/${params.uuid}/progress`),
    {
      manual: true,
      pollingInterval: 1000,
      onSuccess: (res, params) => {
        const modelId = params[0]?.modelId;
        if (!modelId) return;
        if (!res.success || res.data?.progress === 1) cancel();
        if (res.success) {
          progressRef.current[modelId] = res?.data?.progress || 0;
        }
      },
      onError: () => {
        cancel();
      },
    },
  );

  const renderRightPanel = (item: InstanceDetail) => {
    const modelId = item.model_uid;
    const progress = progressRef.current[modelId] || 0;
    const data = results[modelId];
    const loading = loadingMap[modelId];
    if (showProgress && loading) {
      return (
        <div className="mt-[150px] flex flex-col items-center justify-center">
          <WaveProgressBar progress={formatProgress(progress * 100)} />
          <DotPulse>{modelAbilityConfig?.submitLoadingText}</DotPulse>
        </div>
      );
    }
    if (loading) {
      return (
        <div className="mt-[150px] flex flex-col items-center justify-center">
          <Skeleton.Node active style={{ width: 250, borderRadius: 8 }} />
          <DotPulse>{modelAbilityConfig?.submitLoadingText}</DotPulse>
        </div>
      );
    }
    if (data?.success) {
      return (
        <div className="h-full">
          {modelAbilityConfig.resultPanel((data?.data ?? {}) as AbilityApiResult)}
        </div>
      );
    }
    return (
      <div className="mt-[150px] flex items-center justify-center">
        <Empty image={Empty.PRESENTED_IMAGE_DEFAULT} />
      </div>
    );
  };
  const onFinish: ProFormProps['onFinish'] = async (values) => {
    const { extraConfig, kwargs, ...reset } = values;
    let finalKwargs = {
      ...transformFormList(extraConfig),
      ...transformKwargs(kwargs),
    };

    const doRequest = async ({ model_uid }: InstanceDetail) => {
      let uuid;
      if (showProgress) {
        uuid = generateUUID();
        finalKwargs.request_id = uuid;
        progressRef.current[model_uid] = 0;
      }

      setLoadingMap((prev) => ({ ...prev, [model_uid]: true }));
      let payload = {
        ...reset,
        replica_id: (showCompare ? undefined : replicaId) || undefined,
        model: model_uid,
      };
      if (!!modelAbilityConfig?.transformValues) {
        payload = modelAbilityConfig.transformValues({
          ...payload,
          kwargs: finalKwargs,
        });
      } else {
        payload = {
          ...payload,
          ...(isEmpty(finalKwargs) ? {} : { kwargs: JSON.stringify(finalKwargs) }),
        };
      }

      request(modelAbilityConfig.requestApi, {
        method: 'post',
        data: payload,
      })
        .then((res) => {
          setResults((prev) => ({
            ...prev,
            [model_uid]: res,
          }));
          if (!res.success) {
            cancel();
          }
        })
        .finally(() => {
          setLoadingMap((prev) => ({ ...prev, [model_uid]: false }));
        });

      if (uuid) {
        await sleep(1000);
        getProgress({ uuid, modelId: model_uid });
      }
    };
    if (showCompare) {
      await Promise.allSettled(compareData.map(doRequest));
    } else {
      await doRequest(instanceDetail);
    }
  };
  const resetFields = () => {
    form.resetFields();
  };
  useImperativeHandle(ref, () => ({
    resetFields,
  }));

  return (
    <div className="p-[20px]">
      <Row gutter={[20, 20]}>
        {!!modelAbilityConfig.formPanel && (
          <Col {...(showCompare ? fullCol : halfCol)}>
            <ProForm
              grid={true}
              submitter={modelAbilityConfig.submitterConfig}
              form={form}
              loading={loading}
              onFinish={onFinish}
              className={classNames('border p-[20px] rounded-[6px]', {
                'min-h-[calc(100vh-264px)]': !showCompare,
              })}
            >
              {modelAbilityConfig.formPanel}
              {modelAbilityConfig?.showExtraConfig && (
                <>
                  <Divider />
                  <ProFormList
                    initialValue={[{}]}
                    actionRef={extraConfigRef}
                    name="extraConfig"
                    copyIconProps={false}
                    creatorButtonProps={false}
                    actionRender={() => []}
                    label={
                      <div className="flex items-center gap-2">
                        {l('model.running.extendParams')}
                        <Button
                          className="h-7"
                          size="small"
                          onClick={() => extraConfigRef.current?.add?.({})}
                          icon={<Plus size={14} />}
                        >
                          {l('global.actions.add')}
                        </Button>
                      </div>
                    }
                  >
                    {(_, index) => {
                      return (
                        <div className="flex">
                          <div className="flex-1 flex">
                            <ProFormText name="key" placeholder="key" colProps={{ span: 12 }}>
                              <AutoComplete
                                options={MODEL_KWARGS_OPTIONS?.[selectModelAbility] || []}
                                placeholder="key"
                                allowClear
                              />
                            </ProFormText>
                            <ProFormText name="value" placeholder="value" colProps={{ span: 12 }} />
                          </div>
                          <IconButton
                            onClick={() => extraConfigRef.current?.remove(index)}
                            className="hover:bg-background rounded-md group mt-[4px] !h-6 !w-6"
                          >
                            <Trash2 size={16} className="group-hover:text-danger text-muted" />
                          </IconButton>
                        </div>
                      );
                    }}
                  </ProFormList>
                </>
              )}
            </ProForm>
          </Col>
        )}
        {compareData.map((item) => (
          <Col xxl={12} xl={12} lg={24} md={24} sm={24} xs={24} key={item.model_uid}>
            <div className="border rounded-[6px]  min-h-[calc(100vh-264px)] flex flex-col">
              <div className="font-bold border-b py-[10px] px-[20px] shrink-0">
                {l('model.running.result')}
                {showCompare ? `(${item.model_uid})` : ''}
              </div>
              <div className="p-[20px] grow flex-1">{renderRightPanel(item)}</div>
            </div>
          </Col>
        ))}
      </Row>
    </div>
  );
});
export default ModelAbilityCom;
