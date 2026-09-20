import { useRequest, useSetState } from 'ahooks';
import type { ProFormSelectProps } from '@ant-design/pro-components';
import { Form } from 'antd';
import request from '@/utils/request';
import { SamplersItem, SDModelsItem, LorasItem, UpScalersItem, ControlTypes } from '@/types/sd';

export interface ModelState {
  activeAbilityTabKey: 'txt2img' | 'img2img';
  modelOptions: ProFormSelectProps['options'];
  samplersOptions: string[];
  hrUpscaler: string[];
  controlTypes: ControlTypes,
}
export default () => {
  const [form] = Form.useForm();
  const modelValue = Form.useWatch('model', form);
  const [state, setState] = useSetState<ModelState>({
    activeAbilityTabKey: 'txt2img',
    samplersOptions: [],
    modelOptions: [],
    hrUpscaler: [],
    controlTypes: {},
  });
  // sd models
  const { run: getSdModels, loading: sdModelsLoading } = useRequest(
    () =>
      request<{ data: SDModelsItem[]; success: boolean }>(
        `${window.DOMAIN_API_RAW}/sdapi/v1/sd-models`,
      ),
    {
      manual: true,
      onSuccess: (res) => {
        if (res.success) {
          form.setFieldValue('model', res?.data?.[0]?.model_name);
          setState({
            modelOptions: (res.data || []).map((item) => item.model_name),
          });
        }
      },
    },
  );
  // 采样算法
  const { run: getSamplers, loading: samplersLoading } = useRequest(
    () =>
      request<{ data: SamplersItem[]; success: boolean }>(
        `${window.DOMAIN_API_RAW}/sdapi/v1/samplers`,
      ),
    {
      manual: true,
      onSuccess: (res) => {
        if (res.success) {
          setState({
            samplersOptions: (res.data || []).map((item) => item.name),
          });
        }
      },
    },
  );
  const { run: getLoras, loading: lorasLoading } = useRequest(
    () =>
      request<{ data: LorasItem[]; success: boolean }>(`${window.DOMAIN_API_RAW}/sdapi/v1/loras`),
    {
      manual: true,
    },
  );
  // 放大算法
  const { run: getUpscalers, loading: upscalersLoading } = useRequest(
    () =>
      request<{ data: UpScalersItem[]; success: boolean }>(
        `${window.DOMAIN_API_RAW}/sdapi/v1/upscalers`,
      ),
    {
      manual: true,
      // 后端返回错误格式有问题，如果接口报错，data?.data 为 { detail: 'xxx'}
      onSuccess: (res) => {
        if (res.success) {
          setState({
            hrUpscaler: (res.data || []).map((item) => item.name),
          });
        }
      },
    },
  );
  useRequest(
    () =>
      request<{ data: { control_types: ControlTypes }; success: boolean }>(
        `${window.DOMAIN_API_RAW}/controlnet/control_types`,
      ),
    {
      ready: !!modelValue,
      onSuccess: (res) => {
        if (res.success) {
          setState({
            controlTypes: (res?.data?.control_types || {}) as ControlTypes
          })
        }
      },
    },
  );
  const initLoad = () => {
    getSdModels();
    getSamplers();
    getLoras();
    getUpscalers();
  };
  return {
    form,
    ...state,
    initLoading: sdModelsLoading || samplersLoading || lorasLoading || upscalersLoading,
    initLoad,
    updateState: setState,
  };
};
