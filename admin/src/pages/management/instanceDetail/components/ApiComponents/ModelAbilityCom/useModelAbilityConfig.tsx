import { DrawerAttachment, IconButton, InputNumberWithSlider, ReactMarkdown } from '@/components';
import {
  GENERATIONS_NUMBER,
  ModelAbility,
  RESPONSE_FORMAT,
  SAMPLING_METHODS,
  UPLOAD_METHOD,
} from '@/constants/modelData';
import { InstanceDetail } from '@/types/Public/data';
import { copyToClipboard, formatDisplayTime } from '@/utils';
import { l, lGet } from '@/utils/intl';
import { CopyOutlined, PlusOutlined } from '@ant-design/icons';
import { Trash2 } from 'lucide-react';
import type { FormInstance, FormListActionType, ProFormProps } from '@ant-design/pro-components';
import {
  ProFormDigit,
  ProFormField,
  ProFormGroup,
  ProFormList,
  ProFormSegmented,
  ProFormSelect,
  ProFormText,
  ProFormTextArea,
  ProFormUploadDragger,
} from '@ant-design/pro-components';
import { Button, Form, Image, Input } from 'antd';
import { isEmpty, isNumber, omit, size } from 'lodash';
import { useRef } from 'react';
import AudioUpload from './AudioUpload';
import CopyDocuments from './CopyDocuments';
import Divider from './Divider';
import ImageEditor from './ImageEditorCreatMask';
import ImageSize from './ImageSize';
import ResultCarouselMd from './ResultCarouselMd';
import type { AbilityApiResult, AbilityFormValues, AbilityImageItem } from '../../../abilityFormTypes';
import { asApiResult, asImageList, formPart } from '../../../abilityFormTypes';

interface Params {
  form: FormInstance;
  selectModelAbility: ModelAbility;
  instanceDetail: InstanceDetail;
}
interface Config {
  showExtraConfig: boolean;
  submitterConfig: ProFormProps['submitter'];
  requestApi: string;
  submitLoadingText?: string;
  formPanel: React.ReactNode;
  resultPanel: (res: AbilityApiResult | Blob) => React.ReactNode;
  transformValues?: (v: AbilityFormValues) => unknown;
}
type UseModelAbilityConfig = (params: Params) => Config;
const useModelAbilityConfig: UseModelAbilityConfig = ({
  form,
  selectModelAbility,
  instanceDetail,
}) => {
  const modelAbility = instanceDetail.model_ability;
  const documentsRef = useRef<FormListActionType>();
  const referenceImageValues = Form.useWatch('image', form);
  const firstFrameValues = Form.useWatch('first_frame', form);
  const lastFrameValues = Form.useWatch('last_frame', form);

  const transformImageRender = (list?: AbilityImageItem[]) => {
    const rows = list || [];
    if (size(rows)) {
      const imageList = rows.map((item) => item.url || `data:image/png;base64,${item.b64_json}`);
      return (
        <div className="grid grid-cols-2 gap-[20px]">
          {imageList.map((url) => (
            <DrawerAttachment
              key={url}
              type="image"
              url={url}
              urlList={imageList}
              width={'100%'}
              height={300}
            />
          ))}
        </div>
      );
    }
    return null;
  };
  const handleDeleteImg = (e: React.MouseEvent<HTMLDivElement>, formKey: string) => {
    e.stopPropagation();
    form.setFieldsValue({ [formKey]: undefined });
  };
  const config: Record<string, Config> = {
    [ModelAbility.text2image]: {
      showExtraConfig: true,
      submitterConfig: {
        searchConfig: {
          submitText: l('model.running.generateNow'),
        },
        submitButtonProps: {
          block: true,
        },
      },
      requestApi: '/images/generations',
      submitLoadingText: l('model.running.generate'),
      formPanel: (
        <>
          <ProFormTextArea
            name="prompt"
            label={l('model.running.prompt')}
            placeholder={l('model.running.promptTips')}
            rules={[{ required: true }]}
            fieldProps={{ rows: 3 }}
          />
          <ProFormTextArea
            name="negative_prompt"
            label={l('model.running.negativePrompt')}
            placeholder={l('model.running.negativePromptTips')}
            fieldProps={{ rows: 3 }}
          />
          <Divider />
          <Form.Item name="size" initialValue="1024x1024" className="w-full">
            <ImageSize />
          </Form.Item>
          <ProFormSegmented
            name="n"
            label={l('model.running.GenerationsNum')}
            initialValue={GENERATIONS_NUMBER[0]}
            colProps={{ span: 12 }}
            fieldProps={{
              block: true,
              options: GENERATIONS_NUMBER,
            }}
          />
          <ProFormSegmented
            name="response_format"
            label={l('model.running.image.resFormat')}
            initialValue={RESPONSE_FORMAT[0]}
            colProps={{ span: 12 }}
            fieldProps={{
              block: true,
              options: RESPONSE_FORMAT,
            }}
          />
          <ProFormDigit
            label="Guidance Scale"
            name={['kwargs', 'guidance_scale']}
            colProps={{ span: 12 }}
            initialValue={-1}
            fieldProps={{
              min: -1,
            }}
          />
          <ProFormDigit
            label="Inference Step Number"
            name={['kwargs', 'steps']}
            colProps={{ span: 12 }}
            initialValue={-1}
            fieldProps={{
              min: -1,
            }}
          />
          <ProFormSelect
            name={['kwargs', 'sampler_name']}
            label="Sampling Method"
            initialValue={SAMPLING_METHODS[0]}
            options={SAMPLING_METHODS}
          />
        </>
      ),
      resultPanel: (result: AbilityApiResult | Blob) => transformImageRender(asImageList(asApiResult(result)?.data)),
      // 后端处理代码上之后，下掉此兼容逻辑
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
      showExtraConfig: true,
      submitterConfig: {
        searchConfig: {
          submitText: l('model.running.generateNow'),
        },
        submitButtonProps: {
          block: true,
        },
      },
      requestApi: '/images/variations',
      submitLoadingText: l('model.running.generate'),
      formPanel: (
        <>
          <div className="overflow-hidden w-full">
            <ProFormUploadDragger
              name="image"
              accept="image/*"
              rules={[{ required: true }]}
              {...(size(referenceImageValues)
                ? {
                    icon: (
                      <IconButton
                        onClick={(e) => handleDeleteImg(e, 'image')}
                        className="!w-7 !h-7 absolute top-2.5 right-2.5 text-muted hover:text-danger"
                      >
                        <Trash2 size={16} />
                      </IconButton>
                    ),
                    title: (
                      <div
                        onClick={(e) => e.stopPropagation()}
                        className="flex flex-col items-center gap-y-[8px]"
                      >
                        <Image
                          width={80}
                          height={80}
                          src={URL.createObjectURL(referenceImageValues[0].originFileObj)}
                        />
                        <div className="truncate text-[#999] !text-[12px] w-[60%]">
                          {referenceImageValues[0].name}
                        </div>
                      </div>
                    ),
                    description: false,
                  }
                : {
                    title: l('model.running.image.reference'),
                    description: l('model.running.image.referenceDesc'),
                  })}
              fieldProps={{
                maxCount: 1,
                showUploadList: false,
                customRequest: () => {},
              }}
            />
          </div>
          <Divider />
          <ProFormTextArea
            name="prompt"
            label={l('model.running.prompt')}
            placeholder={l('model.running.promptTips')}
            rules={[{ required: true }]}
            fieldProps={{ rows: 3 }}
          />
          <ProFormTextArea
            name="negative_prompt"
            label={l('model.running.negativePrompt')}
            placeholder={l('model.running.negativePromptTips')}
            fieldProps={{ rows: 3 }}
          />
          <Divider />
          <Form.Item name="size" initialValue="1024x1024" className="w-full">
            <ImageSize />
          </Form.Item>
          <ProFormSegmented
            name="n"
            label={l('model.running.GenerationsNum')}
            initialValue={GENERATIONS_NUMBER[0]}
            fieldProps={{
              block: true,
              options: GENERATIONS_NUMBER,
            }}
          />
          <ProFormSegmented
            name="response_format"
            label={l('model.running.image.resFormat')}
            initialValue={RESPONSE_FORMAT[0]}
            fieldProps={{
              block: true,
              options: RESPONSE_FORMAT,
            }}
          />
          <ProFormDigit
            label="Guidance Scale"
            name={['kwargs', 'guidance_scale']}
            colProps={{ span: 12 }}
            initialValue={-1}
            fieldProps={{
              min: -1,
            }}
          />
          <ProFormDigit
            label="Inference Step Number"
            name={['kwargs', 'num_inference_steps']}
            colProps={{ span: 12 }}
            initialValue={-1}
            fieldProps={{
              min: -1,
            }}
          />
          <ProFormDigit
            label="Padding image to multiple"
            name={['kwargs', 'padding_image_to_multiple']}
            colProps={{ span: 12 }}
            initialValue={-1}
            fieldProps={{
              min: -1,
            }}
          />
          <ProFormSelect
            name={['kwargs', 'sampler_name']}
            label="Sampling Method"
            options={SAMPLING_METHODS}
            colProps={{ span: 12 }}
          />
        </>
      ),
      resultPanel: (result: AbilityApiResult | Blob) => transformImageRender(asImageList(asApiResult(result)?.data)),
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
      showExtraConfig: true,
      submitterConfig: {
        searchConfig: {
          submitText: l('model.running.generateNow'),
        },
        submitButtonProps: {
          block: true,
        },
      },
      requestApi: '/images/inpainting',
      submitLoadingText: l('model.running.generate'),
      formPanel: (
        <>
          <ProFormField
            name="inpainting_image"
            rules={[{ required: true, message: lGet('model.running.image.uploadInpaintError') }]}
          >
            <ImageEditor
              updateMask={(value) => {
                form.setFieldValue('mask_image', value);
              }}
            />
          </ProFormField>
          <ProFormField name="mask_image" hidden />
          <Divider />
          <ProFormTextArea
            name="prompt"
            label={l('model.running.prompt')}
            placeholder={l('model.running.promptTips')}
            rules={[{ required: true }]}
            fieldProps={{ rows: 3 }}
          />
          <ProFormTextArea
            name="negative_prompt"
            label={l('model.running.negativePrompt')}
            placeholder={l('model.running.negativePromptTips')}
            fieldProps={{ rows: 3 }}
          />
          <Divider />
          <Form.Item name="size" initialValue="1024x1024" className="w-full">
            <ImageSize />
          </Form.Item>
          <ProFormSegmented
            name="n"
            label={l('model.running.GenerationsNum')}
            initialValue={GENERATIONS_NUMBER[0]}
            colProps={{ span: 12 }}
            fieldProps={{
              block: true,
              options: GENERATIONS_NUMBER,
            }}
          />
          <ProFormSegmented
            name="response_format"
            label={l('model.running.image.resFormat')}
            initialValue={RESPONSE_FORMAT[0]}
            colProps={{ span: 12 }}
            fieldProps={{
              block: true,
              options: RESPONSE_FORMAT,
            }}
          />
          <ProFormDigit
            label="Guidance Scale"
            name={['kwargs', 'guidance_scale']}
            colProps={{ span: 12 }}
            initialValue={-1}
            fieldProps={{
              min: -1,
            }}
          />
          <ProFormDigit
            label="Inference Step Number"
            name={['kwargs', 'num_inference_steps']}
            colProps={{ span: 12 }}
            initialValue={-1}
            fieldProps={{
              min: -1,
            }}
          />
          <ProFormDigit
            label="Padding image to multiple"
            name={['kwargs', 'padding_image_to_multiple']}
            colProps={{ span: 12 }}
            initialValue={-1}
            fieldProps={{
              min: -1,
            }}
          />
          <ProFormField name={['kwargs', 'strength']} initialValue={0.6} colProps={{ span: 12 }}>
            <InputNumberWithSlider label="Strength" min={0} max={1} step={0.1} precision={1} />
          </ProFormField>
          <ProFormSelect
            name={['kwargs', 'sampler_name']}
            label="Sampling Method"
            options={SAMPLING_METHODS}
          />
        </>
      ),
      resultPanel: (result: AbilityApiResult | Blob) => transformImageRender(asImageList(asApiResult(result)?.data)),
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

        // 后端处理代码上之后，下掉此兼容逻辑
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
      showExtraConfig: false,
      submitterConfig: {
        searchConfig: {
          submitText: l('global.actions.call'),
        },
        submitButtonProps: {
          block: true,
        },
        resetButtonProps: false,
      },
      requestApi: '/embeddings',
      formPanel: (
        <ProFormTextArea
          label={l('model.running.prompt')}
          name="input"
          rules={[{ required: true }]}
        />
      ),
      resultPanel: (result: AbilityApiResult | Blob) => {
        const payload = asApiResult(result);
        const first = Array.isArray(payload?.data) ? payload.data[0] : undefined;
        const embeddingList =
          first && typeof first === 'object' && 'embedding' in first
            ? (first as { embedding?: unknown[] }).embedding || []
            : [];
        if (size(embeddingList)) {
          const value = JSON.stringify(embeddingList, null, 2);
          return (
            <div>
              <Input.TextArea value={value} rows={20} />
              <div className="mt-[10px]">
                {l('model.running.length')} {size(embeddingList)}{' '}
                <CopyOutlined
                  className="cursor-pointer hover:text-primary"
                  onClick={() => copyToClipboard(value)}
                />
              </div>
            </div>
          );
        }
        return null;
      },
    },
    [ModelAbility.generate]: {
      showExtraConfig: false,
      submitterConfig: {
        searchConfig: {
          submitText: l('global.actions.call'),
        },
        submitButtonProps: {
          block: true,
        },
        resetButtonProps: false,
      },
      requestApi: '/completions',
      submitLoadingText: l('model.running.generate'),
      formPanel: (
        <>
          <ProFormTextArea
            name="prompt"
            label={l('model.running.prompt')}
            placeholder={l('model.running.promptTips')}
            rules={[{ required: true }]}
            fieldProps={{ rows: 6 }}
          />

          <ProFormDigit
            label="Max Tokens"
            name="max_tokens"
            placeholder="0 stands for maximum possible tokens"
          />
          <ProFormField name="temperature" initialValue={1}>
            <InputNumberWithSlider label="Temperature" min={0} max={2} step={0.01} precision={2} />
          </ProFormField>
          <ProFormDigit label="Top K" name="top_k" colProps={{ span: 12 }} />
          <ProFormText
            name="lora_name"
            label="LoRA Name"
            colProps={{ span: 12 }}
            initialValue={undefined}
          />
        </>
      ),
      resultPanel: (data: AbilityApiResult | Blob) => {
        const payload = asApiResult(data);
        const text = payload?.choices?.[0]?.text;
        if (text) {
          return (
            <div className="flex flex-col justify-between gap-y-[10px] h-[calc(100%-32px)] relative">
              <ReactMarkdown classnames="rounded-2">{text}</ReactMarkdown>
              <CopyOutlined
                className="absolute top-[-48px] right-0 cursor-pointer hover:text-primary"
                onClick={() => copyToClipboard(text)}
              />
              <div className="shrink-0 flex flex-col items-end pb-[20px] text-[#8a8a8a]">
                {payload?.created && (
                  <div>{formatDisplayTime(payload.created * 1000)}</div>
                )}
                {!!payload?.usage?.total_tokens && (
                  <div>
                    {payload?.usage?.prompt_tokens} → {payload?.usage?.completion_tokens} (∑
                    {payload?.usage?.total_tokens})
                  </div>
                )}
              </div>
            </div>
          );
        }
        return null;
      },
    },
    [ModelAbility.rerank]: {
      showExtraConfig: false,
      submitterConfig: {
        searchConfig: {
          submitText: l('global.actions.call'),
        },
        submitButtonProps: {
          block: true,
        },
      },
      requestApi: '/rerank',
      formPanel: (
        <>
          <ProFormTextArea
            rules={[{ required: true }]}
            name="query"
            placeholder={l('model.running.rerank.queryTips')}
            fieldProps={{
              rows: 4,
            }}
          />
          <div className="ml-[4px] mb-[10px] w-full flex justify-between">
            <div className="flex items-center gap-x-[10px]">
              {l('model.running.rerank.documentsTitle')}
              <Button
                size="small"
                type="primary"
                shape="circle"
                onClick={() => documentsRef.current?.add?.({})}
                icon={<PlusOutlined />}
              />
            </div>
            <CopyDocuments form={form} />
          </div>
          <ProFormList
            initialValue={[{}]}
            actionRef={documentsRef}
            name="documents"
            creatorButtonProps={false}
          >
            <ProFormGroup key="group">
              <ProFormText
                name="corpus"
                rules={[{ required: true }]}
                placeholder={l('model.running.rerank.documentsCorpus')}
                colProps={{ span: 24 }}
              />
            </ProFormGroup>
          </ProFormList>
        </>
      ),
      transformValues: (values: AbilityFormValues) => {
        return {
          model: values.model,
          query: values.query,
          documents: (values.documents || []).map((item) => item.corpus),
          replica_id: values.replica_id,
        };
      },
      resultPanel: (data: AbilityApiResult | Blob) => {
        const payload = asApiResult(data);
        if (payload?.results) {
          const jsonString = JSON.stringify(payload.results, null, '\t');
          return <Input.TextArea value={jsonString} rows={10} />;
        }
        return null;
      },
    },
    [ModelAbility.audio2audio]: {
      showExtraConfig: true,
      submitterConfig: {
        searchConfig: {
          submitText: l('model.running.convertNow'),
        },
        submitButtonProps: {
          block: true,
        },
        resetButtonProps: false,
      },
      submitLoadingText: l('model.running.convert'),
      requestApi: '/audio/speech',
      formPanel: (
        <>
          <ProFormSegmented
            name="uploadType"
            label={l('model.running.uploadType')}
            initialValue={UPLOAD_METHOD[0].value}
            fieldProps={{
              block: true,
              options: UPLOAD_METHOD,
            }}
          />
          <Form.Item
            name="prompt_speech"
            className="w-full"
            rules={[{ required: true, message: lGet('model.running.audioMissMsg') }]}
          >
            <AudioUpload form={form} />
          </Form.Item>
          <ProFormTextArea
            rules={[{ required: true }]}
            name="input"
            placeholder={l('model.running.convertText')}
            fieldProps={{
              rows: 10,
            }}
          />
        </>
      ),
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
      resultPanel: (res: AbilityApiResult | Blob) => {
        if (!(res instanceof Blob)) return null;
        const audioUrl = URL.createObjectURL(res);
        return audioUrl ? <DrawerAttachment type="audio" url={audioUrl} /> : null;
      },
    },
    [ModelAbility.audio2text]: {
      showExtraConfig: false,
      submitterConfig: {
        searchConfig: {
          submitText: l('model.running.convertNow'),
        },
        submitButtonProps: {
          block: true,
        },
        resetButtonProps: false,
      },
      submitLoadingText: l('model.running.convert'),
      requestApi: '/audio/transcriptions',
      formPanel: (
        <>
          <ProFormSegmented
            name="uploadType"
            label={l('model.running.uploadType')}
            initialValue={UPLOAD_METHOD[0].value}
            fieldProps={{
              block: true,
              options: UPLOAD_METHOD,
            }}
          />
          <Form.Item
            name="file"
            className="w-full"
            rules={[{ required: true, message: lGet('model.running.audioMissMsg') }]}
          >
            <AudioUpload form={form} />
          </Form.Item>
          <ProFormText name="language" label="Language" placeholder="e.g. en or zh" />
          <ProFormField name="temperature" initialValue={0}>
            <InputNumberWithSlider label="Temperature" min={0} max={1} step={0.1} precision={1} />
          </ProFormField>
          <ProFormTextArea
            label="Prompt"
            name="prompt"
            placeholder="Provide context or vocabulary"
          />
        </>
      ),
      transformValues: (values: AbilityFormValues) => {
        const formData = new FormData();
        formData.append('file', formPart(values.file));
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
      resultPanel: (data: AbilityApiResult | Blob) => {
        if (data instanceof Blob) return null;
        return <div>{data?.text || ''}</div>;
      },
    },
    [ModelAbility.text2audio]: {
      showExtraConfig: true,
      submitterConfig: {
        searchConfig: {
          submitText: l('model.running.convertNow'),
        },
        submitButtonProps: {
          block: true,
        },
      },
      submitLoadingText: l('model.running.convert'),
      requestApi: '/audio/speech',
      formPanel: (
        <>
          <ProFormTextArea
            rules={[{ required: true }]}
            name="input"
            placeholder={l('model.running.convertText')}
            fieldProps={{
              rows: 6,
            }}
          />
          <ProFormText name="voice" label="Voice" />
          <ProFormField initialValue={1} name="speed">
            <InputNumberWithSlider label="Speed" min={0.5} max={2} step={0.1} precision={1} />
          </ProFormField>

          <div className="border-t mx-[-16px] my-[10px] w-[calc(100%+40px)]" />
          {modelAbility.includes(ModelAbility.text2audioVoiceCloning) && (
            <>
              <ProFormField
                label="Prompt Speech (for cloning)"
                name="prompt_speech"
                className="w-full"
                rules={[
                  {
                    required: !modelAbility.includes(ModelAbility.text2audioZeroShot),
                    message: lGet('model.running.audioMissMsg'),
                  },
                ]}
              >
                <AudioUpload form={form} />
              </ProFormField>
              <ProFormTextArea
                label="Prompt Text (for cloning)"
                name={['kwargs', 'prompt_text']}
                fieldProps={{
                  rows: 3,
                }}
              />
            </>
          )}
        </>
      ),
      resultPanel: (res: AbilityApiResult | Blob) => {
        if (!(res instanceof Blob)) return null;
        const audioUrl = URL.createObjectURL(res);
        return audioUrl ? <DrawerAttachment type="audio" url={audioUrl} /> : null;
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
    [ModelAbility.text2video]: {
      showExtraConfig: true,
      submitterConfig: {
        searchConfig: {
          submitText: l('model.running.generateNow'),
        },
        submitButtonProps: {
          block: true,
        },
      },
      submitLoadingText: l('model.running.generate'),
      requestApi: '/video/generations',
      formPanel: (
        <>
          <ProFormTextArea
            name="prompt"
            label={l('model.running.prompt')}
            placeholder={l('model.running.promptTips')}
            rules={[{ required: true }]}
            fieldProps={{
              rows: 3,
            }}
          />
          <ProFormTextArea
            name="negative_prompt"
            label={l('model.running.negativePrompt')}
            placeholder={l('model.running.negativePromptTips')}
            fieldProps={{ rows: 3 }}
          />
          <Divider />
          <ProFormDigit
            name={['kwargs', 'width']}
            label="Width"
            initialValue={512}
            colProps={{ span: 12 }}
            fieldProps={{
              step: 4,
              precision: 0,
            }}
          />
          <ProFormDigit
            name={['kwargs', 'height']}
            label="Height"
            initialValue={512}
            colProps={{ span: 12 }}
            fieldProps={{
              step: 4,
              precision: 0,
            }}
          />
          <ProFormDigit
            name={['kwargs', 'num_frames']}
            colProps={{ span: 12 }}
            label="Frames"
            initialValue={16}
            fieldProps={{
              precision: 0,
            }}
          />
          <ProFormDigit
            label="FPS"
            name={['kwargs', 'fps']}
            colProps={{ span: 12 }}
            initialValue={8}
            fieldProps={{
              precision: 0,
            }}
          />
          <ProFormDigit
            label="Inference Steps"
            name={['kwargs', 'num_inference_steps']}
            colProps={{ span: 12 }}
            initialValue={25}
            fieldProps={{
              precision: 0,
            }}
          />
          <ProFormField
            name={['kwargs', 'guidance_scale']}
            initialValue={7.5}
            colProps={{ span: 12 }}
          >
            <InputNumberWithSlider
              label="Guidance Scale"
              min={1}
              max={20}
              step={0.1}
              precision={1}
            />
          </ProFormField>
        </>
      ),
      resultPanel: (result: AbilityApiResult | Blob) => {
        if (size(asImageList(asApiResult(result)?.data))) {
          return (
            <div className="grid grid-cols-2 gap-[20px]">
              {asImageList(asApiResult(result)?.data).map((item) => (
                <DrawerAttachment
                  type="video"
                  key={item.url || item.b64_json}
                  url={item.url || `data:video/mp4;base64,${item.b64_json}`}
                  className="!w-full !h-[300px]"
                />
              ))}
            </div>
          );
        }
        return null;
      },
    },
    [ModelAbility.image2video]: {
      showExtraConfig: true,
      submitterConfig: {
        searchConfig: {
          submitText: l('model.running.generateNow'),
        },
        submitButtonProps: {
          block: true,
        },
      },
      submitLoadingText: l('model.running.generate'),
      requestApi: '/video/generations/image',
      formPanel: (
        <>
          <div className="overflow-hidden w-full">
            <ProFormUploadDragger
              name="image"
              accept="image/*"
              rules={[{ required: true }]}
              {...(size(referenceImageValues)
                ? {
                    icon: (
                      <IconButton
                        onClick={(e) => handleDeleteImg(e, 'image')}
                        className="!w-7 !h-7 absolute top-2.5 right-2.5 text-muted hover:text-danger"
                      >
                        <Trash2 size={16} />
                      </IconButton>
                    ),
                    title: (
                      <div
                        onClick={(e) => e.stopPropagation()}
                        className="flex flex-col items-center gap-y-[8px]"
                      >
                        <Image
                          width={80}
                          height={80}
                          src={URL.createObjectURL(referenceImageValues[0].originFileObj)}
                        />
                        <div className="truncate text-[#999] !text-[12px] w-[60%]">
                          {referenceImageValues[0].name}
                        </div>
                      </div>
                    ),
                    description: false,
                  }
                : {
                    title: l('model.running.video.reference'),
                    description: l('model.running.video.referenceDesc'),
                  })}
              fieldProps={{
                maxCount: 1,
                showUploadList: false,
                customRequest: () => {},
              }}
            />
          </div>
          <Divider />
          <ProFormTextArea
            name="prompt"
            label={l('model.running.prompt')}
            placeholder={l('model.running.promptTips')}
            rules={[{ required: true }]}
            fieldProps={{ rows: 3 }}
          />
          <ProFormTextArea
            name="negative_prompt"
            label={l('model.running.negativePrompt')}
            placeholder={l('model.running.negativePromptTips')}
            fieldProps={{ rows: 3 }}
          />
          <Divider />

          <ProFormDigit
            name={['kwargs', 'width']}
            label="Width"
            initialValue={512}
            colProps={{ span: 12 }}
            fieldProps={{
              step: 4,
              precision: 0,
            }}
          />
          <ProFormDigit
            name={['kwargs', 'height']}
            label="Height"
            initialValue={512}
            colProps={{ span: 12 }}
            fieldProps={{
              step: 4,
              precision: 0,
            }}
          />
          <ProFormDigit
            name={['kwargs', 'num_frames']}
            colProps={{ span: 12 }}
            label="Frames"
            initialValue={16}
            fieldProps={{
              precision: 0,
            }}
          />
          <ProFormDigit
            label="FPS"
            name={['kwargs', 'fps']}
            colProps={{ span: 12 }}
            initialValue={8}
            fieldProps={{
              precision: 0,
            }}
          />
          <ProFormDigit
            label="Inference Steps"
            name={['kwargs', 'num_inference_steps']}
            colProps={{ span: 12 }}
            initialValue={25}
            fieldProps={{
              precision: 0,
            }}
          />
          <ProFormField
            name={['kwargs', 'guidance_scale']}
            initialValue={7.5}
            colProps={{ span: 12 }}
          >
            <InputNumberWithSlider
              label="Guidance Scale"
              min={1}
              max={20}
              step={0.1}
              precision={1}
            />
          </ProFormField>
        </>
      ),
      resultPanel: (result: AbilityApiResult | Blob) => {
        if (size(asImageList(asApiResult(result)?.data))) {
          return (
            <div className="grid grid-cols-2 gap-[20px]">
              {asImageList(asApiResult(result)?.data).map((item) => (
                <DrawerAttachment
                  type="video"
                  key={item.url || item.b64_json}
                  url={item.url || `data:video/mp4;base64,${item.b64_json}`}
                  className="!w-full !h-[300px]"
                />
              ))}
            </div>
          );
        }
        return null;
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
      showExtraConfig: true,
      submitterConfig: {
        searchConfig: {
          submitText: l('model.running.generateNow'),
        },
        submitButtonProps: {
          block: true,
        },
      },
      submitLoadingText: l('model.running.generate'),
      requestApi: '/video/generations/flf',
      formPanel: (
        <>
          <ProFormUploadDragger
            name="first_frame"
            accept="image/*"
            rules={[{ required: true }]}
            colProps={{ span: 12 }}
            {...(size(firstFrameValues)
              ? {
                  icon: (
                    <IconButton
                      onClick={(e) => handleDeleteImg(e, 'first_frame')}
                      className="!w-7 !h-7 absolute top-2.5 right-2.5 text-muted hover:text-danger"
                    >
                      <Trash2 size={16} />
                    </IconButton>
                  ),
                  title: (
                    <div
                      onClick={(e) => e.stopPropagation()}
                      className="flex flex-col items-center gap-y-[8px]"
                    >
                      <Image
                        width={80}
                        height={80}
                        src={URL.createObjectURL(firstFrameValues[0].originFileObj)}
                      />
                      <div className="truncate text-[#999] !text-[12px] w-[60%]">
                        {firstFrameValues[0].name}
                      </div>
                    </div>
                  ),
                  description: false,
                }
              : {
                  title: l('model.running.video.firstFrame'),
                  description: l('model.running.video.referenceDesc'),
                })}
            fieldProps={{
              maxCount: 1,
              showUploadList: false,
              customRequest: () => {},
            }}
          />
          <ProFormUploadDragger
            name="last_frame"
            accept="image/*"
            colProps={{ span: 12 }}
            rules={[{ required: true }]}
            {...(size(lastFrameValues)
              ? {
                  icon: (
                    <IconButton
                      onClick={(e) => handleDeleteImg(e, 'last_frame')}
                      className="!w-7 !h-7 absolute top-2.5 right-2.5 text-muted hover:text-danger"
                    >
                      <Trash2 size={16} />
                    </IconButton>
                  ),
                  title: (
                    <div
                      onClick={(e) => e.stopPropagation()}
                      className="flex flex-col items-center gap-y-[8px]"
                    >
                      <Image
                        width={80}
                        height={80}
                        src={URL.createObjectURL(lastFrameValues[0].originFileObj)}
                      />
                      <div className="truncate text-[#999] !text-[12px] w-[60%]">
                        {lastFrameValues[0].name}
                      </div>
                    </div>
                  ),
                  description: false,
                }
              : {
                  title: l('model.running.video.lastFrame'),
                  description: l('model.running.video.referenceDesc'),
                })}
            fieldProps={{
              maxCount: 1,
              showUploadList: false,
              customRequest: () => {},
            }}
          />
          <Divider />
          <ProFormTextArea
            name="prompt"
            label={l('model.running.prompt')}
            placeholder={l('model.running.promptTips')}
            rules={[{ required: true }]}
            fieldProps={{ rows: 3 }}
          />
          <ProFormTextArea
            name="negative_prompt"
            label={l('model.running.negativePrompt')}
            placeholder={l('model.running.negativePromptTips')}
            fieldProps={{ rows: 3 }}
          />
          <Divider />
          <ProFormDigit
            name={['kwargs', 'width']}
            label="Width"
            initialValue={512}
            colProps={{ span: 12 }}
            fieldProps={{
              step: 4,
              precision: 0,
            }}
          />
          <ProFormDigit
            name={['kwargs', 'height']}
            label="Height"
            initialValue={512}
            colProps={{ span: 12 }}
            fieldProps={{
              step: 4,
              precision: 0,
            }}
          />
          <ProFormDigit
            name={['kwargs', 'num_frames']}
            colProps={{ span: 12 }}
            label="Frames"
            initialValue={16}
            fieldProps={{
              precision: 0,
            }}
          />
          <ProFormDigit
            label="FPS"
            name={['kwargs', 'fps']}
            colProps={{ span: 12 }}
            initialValue={8}
            fieldProps={{
              precision: 0,
            }}
          />
          <ProFormDigit
            label="Inference Steps"
            name={['kwargs', 'num_inference_steps']}
            colProps={{ span: 12 }}
            initialValue={25}
            fieldProps={{
              precision: 0,
            }}
          />
          <ProFormField
            name={['kwargs', 'guidance_scale']}
            initialValue={7.5}
            colProps={{ span: 12 }}
          >
            <InputNumberWithSlider
              label="Guidance Scale"
              min={1}
              max={20}
              step={0.1}
              precision={1}
            />
          </ProFormField>
        </>
      ),
      resultPanel: (result: AbilityApiResult | Blob) => {
        if (size(asImageList(asApiResult(result)?.data))) {
          return (
            <div className="grid grid-cols-2 gap-[20px]">
              {asImageList(asApiResult(result)?.data).map((item) => (
                <DrawerAttachment
                  type="video"
                  key={item.url || item.b64_json}
                  url={item.url || `data:video/mp4;base64,${item.b64_json}`}
                  className="!w-full !h-[300px]"
                />
              ))}
            </div>
          );
        }
        return null;
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
    [ModelAbility.ocr]: {
      showExtraConfig: true,
      submitterConfig: {
        searchConfig: {
          submitText: l('model.running.startRecognition'),
        },
        submitButtonProps: {
          block: true,
        },
      },
      submitLoadingText: l('model.running.recognizing'),
      requestApi: '/images/ocr',
      formPanel: (
        <ProFormUploadDragger
          name="image"
          fieldProps={{
            maxCount: 1,
            showUploadList: false,
            customRequest: () => {},
          }}
          rules={[{ required: true }]}
          {...(size(referenceImageValues)
            ? {
                icon: (
                  <IconButton
                    onClick={(e) => handleDeleteImg(e, 'image')}
                    className="!w-7 !h-7 absolute top-2.5 right-2.5 text-muted hover:text-danger"
                  >
                    <Trash2 size={16} />
                  </IconButton>
                ),
                title: (
                  <div
                    onClick={(e) => e.stopPropagation()}
                    className="flex flex-col items-center gap-y-[8px]"
                  >
                    <Image
                      width={80}
                      height={80}
                      src={URL.createObjectURL(referenceImageValues[0].originFileObj)}
                    />
                    <div className="truncate text-[#999] !text-[12px] w-[60%]">
                      {referenceImageValues[0].name}
                    </div>
                  </div>
                ),
                description: false,
              }
            : {
                title: l('model.running.image.reference'),
                description: l('model.running.image.referenceDesc'),
              })}
        />
      ),
      resultPanel: (result) => (
        <ResultCarouselMd
          results={
            result instanceof Blob ? undefined : (result?.data as string[] | undefined)
          }
        />
      ),
      transformValues: (values) => {
        const formData = new FormData();
        formData.append('model', formPart(values.model));
        formData.append('image', referenceImageValues[0].originFileObj);
        if (values.replica_id) formData.append('replica_id', formPart(values.replica_id));
        if (!isEmpty(values.kwargs)) {
          formData.append('kwargs', JSON.stringify(values.kwargs));
        }
        return formData;
      },
    },
    [ModelAbility.docanalyze]: {
      showExtraConfig: false,
      submitterConfig: {
        searchConfig: {
          submitText: l('model.running.startAnalysis'),
        },
        submitButtonProps: {
          block: true,
        },
        resetButtonProps: false,
      },
      submitLoadingText: l('model.running.analyzing'),
      requestApi: '/images/docanalyze',
      formPanel: (
        <>
          <ProFormUploadDragger
            name="file"
            rules={[{ required: true, message: lGet('model.running.uploadEmpty') }]}
            description=""
            fieldProps={{
              maxCount: 1,
              customRequest: ({ onSuccess }) => onSuccess?.('ok'),
              listType: 'picture',
            }}
          />
        </>
      ),
      resultPanel: (result) => {
        const jsonString = JSON.stringify(asApiResult(result)?.data, null, '\t');
        return (
          <div className="relative flex flex-col justify-between gap-y-[10px] h-[calc(100vh-368px)]">
            <CopyOutlined
              className="absolute top-[-48px] right-0 cursor-pointer hover:text-primary"
              onClick={() => copyToClipboard(jsonString)}
            />
            <Input.TextArea value={jsonString} style={{ height: '100%' }} />
          </div>
        );
      },
      transformValues: (values) => {
        const formData = new FormData();
        formData.append('model', formPart(values.model));
        formData.append(
          'file',
          formPart(
            Array.isArray(values.file) ? values.file[0]?.originFileObj : values.file,
          ),
        );
        if (values.replica_id) formData.append('replica_id', formPart(values.replica_id));
        return formData;
      },
    },
  };
  return config?.[selectModelAbility] || {};
};

export default useModelAbilityConfig;
