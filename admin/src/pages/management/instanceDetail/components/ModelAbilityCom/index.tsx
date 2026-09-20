import type { ProFormProps } from '@ant-design/pro-components';
import { ProForm, ProCard } from '@ant-design/pro-components';
import { useModel } from '@umijs/max';
import { useRequest } from 'ahooks';
import { Col, Form, Row } from 'antd';
import { isArray, isEmpty, omitBy, size } from 'lodash';
import React, { forwardRef, useImperativeHandle, useMemo, useRef, useState } from 'react';
import { ModelAbility } from '@/constants/modelData';
import type { InstanceDetail } from '@/types/Public/data';
import { formatProgress, generateUUID, sleep, transformValueType } from '@/utils';
import { l } from '@/utils/intl';
import request from '@/utils/request';

import TryToAPI from '../TryToAPI';
import modelAbilityConfig from './modelAbilityConfig';
import type { ModelAbilityConfig } from './modelAbilityConfig';
import FormPanels from './FormPanels';
import ResultPanels from './ResultPanels';
import type { AbilityRunResult } from '../../abilityFormTypes';

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
  const { FormPanel, ResultPanel, submitterConfig, loadingText, requestApi, transformValues } =
    useMemo(() => {
      const config = modelAbilityConfig[selectModelAbility];
      return {
        ...config,
        submitterConfig:
          config?.submitterConfig !== false
            ? {
                ...config?.submitterConfig,
                searchConfig: {
                  ...config?.submitterConfig?.searchConfig,
                  submitText: config?.submitterConfig?.searchConfig?.submitText
                    ? l(config?.submitterConfig?.searchConfig.submitText)
                    : l('models.instances.detail.generate'),
                },
                submitButtonProps: {
                  size: 'large',
                  block: true,
                  ...config?.submitterConfig?.submitButtonProps,
                },
                // 去掉「重置」；Try To API 下移到提交行右侧
                resetButtonProps: false,
                render: (_props: unknown, doms: React.ReactNode) => (
                  <div className="flex w-full items-center gap-2">
                    <div className="min-w-0 flex-1">{doms}</div>
                    <div className="shrink-0">
                      <TryToAPI />
                    </div>
                  </div>
                ),
              }
            : false,
      };
    }, [modelAbilityConfig, selectModelAbility]);

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
    if (loading) {
      return (
        <ResultPanels.Loading
          label={loadingText}
          progress={showProgress ? formatProgress(progress * 100) : undefined}
        />
      );
    }
    if (data?.success) {
      const Panel = ResultPanel as React.ComponentType<{ result: unknown }>;
      return <Panel result={data.data} />;
    }
    return <ResultPanels.Default />;
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
      if (!!transformValues) {
        payload = transformValues({
          ...payload,
          kwargs: finalKwargs,
        });
      } else {
        payload = {
          ...payload,
          ...(isEmpty(finalKwargs) ? {} : { kwargs: JSON.stringify(finalKwargs) }),
        };
      }

      request(requestApi, {
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
  if (!FormPanel) {
    return (
      <ProCard className="h-[calc(100dvh-420px)] max-h-[calc(100dvh-420px)] min-h-[320px]">
        <FormPanels.Empty />
      </ProCard>
    );
  }
  return (
    <Row gutter={[24, 24]}>
      <Col {...(showCompare ? fullCol : halfCol)}>
        <ProCard
          headerBordered
          title={l(`global.model.ability.${selectModelAbility}`)}
          bodyStyle={{ overflowY: 'auto', height: showCompare ? 'auto' : 'calc(100vh - 288px)' }}
        >
          <ProForm
            grid={true}
            submitter={submitterConfig as ModelAbilityConfig['submitterConfig']}
            form={form}
            loading={loading}
            onFinish={onFinish}
          >
            <FormPanel form={form} instanceDetail={instanceDetail} />
          </ProForm>
        </ProCard>
      </Col>
      {compareData.map((item) => (
        <Col xxl={12} xl={12} lg={24} md={24} sm={24} xs={24} key={item.model_uid}>
          <ProCard
            headerBordered
            className="relative"
            title={`${l('models.instances.detail.result')}${
              showCompare ? `(${item.model_uid})` : ''
            }`}
            bodyStyle={{ height: 'calc(100vh - 288px)', overflowY: 'auto' }}
          >
            {renderRightPanel(item)}
          </ProCard>
        </Col>
      ))}
    </Row>
  );
});
export default ModelAbilityCom;
