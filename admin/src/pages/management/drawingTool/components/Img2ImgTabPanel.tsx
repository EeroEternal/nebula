import { InputNumberWithSlider, RadioButtonGroup } from '@/components';
import {
  BATCH_SIZE_OPTIONS,
  N_ITER,
  IMAGE_SIZE,
  STEPS,
  CFG_SCALE,
  RESIZE_MODE_INT,
  DENOISING_STRENGTH,
  MASK_BLUR,
  INPAINT_FULL_RES_PADDING,
  INPAINTING_MASK_INVERT,
  SCHEDULER_OPTIONS,
  ControlNetField,
} from '@/constants/sdWebui';
import { generateUUID, sleep } from '@/utils';
import { l, lGet } from '@/utils/intl';
import request from '@/utils/request';
import {
  ProFormDigit,
  ProFormGroup,
  ProFormSegmented,
  ProFormSelect,
  ProFormField,
  ProFormCheckbox,
} from '@ant-design/pro-components';
import { useModel } from '@umijs/max';
import { useRequest } from 'ahooks';
import { Form } from 'antd';
import { FC, useRef } from 'react';
import ExtendCollapse from './ExtendCollapse';
import GenerateResult from './GenerateResult';
import ImageEditor from './ImageEditor';
import Prompt from './Prompt';
import Seed from './Seed';
import UploadImgDragger from './UploadImgDragger';

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
    (data) => request(`${window.DOMAIN_API_RAW}/sdapi/v1/img2img`, { method: 'post', data }),
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
      submit({
        ...reset,
        request_id: uuid,
        init_images: [reset.init_images],
        alwayson_scripts: {
          ADetailer: {
            args: reset.alwayson_scripts.ADetailer.args[0] ? reset.alwayson_scripts.ADetailer.args : [false],
          },
          ControlNet: {
            args: reset.alwayson_scripts.ControlNet.args.filter((item: ControlNetField) => item.enabled),
          },
        },
      });
      await sleep(1000);
      getProgress(uuid);
    });
  };

  return (
    <>
      <Prompt onGenerate={handleSubmit} loading={loading}/>
      <div className="flex items-start">
        <div className="overflow-y-auto w-1/2 mr-[20px]">
          <ProFormGroup>
            <ProFormField name="init_images" rules={[{required: true, message: lGet('management.drawingTool.image.ruleError') }]}>
              <ImageEditor />
            </ProFormField>
            <div className="flex w-full h-[200px] mb-[10px]">
              <ProFormField name="mask" noStyle colProps={{ span: 24 }}>
                <UploadImgDragger
                  title={l('management.drawingTool.image.title')}
                  description={l('management.drawingTool.image.desc')}
                />
              </ProFormField>
            </div>

            <ProFormField
              name="resize_mode"
              label={l('management.drawingTool.controlNet.resizeMode')}
            >
              <RadioButtonGroup options={RESIZE_MODE_INT} />
            </ProFormField>
            <ProFormField
              name="inpainting_mask_invert"
              label={l('management.drawingTool.inpaintingMaskInvert')}
            >
              <RadioButtonGroup options={INPAINTING_MASK_INVERT} />
            </ProFormField>
            <ProFormField name="mask_blur">
              <InputNumberWithSlider
                {...MASK_BLUR}
                label={l('management.drawingTool.adetailer.inpainting.maskBlur')}
              />
            </ProFormField>

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
            <ProFormField name="steps">
              <InputNumberWithSlider
                {...STEPS}
                label={l('management.drawingTool.steps')}
                tooltip={l('management.drawingTool.stepsTips')}
              />
            </ProFormField>
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
            <ProFormField
              name="width"
              colProps={{ span: 12 }}
            >
              <InputNumberWithSlider {...IMAGE_SIZE} label={l('management.drawingTool.width')} />
            </ProFormField>
            <ProFormField
              name="height"
              colProps={{ span: 12 }}
            >
              <InputNumberWithSlider {...IMAGE_SIZE} label={l('management.drawingTool.height')} />
            </ProFormField>

            <ProFormField name="cfg_scale">
              <InputNumberWithSlider
                {...CFG_SCALE}
                label={l('management.drawingTool.cfgScale')}
                tooltip={l('management.drawingTool.cfgScaleTips')}
              />
            </ProFormField>
            <ProFormField name="denoising_strength">
              <InputNumberWithSlider
                {...DENOISING_STRENGTH}
                label={l('management.drawingTool.denoisingStrength')}
                tooltip={l('management.drawingTool.denoisingStrengthTips')}
              />
            </ProFormField>

            <ProFormCheckbox name="inpaint_full_res">
              {l('management.drawingTool.inpaintFullRes')}
            </ProFormCheckbox>
            <ProFormField
              name="inpaint_full_res_padding"
              colProps={{ span: 24 }}
            >
              <InputNumberWithSlider
                {...INPAINT_FULL_RES_PADDING}
                label={l('management.drawingTool.inpaintFullResPadding')}
              />
            </ProFormField>

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