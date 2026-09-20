import { InputNumberWithSlider } from '@/components';
import {
  DEFAULT_HR_UPSCALER_OPTIONS,
  DENOISING_STRENGTH,
  HR_SCALE,
  HR_SCALE_METHOD,
  HR_SECOND_PASS_STEPS,
} from '@/constants/sdWebui';
import { l } from '@/utils/intl';
import {
  ProFormCheckbox,
  ProFormGroup,
  ProFormSelect,
  ProFormField,
} from '@ant-design/pro-components';
import { useSetState } from 'ahooks';
import { Form } from 'antd';
import { FC } from 'react';
import { useModel } from '@umijs/max';

interface HrScaleState {
  hrScaleMethod: string;
}

const HrScale: FC = () => {
  const { form, hrUpscaler } = useModel('management.drawingTool.model');
  const enableHrValue = Form.useWatch('enable_hr', form);
  const widthValue = Form.useWatch('width', form);
  const heightValue = Form.useWatch('height', form);
  const hrScaleValue = Form.useWatch('hr_scale', form);
  // const hrResizeWidhtValue = Form.useWatch('hr_resize_width', form);
  // const hrResizeHeightValue = Form.useWatch('hr_resize_height', form);
  const [{ hrScaleMethod }, setState] = useSetState<HrScaleState>({
    hrScaleMethod: HR_SCALE_METHOD[0].value,
  });

  const hrUpscalerOptions = [...new Set(DEFAULT_HR_UPSCALER_OPTIONS.concat(hrUpscaler))];
  // const handleHrScaleMethodChange = (value: string) => {
  //   setState({ hrScaleMethod: value });
  //   if (value === HR_SCALE_METHOD[1].value) {
  //     form.setFieldsValue({
  //       hr_resize_width: Math.floor(widthValue * hrScaleValue),
  //       hr_resize_height: Math.floor(heightValue * hrScaleValue),
  //     });
  //   }
  // };
  return (
    <div className="border w-full rounded-[6px] mb-[10px] p-[8px] mx-[4px]">
      <ProFormCheckbox noStyle={!enableHrValue} name="enable_hr">
        {l('management.drawingTool.enableHr')}
      </ProFormCheckbox>
      {enableHrValue && (
        <>
          <ProFormSelect
            label={l('management.drawingTool.hrUpscaler')}
            name="hr_upscaler"
            initialValue={hrUpscalerOptions[0]}
            options={hrUpscalerOptions}
            allowClear={false}
          />
          <ProFormGroup>
            <ProFormField
              name="hr_second_pass_steps"
              initialValue={HR_SECOND_PASS_STEPS.defaultValue}
              colProps={{ span: 12 }}
            >
              <InputNumberWithSlider
                {...HR_SECOND_PASS_STEPS}
                label={l('management.drawingTool.hrSecondPassSteps')}
                tooltip={l('management.drawingTool.hrSecondPassStepsTips')}
              />
            </ProFormField>
            <ProFormField
              name="denoising_strength"
              initialValue={DENOISING_STRENGTH.defaultValue}
              colProps={{ span: 12 }}
            >
              <InputNumberWithSlider
                {...DENOISING_STRENGTH}
                label={l('management.drawingTool.denoisingStrength')}
                tooltip={l('management.drawingTool.denoisingStrengthTips')}
              />
            </ProFormField>
          </ProFormGroup>
          <ProFormField
            name="hr_scale"
            initialValue={HR_SCALE.defaultValue}
            extra={l('management.drawingTool.hrScaleTips', undefined, {
              default: `${widthValue}*${heightValue}`,
              newValue: `${Math.floor(widthValue * hrScaleValue)}*${Math.floor(
                heightValue * hrScaleValue,
              )}`,
            })}
          >
            <InputNumberWithSlider {...HR_SCALE} label={l('management.drawingTool.hrScale')} />
          </ProFormField>
          {/* <Segmented
            options={HR_SCALE_METHOD}
            block
            onChange={handleHrScaleMethodChange}
            className="mb-[10px]"
          />
          {hrScaleMethod === HR_SCALE_METHOD[0].value ? (
            <ProFormText name="hr_scale" initialValue={HR_SCALE_VALUE} colProps={{ span: 12 }}>
              <InputNumberWithSlider
                min={HR_SCALE_MIN}
                max={HR_SCALE_MAX}
                precision={2}
                step={0.05}
                label={l('management.drawingTool.hrScale')}
              />
            </ProFormText>
          ) : (
            <ProFormGroup>
              <ProFormText
                name="hr_resize_width"
                initialValue={HR_RESIZE_VALUE}
                colProps={{ span: 12 }}
              >
                <InputNumberWithSlider
                  min={HR_RESIZE_MIN}
                  max={HR_RESIZE_MAX}
                  label={l('management.drawingTool.adjustWidthTo')}
                />
              </ProFormText>
              <ProFormText
                name="hr_resize_height"
                initialValue={HR_RESIZE_VALUE}
                colProps={{ span: 12 }}
              >
                <InputNumberWithSlider
                  min={HR_RESIZE_MIN}
                  max={HR_RESIZE_MAX}
                  label={l('management.drawingTool.adjustHeightTo')}
                />
              </ProFormText>
            </ProFormGroup>
          )}
          <div className="text-[#8c8c8c]">
            {l('management.drawingTool.hrScaleTips', undefined, {
              default: `${widthValue}*${heightValue}`,
              newValue:
                hrScaleMethod === HR_SCALE_METHOD[0].value
                  ? `${Math.floor(widthValue * hrScaleValue)}*${Math.floor(
                      heightValue * hrScaleValue,
                    )}`
                  : `${hrResizeWidhtValue}*${hrResizeHeightValue}`,
            })}
          </div> */}
        </>
      )}
    </div>
  );
};
export default HrScale;
