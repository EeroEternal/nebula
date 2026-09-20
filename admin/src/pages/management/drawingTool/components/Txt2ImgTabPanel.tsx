import { InputNumberWithSlider } from '@/components';
import {
  BATCH_SIZE_OPTIONS,
  INIT_SAMPLER,
  N_ITER,
  IMAGE_SIZE,
  STEPS,
  CFG_SCALE,
  SCHEDULER_OPTIONS,
  ControlNetField,
} from '@/constants/sdWebui';
import { generateUUID, sleep } from '@/utils';
import { l } from '@/utils/intl';
import request from '@/utils/request';
import {
  ProFormDigit,
  ProFormGroup,
  ProFormSegmented,
  ProFormSelect,
  ProFormField,
} from '@ant-design/pro-components';
import { useModel } from '@umijs/max';
import { useRequest } from 'ahooks';
import { Form } from 'antd';
import { FC, useRef } from 'react';
import ExtendCollapse from './ExtendCollapse';
import GenerateResult from './GenerateResult';
import HrScale from './HrScale';
import Prompt from './Prompt';
import Seed from './Seed';

const Txt2ImgTabPanel: FC = () => {
  const { form, samplersOptions } = useModel('management.drawingTool.model');
  const batchSize = Form.useWatch('batch_size', form);
  const nIter = Form.useWatch('n_iter', form);
  const progressRef = useRef(0);

  const { run: getProgress, cancel } = useRequest(
    (uuid) => request(`${window.DOMAIN_API_RAW}/sdapi/v1/progress?request_id=${uuid}`),
    {
      manual: true,
      pollingInterval: 1000,
      onSuccess: (res) => {
        if (!res.success || res.data?.progress === 1) cancel();
        if (res.success) {
          progressRef.current = res?.data?.progress || 0;
        }
      },
      onError: () => {
        cancel();
      },
    },
  );
  const {
    loading,
    data: generateResult,
    run: submit,
  } = useRequest(
    (data) => request(`${window.DOMAIN_API_RAW}/sdapi/v1/txt2img`, { method: 'post', data }),
    {
      manual: true,
      onSuccess: () => {
        cancel();
      },
      onError: () => cancel(),
    },
  );

  const handleSubmit = () => {
    form.validateFields().then(async (values) => {
      const { seedAdvancedSet, ...reset } = values;
      progressRef.current = 0;
      const uuid = generateUUID();
      // console.log(values, {
      //   ...reset,
      //   request_id: uuid,
      //   alwayson_scripts: {
      //     ADetailer: {
      //       args: reset.alwayson_scripts.ADetailer.args[0]
      //         ? reset.alwayson_scripts.ADetailer.args
      //         : [false],
      //     },
      //     ControlNet: {
      //       args: reset.alwayson_scripts.ControlNet.args.filter(
      //         (item: ControlNetField) => item.enabled,
      //       ),
      //     },
      //   },
      // })
      submit({
        ...reset,
        request_id: uuid,
        alwayson_scripts: {
          ADetailer: {
            args: reset.alwayson_scripts.ADetailer.args[0]
              ? reset.alwayson_scripts.ADetailer.args
              : [false],
          },
          ControlNet: {
            args: reset.alwayson_scripts.ControlNet.args.filter(
              (item: ControlNetField) => item.enabled,
            ),
          },
        },
      });
      await sleep(1000);
      getProgress(uuid);
    });
  };

  return (
    <>
      <Prompt onGenerate={handleSubmit} loading={loading} />
      <div className="flex items-start">
        <div className="overflow-y-auto w-1/2 mr-[20px]">
          <ProFormGroup>
            <ProFormSelect
              label={l('management.drawingTool.sampler')}
              tooltip={l('management.drawingTool.samplerTips')}
              name="sampler_name"
              options={samplersOptions}
              colProps={{ span: 12 }}
            />
            <ProFormSelect
              label={l('management.drawingTool.scheduler')}
              name="scheduler"
              options={SCHEDULER_OPTIONS}
              colProps={{ span: 12 }}
            />
            <ProFormSegmented
              name="batch_size"
              label={l('management.drawingTool.batchSize')}
              colProps={{ span: 24 }}
              fieldProps={{
                block: true,
                options: BATCH_SIZE_OPTIONS,
              }}
            />
            <ProFormDigit
              name="n_iter"
              label={l('management.drawingTool.nIter')}
              tooltip={l('management.drawingTool.nIterTips', undefined, {
                batchSize,
                nIter,
                all: `${batchSize}x${nIter}`,
              })}
              {...N_ITER}
              fieldProps={{
                precision: 0,
              }}
            />

            <ProFormField name="width" colProps={{ span: 12 }}>
              <InputNumberWithSlider {...IMAGE_SIZE} label={l('management.drawingTool.width')} />
            </ProFormField>
            <ProFormField name="height" colProps={{ span: 12 }}>
              <InputNumberWithSlider {...IMAGE_SIZE} label={l('management.drawingTool.height')} />
            </ProFormField>
            <ProFormField name="steps" colProps={{ span: 12 }}>
              <InputNumberWithSlider
                {...STEPS}
                label={l('management.drawingTool.steps')}
                tooltip={l('management.drawingTool.stepsTips')}
              />
            </ProFormField>
            <ProFormField name="cfg_scale" colProps={{ span: 12 }}>
              <InputNumberWithSlider
                {...CFG_SCALE}
                label={l('management.drawingTool.cfgScale')}
                tooltip={l('management.drawingTool.cfgScaleTips')}
              />
            </ProFormField>
            {/* 启用高分辨率 */}
            <HrScale />
            {/* 随机数种子 */}
            <Seed />
            <ExtendCollapse />
          </ProFormGroup>
        </div>
        <GenerateResult
          loading={loading}
          progress={progressRef.current}
          result={generateResult?.data}
        />
      </div>
    </>
  );
};
export default Txt2ImgTabPanel;
