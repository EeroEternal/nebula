import { InputNumberWithSlider, RadioButtonGroup } from '@/components';
import {
  CONTROL_GUIDANCE_END,
  CONTROL_GUIDANCE_START,
  CONTROL_MODE,
  CONTROL_PREPROCESSOR_RESOLUTION,
  CONTROL_THRESHOLD_MAP,
  CONTROL_WEIGHT,
  HR_OPTION,
  RESIZE_MODE_STR,
  SHOW_THRESHOLD,
} from '@/constants/sdWebui';
import { l, lGet } from '@/utils/intl';
import request from '@/utils/request';
import { RocketOutlined, CloseOutlined, PictureOutlined } from '@ant-design/icons';
import {
  ProFormCheckbox,
  ProFormField,
  ProFormGroup,
  ProFormSelect,
  ProFormSwitch,
} from '@ant-design/pro-components';
import { useModel } from '@umijs/max';
import { useRequest, useSetState } from 'ahooks';
import {
  Button,
  Checkbox,
  CheckboxProps,
  Col,
  Form,
  RadioGroupProps,
  Row,
  Image,
  Skeleton,
} from 'antd';
import { get, isArray, isEmpty } from 'lodash';
import { FC, useEffect } from 'react';
import ImageEditor from './ImageEditor';
import UploadImgDragger from './UploadImgDragger';

interface ControlNetUnitProps {
  index: number;
}
interface ControlNetUnitState {
  allowPreview: boolean;
  uploadMask: boolean;
  controlType?: string;
  previewImgUrl?: string;
}
const ControlNetUnit: FC<ControlNetUnitProps> = ({ index }) => {
  const { controlTypes, form } = useModel('management.drawingTool.model');
  const controlTypeOptions = Object.keys(controlTypes || {});
  const enabledValue = Form.useWatch(['alwayson_scripts', 'ControlNet', 'args', index, 'enabled'], form)
  // 预处理器
  const moduleValue = Form.useWatch(
    ['alwayson_scripts', 'ControlNet', 'args', index, 'module'],
    form,
  );
  // 完美像素（若勾选，则隐藏预处理器分辨率）
  const pixelPerfectValue = Form.useWatch(
    ['alwayson_scripts', 'ControlNet', 'args', index, 'pixel_perfect'],
    form,
  );
  const [{ allowPreview, uploadMask, controlType, previewImgUrl }, setState] =
    useSetState<ControlNetUnitState>({
      allowPreview: false,
      uploadMask: false,
      controlType: undefined,
      previewImgUrl: undefined,
    });

  const handleAllowPreview: CheckboxProps['onChange'] = (e) => {
    setState({ allowPreview: e.target.checked });
  };
  const handleClosePreview = () => {
    setState({ allowPreview: false, previewImgUrl: undefined });
  };
  const handleUploadMask: CheckboxProps['onChange'] = (e) => {
    setState({ uploadMask: e.target.checked });
  };
  const handleControlType: RadioGroupProps['onChange'] = (e) => {
    const value = e.target.value;
    setState({ controlType: value });
    form.setFieldValue(
      ['alwayson_scripts', 'ControlNet', 'args', index, 'model'],
      controlTypes[value]?.default_model,
    );
    form.setFieldValue(
      ['alwayson_scripts', 'ControlNet', 'args', index, 'module'],
      controlTypes[value]?.default_option,
    );
  };
  const { run, loading: detectLoading } = useRequest(
    (data) => request(`${window.DOMAIN_API_RAW}/controlnet/detect`, { method: 'post', data }),
    {
      manual: true,
      onSuccess: (res) => {
        setState({
          previewImgUrl: isArray(res?.data?.images)
            ? `data:image/png;base64,${res.data.images[0]}`
            : undefined,
        });
      },
    },
  );

  const handleRunPreview = () => {
    setState({ allowPreview: true, previewImgUrl: undefined });
    const { module, image, processor_res, controlnet_threshold_a, controlnet_threshold_b } =
      form.getFieldValue(['alwayson_scripts', 'ControlNet', 'args', index]);
    
    run({
      controlnet_module: module,
      controlnet_input_images: image ? [image] : [],
      controlnet_images: image ? [image] : [],
      controlnet_processor_res: processor_res,
      controlnet_threshold_a: controlnet_threshold_a,
      controlnet_threshold_b: controlnet_threshold_b,
    });
  };
  const handlePixelPerfect: CheckboxProps['onChange'] = (e) => {
    if (e.target.checked) {
      form.setFieldValue(
        ['alwayson_scripts', 'ControlNet', 'args', index, 'processor_res'],
        CONTROL_PREPROCESSOR_RESOLUTION.defaultValue,
      );
    }
  };
  useEffect(() => {
    if (!isEmpty(controlTypes)) {
      const initControlType = Object.keys(controlTypes || {})?.[0];
      setState({
        controlType: initControlType,
      });
      if (initControlType) {
        form.setFieldValue(
          ['alwayson_scripts', 'ControlNet', 'args', index, 'model'],
          controlTypes[initControlType]?.default_model,
        );
        form.setFieldValue(
          ['alwayson_scripts', 'ControlNet', 'args', index, 'module'],
          controlTypes[initControlType]?.default_option,
        );
      }
    }
  }, [controlTypes]);
  return (
    <div>
      <ProFormField
        name={['alwayson_scripts', 'ControlNet', 'args', index, 'image']}
        colProps={{ span: 24 }}
        rules={[{required: enabledValue, message: lGet('management.drawingTool.controlNet.image.ruleError') }]}
      >
        <ImageEditor />
      </ProFormField>
      {(allowPreview || uploadMask) && (
        <div className="flex h-[200px] mb-[10px]">
          {allowPreview && (
            <ProFormField colProps={{ span: uploadMask ? 12 : 24 }} noStyle>
              <div className="w-full h-full rounded-[8px] p-[10px] border border-dashed relative flex flex-col items-center justify-center">
                <div className="absolute z-[100]  p-[4px] rounded-[8px] left-0 top-0 text-[#b0b0b0]">
                  Preview
                </div>
                {detectLoading ? (
                  <Skeleton.Button active block size="small" />
                ) : previewImgUrl ? (
                  <Image src={previewImgUrl} height={150} className="object-contain" />
                ) : (
                  <PictureOutlined className="text-[30px] text-[#b0b0b0]" />
                )}
              </div>
              <Button
                className="absolute  bottom-[10px] right-[10px] z-[100]"
                icon={<CloseOutlined />}
                size="small"
                onClick={handleClosePreview}
              />
            </ProFormField>
          )}
          {uploadMask && (
            <ProFormField
              name={['alwayson_scripts', 'ControlNet', 'args', index, 'mask']}
              colProps={{ span: allowPreview ? 12 : 24 }}
              noStyle
            >
              <UploadImgDragger
                title={l('management.drawingTool.image.title')}
                description={l('management.drawingTool.image.desc')}
              />
            </ProFormField>
          )}
        </div>
      )}

      <Row className="mb-[8px]">
        <ProFormCheckbox
          name={['alwayson_scripts', 'ControlNet', 'args', index, 'enabled']}
          noStyle
          colProps={{ span: 6 }}
        >
          {l('global.actions.enable')}
        </ProFormCheckbox>
        <ProFormCheckbox
          name={['alwayson_scripts', 'ControlNet', 'args', index, 'pixel_perfect']}
          noStyle
          colProps={{ span: 6 }}
          fieldProps={{
            onChange: handlePixelPerfect,
          }}
        >
          {l('management.drawingTool.controlNet.pixelPerfect')}
        </ProFormCheckbox>
        <Col span={6}>
          <Checkbox onChange={handleAllowPreview} checked={allowPreview}>
            {l('management.drawingTool.controlNet.allowPreview')}
          </Checkbox>
        </Col>
        <Col span={6}>
          <Checkbox onChange={handleUploadMask} checked={uploadMask}>
            {l('management.drawingTool.controlNet.uploadMask')}
          </Checkbox>
        </Col>
      </Row>
      <div className="ant-form-item-label">{l('management.drawingTool.controlNet.controlType')}</div>
      <RadioButtonGroup
        className="mb-[8px]"
        options={controlTypeOptions}
        value={controlType}
        onChange={handleControlType}
      />

      <div className="flex items-center">
        <ProFormSelect
          label={l('management.drawingTool.controlNet.module')}
          options={controlType ? get(controlTypes, [controlType, 'module_list'], []) : []}
          name={['alwayson_scripts', 'ControlNet', 'args', index, 'module']}
          colProps={{ span: 9 }}
        />
        <Button
          loading={detectLoading}
          className="mt-[21px] w-1/4"
          type="primary"
          icon={<RocketOutlined />}
          onClick={handleRunPreview}
        >
          {l('management.drawingTool.controlNet.runPreview')}
        </Button>
        <ProFormSelect
          label={l('management.drawingTool.controlNet.model')}
          options={controlType ? get(controlTypes, [controlType, 'model_list'], []) : []}
          name={['alwayson_scripts', 'ControlNet', 'args', index, 'model']}
          colProps={{ span: 9 }}
        />
      </div>
      <ProFormGroup>
        <ProFormField
          name={['alwayson_scripts', 'ControlNet', 'args', index, 'weight']}
          colProps={{ span: 8 }}
        >
          <InputNumberWithSlider
            {...CONTROL_WEIGHT}
            label={l('management.drawingTool.controlNet.weight')}
          />
        </ProFormField>
        <ProFormField
          name={['alwayson_scripts', 'ControlNet', 'args', index, 'guidance_start']}
          colProps={{ span: 8 }}
        >
          <InputNumberWithSlider
            {...CONTROL_GUIDANCE_START}
            label={l('management.drawingTool.controlNet.guidanceStart')}
          />
        </ProFormField>
        <ProFormField
          name={['alwayson_scripts', 'ControlNet', 'args', index, 'guidance_end']}
          colProps={{ span: 8 }}
        >
          <InputNumberWithSlider
            {...CONTROL_GUIDANCE_END}
            label={l('management.drawingTool.controlNet.guidanceEnd')}
          />
        </ProFormField>
      </ProFormGroup>

      <ProFormField
        name={['alwayson_scripts', 'ControlNet', 'args', index, 'processor_res']}
      >
        <InputNumberWithSlider
          {...CONTROL_PREPROCESSOR_RESOLUTION}
          label={l('management.drawingTool.controlNet.preprocessorResolution')}
          disabled={pixelPerfectValue}
        />
      </ProFormField>
      {SHOW_THRESHOLD.includes(moduleValue) && (
        <>
          <ProFormField
            name={['alwayson_scripts', 'ControlNet', 'args', index, 'threshold_a']}
            initialValue={CONTROL_THRESHOLD_MAP[moduleValue].low.value}
          >
            <InputNumberWithSlider
              min={CONTROL_THRESHOLD_MAP[moduleValue].low.min}
              max={CONTROL_THRESHOLD_MAP[moduleValue].low.max}
              label={CONTROL_THRESHOLD_MAP[moduleValue].low.label}
            />
          </ProFormField>
          <ProFormField
            name={['alwayson_scripts', 'ControlNet', 'args', index, 'threshold_b']}
            initialValue={CONTROL_THRESHOLD_MAP[moduleValue].high.value}
          >
            <InputNumberWithSlider
              min={CONTROL_THRESHOLD_MAP[moduleValue].high.min}
              max={CONTROL_THRESHOLD_MAP[moduleValue].high.max}
              label={CONTROL_THRESHOLD_MAP[moduleValue].high.label}
            />
          </ProFormField>
        </>
      )}
      <ProFormSwitch
        name={['alwayson_scripts', 'ControlNet', 'args', index, 'low_vram']}
        label={l('management.drawingTool.controlNet.lowVram')}
      />
      <ProFormField
        name={['alwayson_scripts', 'ControlNet', 'args', index, 'hr_option']}
        label={l('management.drawingTool.controlNet.hrOption')}
      >
        <RadioButtonGroup options={HR_OPTION} />
      </ProFormField>
      <ProFormField
        name={['alwayson_scripts', 'ControlNet', 'args', index, 'control_mode']}
        label={l('management.drawingTool.controlNet.controlMode')}
      >
        <RadioButtonGroup options={CONTROL_MODE} />
      </ProFormField>
      <ProFormField
        name={['alwayson_scripts', 'ControlNet', 'args', index, 'resize_mode']}
        label={l('management.drawingTool.controlNet.resizeMode')}
      >
        <RadioButtonGroup options={RESIZE_MODE_STR} />
      </ProFormField>
    </div>
  );
};
export default ControlNetUnit;
