import { InputNumberWithSlider, RadioButtonGroup } from '@/components';
import {
  ADETAILER_MODEL_OPTIONS,
  AD_CONFIDENCE,
  AD_MASK_K_LARGEST,
  AD_MASK_MIN_RATIO,
  AD_MASK_MAX_RATIO,
  AD_MASK_OFFSET,
  AD_DILATE_ERODE,
  AD_MASK_MERGE_INVERT_OPTIONS,
  MASK_BLUR,
  AD_DENOISING_STRENGTH,
  AD_ONLY_MASKED_PADDING,
  AD_INPAINT_WH,
  AD_INPAINTING_STEPS,
  AD_CFG_SCALE,
  AD_USE_VAE_OPTIONS,
  SCHEDULER_OPTIONS,
  AD_INIT_SAMPLER,
  AD_NOISE_MULTIPLIER,
  AD_CLIP_SKIP,
} from '@/constants/sdWebui';
import { l } from '@/utils/intl';
import { CaretRightOutlined } from '@ant-design/icons';
import {
  ProFormCheckbox,
  ProFormGroup,
  ProFormSelect,
  ProFormText,
  ProFormTextArea,
  ProFormField,
} from '@ant-design/pro-components';
import { Collapse, Form } from 'antd';
import { FC } from 'react';
import { useModel } from '@umijs/max';

interface ADetailerUnitProps {
  index: number;
}
const panelStyle = {
  borderRadius: 8,
  border: '1px solid #d9d9d9',
};
const panelStyles = {
  header: {
    padding: 10,
    fontWeight: 600,
    fontSize: 14,
  },
  body: {
    padding: '0 10px',
  },
};
const baseCollapsePanel = {
  style: panelStyle,
  styles: panelStyles,
  forceRender: true,
};
const ADetailerUnit: FC<ADetailerUnitProps> = ({ index }) => {
  const { samplersOptions, form } = useModel('management.drawingTool.model');
  const onlyMasked = Form.useWatch(
    ['alwayson_scripts', 'ADetailer', 'args', index, 'ad_inpaint_only_masked'],
    form,
  );
  const useInpaintWH = Form.useWatch(
    ['alwayson_scripts', 'ADetailer', 'args', index, 'ad_use_inpaint_width_height'],
    form,
  );
  const useSteps = Form.useWatch(
    ['alwayson_scripts', 'ADetailer', 'args', index, 'ad_use_steps'],
    form,
  );
  const cFGScale = Form.useWatch(
    ['alwayson_scripts', 'ADetailer', 'args', index, 'ad_use_cfg_scale'],
    form,
  );
  const useVAE = Form.useWatch(
    ['alwayson_scripts', 'ADetailer', 'args', index, 'ad_use_vae'],
    form,
  );
  const useSampler = Form.useWatch(
    ['alwayson_scripts', 'ADetailer', 'args', index, 'ad_use_sampler'],
    form,
  );
  const useNoiseMultiplier = Form.useWatch(
    ['alwayson_scripts', 'ADetailer', 'args', index, 'ad_use_noise_multiplier'],
    form,
  );
  const useClipSkip = Form.useWatch(
    ['alwayson_scripts', 'ADetailer', 'args', index, 'ad_use_clip_skip'],
    form,
  );
  const collapseItems = [
    {
      ...baseCollapsePanel,
      key: 'Detection',
      label: l('management.drawingTool.adetailer.detection'),
      children: (
        <ProFormGroup>
          <ProFormField
            name={['alwayson_scripts', 'ADetailer', 'args', index, 'ad_confidence']}
          >
            <InputNumberWithSlider
              {...AD_CONFIDENCE}
              label={l('management.drawingTool.adetailer.detection.confidence')}
            />
          </ProFormField>
          <ProFormField
            name={['alwayson_scripts', 'ADetailer', 'args', index, 'ad_mask_k_largest']}
          >
            <InputNumberWithSlider
              {...AD_MASK_K_LARGEST}
              label={l('management.drawingTool.adetailer.detection.maskKLargest')}
              tooltip={l('management.drawingTool.adetailer.detection.masKLargestTips')}
            />
          </ProFormField>
          <ProFormField
            name={['alwayson_scripts', 'ADetailer', 'args', index, 'ad_mask_min_ratio']}
            colProps={{ span: 12 }}
          >
            <InputNumberWithSlider
              {...AD_MASK_MIN_RATIO}
              label={l('management.drawingTool.adetailer.detection.maskMinRatio')}
            />
          </ProFormField>
          <ProFormField
            name={['alwayson_scripts', 'ADetailer', 'args', index, 'ad_mask_max_ratio']}
            colProps={{ span: 12 }}
          >
            <InputNumberWithSlider
              {...AD_MASK_MAX_RATIO}
              label={l('management.drawingTool.adetailer.detection.maskMaxRatio')}
            />
          </ProFormField>
        </ProFormGroup>
      ),
    },
    {
      ...baseCollapsePanel,
      key: 'MaskPreprocessing',
      label: l('management.drawingTool.adetailer.maskPreprocessing'),
      children: (
        <ProFormGroup>
          <ProFormField
            name={['alwayson_scripts', 'ADetailer', 'args', index, 'ad_x_offset']}
            colProps={{ span: 12 }}
          >
            <InputNumberWithSlider
              {...AD_MASK_OFFSET}
              label={l('management.drawingTool.adetailer.maskPreprocessing.xOffset')}
            />
          </ProFormField>
          <ProFormField
            name={['alwayson_scripts', 'ADetailer', 'args', index, 'ad_y_offset']}
            colProps={{ span: 12 }}
          >
            <InputNumberWithSlider
              {...AD_MASK_OFFSET}
              label={l('management.drawingTool.adetailer.maskPreprocessing.yOffset')}
            />
          </ProFormField>
          <ProFormText
            name={['alwayson_scripts', 'ADetailer', 'args', index, 'ad_dilate_erode']}
          >
            <InputNumberWithSlider
              {...AD_DILATE_ERODE}
              label={l('management.drawingTool.adetailer.maskPreprocessing.dilateErode')}
            />
          </ProFormText>
          <ProFormField
            name={['alwayson_scripts', 'ADetailer', 'args', index, 'ad_mask_merge_invert']}
            label={l('management.drawingTool.adetailer.maskPreprocessing.mergeMode')}
          >
            <RadioButtonGroup options={AD_MASK_MERGE_INVERT_OPTIONS} />
          </ProFormField>
        </ProFormGroup>
      ),
    },
    {
      ...baseCollapsePanel,
      key: 'Inpainting',
      label: l('management.drawingTool.adetailer.inpainting'),
      children: (
        <ProFormGroup>
          <ProFormField
            name={['alwayson_scripts', 'ADetailer', 'args', index, 'ad_mask_blur']}
            colProps={{ span: 12 }}
          >
            <InputNumberWithSlider
              {...MASK_BLUR}
              label={l('management.drawingTool.adetailer.inpainting.maskBlur')}
            />
          </ProFormField>
          <ProFormField
            name={['alwayson_scripts', 'ADetailer', 'args', index, 'ad_denoising_strength']}
            colProps={{ span: 12 }}
          >
            <InputNumberWithSlider
              {...AD_DENOISING_STRENGTH}
              label={l('management.drawingTool.adetailer.inpainting.denoisingStrength')}
            />
          </ProFormField>

          <ProFormCheckbox
            name={['alwayson_scripts', 'ADetailer', 'args', index, 'ad_inpaint_only_masked']}
          >
            {l('management.drawingTool.adetailer.inpainting.onlyMasked')}
          </ProFormCheckbox>
          <ProFormField
            name={
              onlyMasked
                ? ['alwayson_scripts', 'ADetailer', 'args', index, 'ad_inpaint_only_masked_padding']
                : undefined
            }
          >
            <InputNumberWithSlider
              {...AD_ONLY_MASKED_PADDING}
              disabled={!onlyMasked}
              label={l('management.drawingTool.adetailer.inpainting.onlyMaskedPadding')}
            />
          </ProFormField>

          <ProFormCheckbox
            name={['alwayson_scripts', 'ADetailer', 'args', index, 'ad_use_inpaint_width_height']}
          >
            {l('management.drawingTool.adetailer.inpainting.separateWH')}
          </ProFormCheckbox>
          <ProFormField
            name={
              useInpaintWH
                ? ['alwayson_scripts', 'ADetailer', 'args', index, 'ad_inpaint_width']
                : undefined
            }
            // initialValue={AD_INPAINT_WH.defaultValue}
            colProps={{ span: 12 }}
          >
            <InputNumberWithSlider
              {...AD_INPAINT_WH}
              disabled={!useInpaintWH}
              label={l('management.drawingTool.adetailer.inpainting.inpaintWidth')}
            />
          </ProFormField>
          <ProFormField
            name={
              useInpaintWH
                ? ['alwayson_scripts', 'ADetailer', 'args', index, 'ad_inpaint_height']
                : undefined
            }
            // initialValue={AD_INPAINT_WH.defaultValue}
            colProps={{ span: 12 }}
          >
            <InputNumberWithSlider
              {...AD_INPAINT_WH}
              disabled={!useInpaintWH}
              label={l('management.drawingTool.adetailer.inpainting.inpaintHeight')}
            />
          </ProFormField>

          <ProFormField
            name={
              useSteps ? ['alwayson_scripts', 'ADetailer', 'args', index, 'ad_steps'] : undefined
            }
            // initialValue={AD_INPAINTING_STEPS.defaultValue}
          >
            <InputNumberWithSlider
              {...AD_INPAINTING_STEPS}
              label={
                <ProFormCheckbox
                  noStyle
                  name={['alwayson_scripts', 'ADetailer', 'args', index, 'ad_use_steps']}
                >
                  {l('management.drawingTool.adetailer.inpainting.steps')}
                </ProFormCheckbox>
              }
            />
          </ProFormField>

          <ProFormField
            name={
              cFGScale
                ? ['alwayson_scripts', 'ADetailer', 'args', index, 'ad_cfg_scale']
                : undefined
            }
            // initialValue={AD_CFG_SCALE.defaultValue}
          >
            <InputNumberWithSlider
              disabled={!cFGScale}
              {...AD_CFG_SCALE}
              label={
                <ProFormCheckbox
                  noStyle
                  name={['alwayson_scripts', 'ADetailer', 'args', index, 'ad_use_cfg_scale']}
                >
                  {l('management.drawingTool.adetailer.inpainting.useCFGScale')}
                </ProFormCheckbox>
              }
            />
          </ProFormField>
          {/* 
            ad_use_checkpoint: bool = False
            ad_checkpoint: Optional[str] = None
          */}
          <ProFormCheckbox
            name={['alwayson_scripts', 'ADetailer', 'args', index, 'ad_use_vae']}
          >
            {l('management.drawingTool.adetailer.inpainting.useVae')}
          </ProFormCheckbox>
          <ProFormField
            name={useVAE ? ['alwayson_scripts', 'ADetailer', 'args', index, 'ad_vae'] : undefined}
            // initialValue={AD_USE_VAE_OPTIONS[0].value}
          >
            <RadioButtonGroup
              options={AD_USE_VAE_OPTIONS}
              defaultValue={AD_USE_VAE_OPTIONS[0].value}
            />
          </ProFormField>

          <ProFormSelect
            label={
              <ProFormCheckbox
                noStyle
                name={['alwayson_scripts', 'ADetailer', 'args', index, 'ad_use_sampler']}
              >
                {l('management.drawingTool.adetailer.inpainting.useSampler')}
              </ProFormCheckbox>
            }
            name={
              useSampler
                ? ['alwayson_scripts', 'ADetailer', 'args', index, 'ad_sampler']
                : undefined
            }
            options={samplersOptions}
            // initialValue={AD_INIT_SAMPLER}
            disabled={!useSampler}
            fieldProps={{
              defaultValue: AD_INIT_SAMPLER,
            }}
            colProps={{ span: 12 }}
          />
          <ProFormSelect
            label={l('management.drawingTool.adetailer.inpainting.scheduler')}
            name={['alwayson_scripts', 'ADetailer', 'args', index, 'ad_scheduler']}
            options={SCHEDULER_OPTIONS}
            colProps={{ span: 12 }}
          />

          <ProFormField
            name={
              useNoiseMultiplier
                ? ['alwayson_scripts', 'ADetailer', 'args', index, 'ad_noise_multiplier']
                : undefined
            }
            // initialValue={AD_NOISE_MULTIPLIER.defaultValue}
          >
            <InputNumberWithSlider
              disabled={!useNoiseMultiplier}
              {...AD_NOISE_MULTIPLIER}
              label={
                <ProFormCheckbox
                  noStyle
                  name={['alwayson_scripts', 'ADetailer', 'args', index, 'ad_use_noise_multiplier']}
                >
                  {l('management.drawingTool.adetailer.inpainting.useNoiseMultiplier')}
                </ProFormCheckbox>
              }
            />
          </ProFormField>

          <ProFormField
            name={
              useClipSkip
                ? ['alwayson_scripts', 'ADetailer', 'args', index, 'ad_clip_skip']
                : undefined
            }
            // initialValue={AD_CLIP_SKIP.defaultValue}
          >
            <InputNumberWithSlider
              disabled={!useClipSkip}
              {...AD_CLIP_SKIP}
              label={
                <ProFormCheckbox
                  noStyle
                  name={['alwayson_scripts', 'ADetailer', 'args', index, 'ad_use_clip_skip']}
                >
                  {l('management.drawingTool.adetailer.inpainting.useClipSkip')}
                </ProFormCheckbox>
              }
            />
          </ProFormField>

          <ProFormCheckbox
            name={['alwayson_scripts', 'ADetailer', 'args', index, 'ad_restore_face']}
          >
            {l('management.drawingTool.adetailer.inpainting.restoreFace')}
          </ProFormCheckbox>
        </ProFormGroup>
      ),
    },
  ];
  return (
    <div className="bg-card">
      <ProFormSelect
        options={ADETAILER_MODEL_OPTIONS}
        label={l('management.drawingTool.adetailer.model')}
        name={['alwayson_scripts', 'ADetailer', 'args', index, 'ad_model']}
      />
      <ProFormTextArea
        name={['alwayson_scripts', 'ADetailer', 'args', index, 'ad_prompt']}
        placeholder={l('management.drawingTool.adetailer.prompt')}
        allowClear
      />
      <ProFormTextArea
        name={['alwayson_scripts', 'ADetailer', 'args', index, 'ad_negative_prompt']}
        placeholder={l('management.drawingTool.adetailer.negativePrompt')}
        allowClear
      />
      <Collapse
        bordered={false}
        className="w-full flex flex-col gap-y-[10px] bg-card mb-[10px] px-[4px]"
        expandIcon={({ isActive }) => <CaretRightOutlined rotate={isActive ? 90 : 0} />}
        items={collapseItems}
        expandIconPosition="end"
      />
    </div>
  );
};
export default ADetailerUnit;
