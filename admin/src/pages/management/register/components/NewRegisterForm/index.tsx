import {
  ProFormText,
  ProFormDigit,
  ProFormField,
  ProFormTextArea,
  ProFormSelect,
  ProFormCheckbox,
  ProFormRadio,
  ProFormSwitch,
} from '@ant-design/pro-components';
import { Plus } from 'lucide-react';
import { Form, Checkbox, AutoComplete, Button, App } from 'antd';
import type { FormInstance, CheckboxProps, AutoCompleteProps } from 'antd';
import { FC, useRef, useMemo } from 'react';
import { useRequest } from 'ahooks';
import { size, mapValues } from 'lodash';
import request from '@/utils/request';
import { ALL_LIST_PAGES_PARAMS } from '@/constants';
import {
  ModelType,
  MODEL_DEFAULT_VALUE,
  MODEL_ABILITY_OPTIONS,
  MODEL_FAMILY_IMAGE_OPTIONS,
  MODEL_ABILITY_IMAGE_OPTIONS,
  MODEL_ABILITY_AUDIO_OPTIONS,
  MODEL_FAMILY_AUDIO_OPTIONS,
} from '@/constants/modelData';
import { l, lGet } from '@/utils/intl';
import type { DeviceInfo } from '@/types/Public/data';
import { DeviceStatus } from '@/components';
import ProCard from '../ProCard';
import LanguageSelect from '../LanguageSelect';
import ChatTemplate, { ChatTemplateMethod } from '../NewChatTemplate';
import ModelStop from '../NewModelStop';
import ModelSpecs from '../NewModelSpecs';
import NewModelControlnet from '../NewModelControlnet';
import { history } from '@umijs/max';

interface RegisterFormProps {
  currentTab: ModelType;
  form: FormInstance;
  transformFormValues: (needStringify?: boolean, isSubmit?: boolean) => void;
}

const RegisterForm: FC<RegisterFormProps> = ({ currentTab, form, transformFormValues }) => {
  const { message } = App.useApp();
  const modelDefaultValue = MODEL_DEFAULT_VALUE[currentTab] || {};
  const showModelDescription = [ModelType.LLM, ModelType.flexible].includes(currentTab);
  const chatTemplateRef = useRef<ChatTemplateMethod>(null);
  const abilityValue = Form.useWatch('model_ability', form) || [];
  const { data: deviceResult } = useRequest(() =>
    request<{ data: { results: DeviceInfo[] } }>('/device/info', {
      params: ALL_LIST_PAGES_PARAMS,
    }),
  );
  const workerIpOptions = (deviceResult?.data?.results || []).map((item) => ({
    label: item.worker_address,
    value: item.worker_address,
    name: item.name,
    status: item.status,
  }));
  const { data: promptsData } = useRequest(() => request('/models/prompts'));
  const promptsConfig = promptsData?.data || {};
  const { data: familiesData } = useRequest(() => request('/models/families'));
  const familiesSource = useMemo(() => {
    if (!familiesData?.success) return {};
    return mapValues(familiesData?.data || {}, (values) =>
      (values || []).map((value: string) => ({ label: value, value })),
    );
  }, [familiesData]);

  const { run: submit, loading } = useRequest(
    (data) => request(`/model_registrations/${currentTab}`, { method: 'post', data }),
    {
      manual: true,
      onSuccess: (res) => {
        if (res.success) {
          message.success(lGet('models.register.success'));
          history.push(`/models/repository/custom?custom_model_type=${currentTab}`);
        }
      },
    },
  );
  const modelFamilySource = useMemo(() => {
    if (abilityValue.includes('tools')) {
      return familiesSource?.tools || [];
    } else if (abilityValue.includes('vision')) {
      return familiesSource?.vision || [];
    } else if (abilityValue.includes('chat')) {
      return familiesSource?.chat || [];
    } else if (abilityValue.includes('generate')) {
      return familiesSource.generate || [];
    }
    return [];
  }, [familiesSource, abilityValue]);

  const getLanguageFormKeyWithModelType = () => {
    if (currentTab === ModelType.LLM) {
      return 'model_lang';
    }
    return 'language';
  };
  const changeModelAbility: CheckboxProps['onChange'] = (e) => {
    form.setFieldsValue({
      stopController: [{ stop_token_id: '', stop: '' }],
      model_family: '',
    });
    chatTemplateRef.current?.resetStatus?.();
    const formValues = (form.getFieldValue('model_ability') || []) as string[];
    const checked = e.target.checked;
    const currentValue = e.target.value;
    // 勾选vision: 需排除tools(互斥), 如果chat没有勾选，则把chat也勾选，如果chat勾选了，则只把vision勾选
    if (currentValue === 'vision' && checked) {
      setTimeout(
        () =>
          form.setFieldValue(
            'model_ability',
            formValues
              .filter((item) => item !== 'tools')
              .concat(formValues.includes('chat') ? ['vision'] : ['chat', 'vision']),
          ),
        0,
      );
    }
    // 勾选tools: 需排除vision(互斥), 如果chat没有勾选，则把chat也勾选，如果chat勾选了，则只把tools勾选
    if (currentValue === 'tools' && checked) {
      setTimeout(
        () =>
          form.setFieldValue(
            'model_ability',
            formValues
              .filter((item) => item !== 'vision')
              .concat(formValues.includes('chat') ? ['tools'] : ['chat', 'tools']),
          ),
        0,
      );
    }
    // 取消勾选chat时，需要把vision, tools都取消勾选
    if (currentValue === 'chat' && !checked) {
      setTimeout(
        () =>
          form.setFieldValue(
            'model_ability',
            formValues.filter((item) => !['vision', 'tools', 'chat'].includes(item)),
          ),
        0,
      );
    }
  };
  const handleModelFamilyChange = () => {
    if (abilityValue.includes('chat')) {
      form.setFieldValue('stopController', [{ stop_token_id: '', stop: '' }]);
    }
    chatTemplateRef.current?.resetStatus?.();
  };
  const handleModelFamilySelect: AutoCompleteProps['onSelect'] = (value) => {
    if (abilityValue.includes('chat')) {
      const { stop_token_ids = [], stop = [], chat_template } = promptsConfig[value] || {};
      const stopControllerValue = stop_token_ids.map((item: number, index: number) => ({
        stop_token_id: item,
        stop: stop[index],
      }));
      form.setFieldsValue({
        chat_template: chat_template,
        stopController: stopControllerValue,
      });
    }
  };
  const handleCancel = () => history.go(-1);

  const handleSubmit = () => {
    form.validateFields().then((values) => {
      const submitValues = transformFormValues(true, true);
      submit({
        persist: true,
        model: submitValues,
        worker_ip: size(values.worker_ip) ? values.worker_ip : undefined,
      });
    });
  };
  const renderTypeConfig = () => {
    const modelLanguage = (
      <ProFormField
        label={l('models.register.modelLanguage')}
        name={getLanguageFormKeyWithModelType()}
        initialValue={modelDefaultValue[getLanguageFormKeyWithModelType()] || []}
        rules={[{ required: true }]}
      >
        <LanguageSelect
          selectProps={{ size: 'large', maxTagCount: 'responsive' }}
          defaultValue={modelDefaultValue[getLanguageFormKeyWithModelType()] || []}
        />
      </ProFormField>
    );
    const modelUri = (
      <ProFormText
        name="model_uri"
        rules={[{ required: true }]}
        label={l('models.register.modelPath')}
        fieldProps={{ size: 'large' }}
      />
    );
    const modelMaxTokens = (
      <ProFormDigit
        name="max_tokens"
        label={l('models.register.maxTokens')}
        rules={[
          { required: true },
          { pattern: /^[1-9]\d*$/, message: l('models.register.ruleTip') },
        ]}
        fieldProps={{ size: 'large' }}
      />
    );
    switch (currentTab) {
      case ModelType.LLM:
        return (
          <>
            <ProFormDigit name="version" hidden />
            <div className="grid grid-cols-2 gap-2">
              <ProFormField
                name="model_ability"
                label={l('models.register.modelAbility')}
                rules={[{ required: true }]}
              >
                <Checkbox.Group>
                  {/* Checkbox.Group的onChange获取不到当前点击的是谁，故采用遍历的方式渲染Checkbox */}
                  {MODEL_ABILITY_OPTIONS.map((item) => (
                    <Checkbox
                      onChange={(value) => changeModelAbility(value)}
                      key={item.value}
                      value={item.value}
                    >
                      {item.label}
                    </Checkbox>
                  ))}
                </Checkbox.Group>
              </ProFormField>
              {modelLanguage}
              <ProFormText
                name="model_family"
                label={l('models.register.modelFamily')}
                rules={[{ required: true }]}
                fieldProps={{
                  onChange: handleModelFamilyChange,
                }}
              >
                <AutoComplete
                  filterOption
                  options={modelFamilySource}
                  placeholder="You can choose from the built-in models or input your own."
                  onSelect={handleModelFamilySelect}
                  size="large"
                />
              </ProFormText>
              <ProFormDigit
                name="context_length"
                colProps={{ span: 24 }}
                label={l('models.register.contextLength')}
                rules={[
                  { required: true },
                  { pattern: /^[1-9]\d*$/, message: l('models.register.ruleTip') },
                ]}
                fieldProps={{ size: 'large' }}
              />
            </div>
            {abilityValue.includes('chat') && (
              <>
                <ChatTemplate form={form} />
                <ModelStop />
              </>
            )}
            <ModelSpecs form={form} modelType={currentTab} />
          </>
        );
      case ModelType.image:
        return (
          <>
            <div className="grid grid-cols-2 gap-2">
              {modelUri}
              <ProFormRadio.Group
                name="model_family"
                label={l('models.register.modelFamily')}
                rules={[{ required: true }]}
                options={MODEL_FAMILY_IMAGE_OPTIONS}
              />
            </div>
            <ProFormCheckbox.Group
              name="model_ability"
              label={l('models.register.modelAbility')}
              rules={[{ required: true }]}
              options={MODEL_ABILITY_IMAGE_OPTIONS}
              colProps={{ span: 24 }}
            />
            <NewModelControlnet form={form} />
          </>
        );
      case ModelType.embedding:
        return (
          <>
            <div className="grid grid-cols-2 gap-2">
              <ProFormDigit
                name="dimensions"
                label={l('models.register.dimensions')}
                rules={[
                  { required: true },
                  { pattern: /^[1-9]\d*$/, message: l('models.register.ruleTip') },
                ]}
                fieldProps={{ size: 'large' }}
              />
              {modelMaxTokens}
            </div>
            {modelLanguage}
            <ModelSpecs form={form} modelType={currentTab} />
          </>
        );
      case ModelType.rerank:
        return (
          <>
            <div className="grid grid-cols-2 gap-2">
              {modelMaxTokens}
              {modelLanguage}
            </div>
            <ModelSpecs form={form} modelType={currentTab} />
          </>
        );
      case ModelType.audio:
        return (
          <>
            <div className="grid grid-cols-2 gap-2">
              {modelUri}
              <ProFormSelect
                name="model_family"
                label={l('model.register.modelFamily')}
                rules={[{ required: true }]}
                options={MODEL_FAMILY_AUDIO_OPTIONS}
                fieldProps={{ size: 'large' }}
              />
              <ProFormRadio.Group
                name="model_ability"
                label={l('model.register.modelAbility')}
                options={MODEL_ABILITY_AUDIO_OPTIONS}
              />
              <ProFormSwitch name="multilingual" label={l('models.register.multilingual')} />
            </div>
          </>
        );
      case ModelType.flexible:
        return (
          <>
            {modelUri}
            <ProFormText
              name="launcher"
              rules={[{ required: true }]}
              label={l('model.register.launcher')}
              extra={l('model.register.launcher.tip')}
              fieldProps={{ size: 'large' }}
            />
            <ProFormTextArea
              name="launcher_args"
              label={l('model.register.launcherArgs')}
              extra={l('model.register.launcherArgs.tip')}
            />
          </>
        );
      default:
        return null;
    }
  };
  return (
    <div className="flex-1 min-w-0 flex flex-col gap-4">
      <ProCard title={l('models.register.basicInfo')}>
        <div className="flex gap-2">
          <ProFormText
            name="model_name"
            rules={[{ required: true }]}
            label={l('models.register.modelName')}
            extra={l('models.register.modelName.extra')}
            initialValue={modelDefaultValue.model_name}
            colProps={{ span: 12 }}
            fieldProps={{ size: 'large' }}
          />
          {showModelDescription && (
            <ProFormText
              name="model_description"
              label={l('models.register.modelDescription')}
              fieldProps={{ size: 'large' }}
              colProps={{ span: 12 }}
            />
          )}
        </div>
      </ProCard>
      <ProCard
        title={
          <span className="flex items-center gap-2">
            {l('models.register.typeConfig')}
            <div className="py-0.5 px-2.5 rounded-full inline-flex items-center justify-center bg-background text-muted text-xs font-normal">
              {l(`global.model.type.${currentTab}`)}
            </div>
          </span>
        }
      >
        {renderTypeConfig()}
      </ProCard>
      <ProCard bodyStyle={{ paddingBottom: 0 }}>
        <ProFormSelect
          name="worker_ip"
          label="Worker IP"
          mode="multiple"
          options={workerIpOptions}
          fieldProps={{
            size: 'large',
            optionRender: (option) => (
              <div
                className="flex justify-between items-center gap-x-[8px] mr-2"
                key={option.value}
              >
                <span>{`${option.label} (${option.data.name})`}</span>
                {option.data.status && <DeviceStatus status={option.data.status} />}
              </div>
            ),
          }}
        />
      </ProCard>
      <div className="flex justify-end gap-2">
        <Button
          onClick={handleSubmit}
          size="large"
          type="primary"
          icon={<Plus size={16} />}
          loading={loading}
        >
          {l('models.register.submit')}
        </Button>
        <Button onClick={handleCancel} size="large">
          {l('global.actions.cancel')}
        </Button>
      </div>
    </div>
  );
};
export default RegisterForm;
