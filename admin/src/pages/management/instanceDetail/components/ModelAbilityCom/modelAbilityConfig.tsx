import { isEmpty } from 'lodash';
import type { FormInstance, ProFormProps } from '@ant-design/pro-components';
import { omit, isNumber } from 'lodash';
import type { InstanceDetail } from '@/types/Public/data';
import { ModelAbility, SAMPLING_METHODS } from '@/constants/modelData';
import FormPanels from './FormPanels';
import ResultPanels from './ResultPanels';
import type { ApiSchema } from '../../utils';
import type { AbilityFormValues } from '../../abilityFormTypes';
import { formPart } from '../../abilityFormTypes';
export interface FormPanelProps {
  form: FormInstance;
  instanceDetail: InstanceDetail;
}

export type AbilityResultPanel =
  | typeof ResultPanels.Ocr
  | typeof ResultPanels.Image
  | typeof ResultPanels.Embedding
  | typeof ResultPanels.Rerank
  | typeof ResultPanels.Generate
  | typeof ResultPanels.Audio
  | typeof ResultPanels.AudioToText
  | typeof ResultPanels.Video
  | typeof ResultPanels.Docanalyze;

export interface ModelAbilityConfig {
  submitterConfig?: ProFormProps['submitter'];
  requestApi: string;
  loadingText?: string;
  FormPanel: React.FC<FormPanelProps>;
  ResultPanel: AbilityResultPanel;
  codeExample: Omit<ApiSchema, 'path' | 'name'>;
  transformValues?: (v: AbilityFormValues) => unknown;
}

export const chatCodeExample = {
  name: ModelAbility.chat,
  path: '/chat/completions',
  method: 'POST',
  contentType: 'json',
  fields: [
    { key: 'model', required: true },
    { key: 'replica_id' },
    { key: 'max_tokens', value: 4000 },
    { key: 'top_p', value: 1 },
    { key: 'top_k', value: 40 },
    { key: 'presence_penalty', value: 0 },
    { key: 'frequency_penalty', value: 0 },
    { key: 'temperature', value: 0.6 },
    { key: 'stream', value: true },
    { key: 'stream_options', value: { include_usage: true } },
    {
      key: 'messages',
      required: true,
      value: [
        {
          role: 'user',
          content: 'Hello, What can you do?',
        },
      ],
    },
  ],
};
const modelAbilityConfig: Record<string, ModelAbilityConfig> = {
  [ModelAbility.ocr]: {
    submitterConfig: {
      searchConfig: {
        submitText: 'models.instances.detail.extract',
      },
    },
    loadingText: 'models.instances.detail.extract.loading',
    requestApi: '/images/ocr',
    FormPanel: FormPanels.Ocr,
    ResultPanel: ResultPanels.Ocr,
    codeExample: {
      method: 'POST',
      contentType: 'form',
      fields: [
        { key: 'model', required: true },
        { key: 'image', required: true, type: 'file', fileName: 'file.png' },
        { key: 'kwargs' },
      ],
    },
    transformValues: (values) => {
      const formData = new FormData();
      formData.append('model', formPart(values.model));
      formData.append('image', values.image![0].originFileObj);
      if (values.replica_id) formData.append('replica_id', formPart(values.replica_id));
      if (!isEmpty(values.kwargs)) {
        formData.append('kwargs', JSON.stringify(values.kwargs));
      }
      return formData;
    },
  },
  [ModelAbility.text2image]: {
    requestApi: '/images/generations',
    FormPanel: FormPanels.TextToimage,
    ResultPanel: ResultPanels.Image,
    codeExample: {
      method: 'POST',
      contentType: 'json',
      fields: [
        { key: 'model', required: true },
        { key: 'replica_id' },
        { key: 'prompt', required: true, value: 'a cute little cat' },
        { key: 'negative_prompt' },
        { key: 'n', required: true, value: 1 },
        { key: 'response_format', required: true, value: 'b64_json' },
        { key: 'size', required: true, value: '1024x1024' },
        { key: 'kwargs', comment: 'guidance_scale、num_inference_steps、sampler_name...' },
      ],
    },
    transformValues: (values) => {
      let { kwargs, ...reset } = values;
      if (kwargs?.guidance_scale === -1) kwargs = omit(kwargs, 'guidance_scale');
      if (kwargs?.num_inference_steps === -1) kwargs = omit(kwargs, 'num_inference_steps');
      if (kwargs?.sampler_name === SAMPLING_METHODS[0]) kwargs = omit(kwargs, 'sampler_name');
      return {
        ...reset,
        model: values.model,
        ...(isEmpty(kwargs) ? {} : { kwargs: JSON.stringify(kwargs) }),
      };
    },
  },
  [ModelAbility.image2image]: {
    requestApi: '/images/variations',
    FormPanel: FormPanels.ImageToImage,
    ResultPanel: ResultPanels.Image,
    codeExample: {
      method: 'POST',
      contentType: 'form',
      fields: [
        { key: 'image', required: true, type: 'file', fileName: 'file.png' },
        { key: 'model', required: true },
        { key: 'replica_id' },
        { key: 'prompt', required: true, value: 'a cute little cat' },
        { key: 'negative_prompt' },
        { key: 'n', required: true, value: 1 },
        { key: 'response_format', required: true, value: 'b64_json' },
        { key: 'size', required: true, value: '1024x1024' },
        {
          key: 'kwargs',
          comment:
            'guidance_scale、num_inference_steps、padding_image_to_multiple、sampler_name...',
        },
      ],
    },
    transformValues: (values: AbilityFormValues) => {
      let { kwargs } = values;
      const formData = new FormData();
      formData.append('image', values.image![0].originFileObj);
      formData.append('model', formPart(values.model));
      formData.append('n', formPart(values.n));
      formData.append('prompt', formPart(values.prompt));
      formData.append('response_format', formPart(values.response_format));
      if (values.negative_prompt) formData.append('negative_prompt', formPart(values.negative_prompt));
      if (values.size) formData.append('size', formPart(values.size));
      if (values.replica_id) formData.append('replica_id', formPart(values.replica_id));

      if (kwargs?.guidance_scale === -1) kwargs = omit(kwargs, 'guidance_scale');
      if (kwargs?.num_inference_steps === -1) kwargs = omit(kwargs, 'num_inference_steps');
      if (kwargs?.padding_image_to_multiple === -1)
        kwargs = omit(kwargs, 'padding_image_to_multiple');
      if (kwargs?.sampler_name === SAMPLING_METHODS[0]) kwargs = omit(kwargs, 'sampler_name');

      if (!isEmpty(kwargs)) {
        formData.append('kwargs', JSON.stringify(kwargs));
      }
      return formData;
    },
  },

  [ModelAbility.inpainting]: {
    requestApi: '/images/inpainting',
    FormPanel: FormPanels.Inpainting,
    ResultPanel: ResultPanels.Image,
    codeExample: {
      method: 'POST',
      contentType: 'form',
      fields: [
        { key: 'image', required: true, type: 'file', fileName: 'file.png' },
        { key: 'mask_image', required: true, type: 'file', fileName: 'file.png' },
        { key: 'model', required: true },
        { key: 'replica_id' },
        { key: 'prompt', required: true, value: 'a cute little cat' },
        { key: 'negative_prompt' },
        { key: 'n', required: true, value: 1 },
        { key: 'response_format', required: true, value: 'b64_json' },
        { key: 'size', required: true, value: '1024x1024' },
        {
          key: 'kwargs',
          comment:
            'guidance_scale、num_inference_steps、padding_image_to_multiple、strength、sampler_name...',
        },
      ],
    },
    transformValues: (values: AbilityFormValues) => {
      let { kwargs } = values;
      const formData = new FormData();
      formData.append('image', formPart(values.inpainting_image));
      formData.append('mask_image', formPart(values.mask_image));
      formData.append('model', formPart(values.model));
      formData.append('n', formPart(values.n));
      formData.append('prompt', formPart(values.prompt));
      formData.append('response_format', formPart(values.response_format));
      if (values.negative_prompt) formData.append('negative_prompt', formPart(values.negative_prompt));
      if (values.size) formData.append('size', formPart(values.size));
      if (values.replica_id) formData.append('replica_id', formPart(values.replica_id));

      if (kwargs?.guidance_scale === -1) kwargs = omit(kwargs, 'guidance_scale');
      if (kwargs?.num_inference_steps === -1) kwargs = omit(kwargs, 'num_inference_steps');
      if (kwargs?.padding_image_to_multiple === -1)
        kwargs = omit(kwargs, 'padding_image_to_multiple');
      if (kwargs?.sampler_name === SAMPLING_METHODS[0]) kwargs = omit(kwargs, 'sampler_name');
      if (!isEmpty(kwargs)) {
        formData.append('kwargs', JSON.stringify(kwargs));
      }
      return formData;
    },
  },
  [ModelAbility.embed]: {
    submitterConfig: {
      searchConfig: {
        submitText: 'global.actions.submit',
      },
    },
    requestApi: '/embeddings',
    FormPanel: FormPanels.Embedding,
    ResultPanel: ResultPanels.Embedding,
    codeExample: {
      method: 'POST',
      contentType: 'json',
      fields: [
        { key: 'model', required: true },
        { key: 'replica_id' },
        { key: 'input', required: true, value: 'Artificial Intelligence' },
      ],
    },
  },
  [ModelAbility.rerank]: {
    submitterConfig: {
      searchConfig: {
        submitText: 'global.actions.call',
      },
    },
    requestApi: '/rerank',
    FormPanel: FormPanels.Rerank,
    ResultPanel: ResultPanels.Rerank,
    codeExample: {
      method: 'POST',
      contentType: 'json',
      fields: [
        { key: 'model', required: true },
        { key: 'replica_id' },
        { key: 'query', required: true, value: 'Python list sort' },
        {
          key: 'documents',
          required: true,
          value: ['Use .sort() method on lists.', 'Java is a compiled language.'],
        },
      ],
    },
    transformValues: (values: AbilityFormValues) => {
      return {
        model: values.model,
        query: values.query,
        documents: (values.documents || []).map((item) => item.corpus),
        replica_id: values.replica_id,
      };
    },
  },
  [ModelAbility.generate]: {
    requestApi: '/completions',
    FormPanel: FormPanels.Generate,
    ResultPanel: ResultPanels.Generate,
    codeExample: {
      method: 'POST',
      contentType: 'json',
      fields: [
        { key: 'model', required: true },
        { key: 'replica_id' },
        { key: 'prompt', required: true, value: 'Hello, What can you do?' },
      ],
    },
  },
  [ModelAbility.text2audio]: {
    requestApi: '/audio/speech',
    FormPanel: FormPanels.TextToAudio,
    ResultPanel: ResultPanels.Audio,
    codeExample: {
      method: 'POST',
      contentType: 'json',
      fields: [
        { key: 'model', required: true },
        { key: 'replica_id' },
        { key: 'input', required: true, value: 'Hello' },
        { key: 'voice', value: 'voice ID' },
        { key: 'speed', value: 1 },
        { key: 'kwargs' },
      ],
    },
    transformValues: (values) => {
      if (values.prompt_speech) {
        const formData = new FormData();
        formData.append('input', formPart(values.input));
        formData.append('model', formPart(values.model));
        formData.append('prompt_speech', formPart(values.prompt_speech));
        if (values.voice) formData.append('voice', formPart(values.voice));
        if (values.speed) formData.append('speed', formPart(values.speed));
        if (values.replica_id) formData.append('replica_id', formPart(values.replica_id));
        if (!isEmpty(values.kwargs)) {
          formData.append('kwargs', JSON.stringify(values.kwargs));
        }
        return formData;
      }
      return {
        model: values.model,
        input: values.input,
        speed: values.speed,
        voice: values.voice,
        replica_id: values.replica_id,
        ...(isEmpty(values.kwargs) ? {} : { kwargs: JSON.stringify(values.kwargs) }),
      };
    },
  },
  [ModelAbility.audio2text]: {
    submitterConfig: {
      searchConfig: {
        submitText: 'models.instances.detail.identify',
      },
    },
    loadingText: 'models.instances.detail.identify.loading',
    requestApi: '/audio/transcriptions',
    FormPanel: FormPanels.AudioToText,
    ResultPanel: ResultPanels.AudioToText,
    codeExample: {
      method: 'POST',
      contentType: 'form',
      fields: [
        { key: 'model', required: true },
        { key: 'replica_id' },
        { key: 'file', required: true, type: 'file', fileName: 'file.mp3' },
        { key: 'prompt', value: 'Provide context or vocabulary' },
        { key: 'language', value: 'zh' },
        { key: 'temperature', value: 0 },
        { key: 'kwargs' },
      ],
    },
    transformValues: (values: AbilityFormValues) => {
      const formData = new FormData();
      formData.append(
        'file',
        formPart(Array.isArray(values.file) ? values.file[0]?.originFileObj : values.file),
      );
      formData.append('model', formPart(values.model));
      if (values.replica_id) formData.append('replica_id', formPart(values.replica_id));
      if (values.language) formData.append('language', formPart(values.language));
      if (isNumber(values.temperature)) formData.append('temperature', formPart(values.temperature));
      if (values.prompt) formData.append('prompt', formPart(values.prompt));
      if (!isEmpty(values.kwargs)) {
        formData.append('kwargs', JSON.stringify(values.kwargs));
      }
      return formData;
    },
  },
  [ModelAbility.audio2audio]: {
    submitterConfig: {
      searchConfig: {
        submitText: 'models.instances.detail.convert',
      },
    },
    loadingText: 'models.instances.detail.convert.loading',
    requestApi: '/audio/speech',
    FormPanel: FormPanels.AudioToAudio,
    ResultPanel: ResultPanels.Audio,
    codeExample: {
      method: 'POST',
      contentType: 'form',
      fields: [
        { key: 'model', required: true },
        { key: 'replica_id' },
        { key: 'prompt_speech', required: true, type: 'file', fileName: 'file.mp3' },
        { key: 'input', required: true, value: 'Text to Convert' },
        { key: 'kwargs' },
      ],
    },
    transformValues: (values: AbilityFormValues) => {
      const formData = new FormData();
      formData.append('prompt_speech', formPart(values.prompt_speech));
      formData.append('model', formPart(values.model));
      formData.append('input', formPart(values.input));
      if (values.replica_id) formData.append('replica_id', formPart(values.replica_id));
      if (!isEmpty(values.kwargs)) {
        formData.append('kwargs', JSON.stringify(values.kwargs));
      }
      return formData;
    },
  },
  [ModelAbility.text2video]: {
    requestApi: '/video/generations',
    FormPanel: FormPanels.TextToVideo,
    ResultPanel: ResultPanels.Video,
    codeExample: {
      method: 'POST',
      contentType: 'json',
      fields: [
        { key: 'model', required: true },
        { key: 'replica_id' },
        { key: 'prompt', required: true, value: 'a cute little cat' },
        { key: 'negative_prompt' },
        {
          key: 'kwargs',
          value: {
            width: 512,
            height: 512,
            num_frames: 16,
            fps: 8,
            num_inference_steps: 25,
            guidance_scale: 7.5,
          },
          comment: 'other key/value',
        },
      ],
    },
  },
  [ModelAbility.image2video]: {
    requestApi: '/video/generations/image',
    FormPanel: FormPanels.ImageToVideo,
    ResultPanel: ResultPanels.Video,
    codeExample: {
      method: 'POST',
      contentType: 'form',
      fields: [
        { key: 'model', required: true },
        { key: 'replica_id' },
        { key: 'image', required: true, type: 'file', fileName: 'file.png' },
        { key: 'n', required: true, value: 1 },
        { key: 'prompt', required: true, value: 'a cute little cat' },
        { key: 'negative_prompt' },
        {
          key: 'kwargs',
          value: {
            width: 512,
            height: 512,
            num_frames: 16,
            fps: 8,
            num_inference_steps: 25,
            guidance_scale: 7.5,
          },
          comment: 'other key/value',
        },
      ],
    },
    transformValues: (values: AbilityFormValues) => {
      const formData = new FormData();
      formData.append('image', values.image![0].originFileObj);
      formData.append('model', formPart(values.model));
      formData.append('n', formPart(values.n));
      formData.append('prompt', formPart(values.prompt));
      if (values.negative_prompt) formData.append('negative_prompt', formPart(values.negative_prompt));
      if (values.replica_id) formData.append('replica_id', formPart(values.replica_id));
      if (!isEmpty(values.kwargs)) {
        formData.append('kwargs', JSON.stringify(values.kwargs));
      }
      return formData;
    },
  },
  [ModelAbility.flf2v]: {
    requestApi: '/video/generations/flf',
    FormPanel: FormPanels.FirstLastframeToVideo,
    ResultPanel: ResultPanels.Video,
    codeExample: {
      method: 'POST',
      contentType: 'form',
      fields: [
        { key: 'model', required: true },
        { key: 'replica_id' },
        { key: 'first_frame', required: true, type: 'file', fileName: 'firstFrame.png' },
        { key: 'last_frame', required: true, type: 'file', fileName: 'lastFrame.png' },
        { key: 'n', required: true, value: 1 },
        { key: 'prompt', required: true, value: 'a cute little cat' },
        { key: 'negative_prompt' },
        {
          key: 'kwargs',
          value: {
            width: 512,
            height: 512,
            num_frames: 16,
            fps: 8,
            num_inference_steps: 25,
            guidance_scale: 7.5,
          },
          comment: 'other key/value',
        },
      ],
    },
    transformValues: (values: AbilityFormValues) => {
      const formData = new FormData();
      formData.append('first_frame', values.first_frame![0].originFileObj);
      formData.append('last_frame', values.last_frame![0].originFileObj);
      formData.append('model', formPart(values.model));
      formData.append('prompt', formPart(values.prompt));
      if (values.negative_prompt) formData.append('negative_prompt', formPart(values.negative_prompt));
      if (values.replica_id) formData.append('replica_id', formPart(values.replica_id));
      if (!isEmpty(values.kwargs)) {
        formData.append('kwargs', JSON.stringify(values.kwargs));
      }
      return formData;
    },
  },
  [ModelAbility.docanalyze]: {
    submitterConfig: {
      searchConfig: {
        submitText: 'models.instances.detail.docanalyze',
      },
    },
    loadingText: 'models.instances.detail.docanalyze.loading',
    requestApi: '/images/docanalyze',
    FormPanel: FormPanels.Docanalyze,
    ResultPanel: ResultPanels.Docanalyze,
    codeExample: {
      method: 'POST',
      contentType: 'form',
      fields: [
        { key: 'model', required: true },
        { key: 'replica_id' },
        { key: 'file', required: true, type: 'file', fileName: 'file.pdf' },
      ],
    },
    transformValues: (values) => {
      const formData = new FormData();
      formData.append('model', formPart(values.model));
      formData.append(
        'file',
        formPart(Array.isArray(values.file) ? values.file[0]?.originFileObj : values.file),
      );
      if (values.replica_id) formData.append('replica_id', formPart(values.replica_id));
      return formData;
    },
  },
};

export default modelAbilityConfig;
