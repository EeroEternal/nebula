import { REGISTER_TABS } from '@/constants/modelData';
import { Check, Copy, FileCode2, SlidersHorizontal } from 'lucide-react';
import { ProForm } from '@ant-design/pro-components';
import { Button, Form } from 'antd';
import { omit, size } from 'lodash';
import { useEffect, useState, useCallback } from 'react';

import { MODEL_DEFAULT_VALUE, ModelType } from '@/constants/modelData';
import { copyToClipboard } from '@/utils';
import { l, lGet } from '@/utils/intl';
import { PageContainer, PillTabs } from '@/components';
import RegisterForm from './components/NewRegisterForm';
import ProCard from './components/ProCard';
import AutoFill from './components/AutoFill';

type ActivePanel = 'form' | 'json';

type ModelSpecFormItem = {
  model_size_in_billions?: string | number;
  [key: string]: unknown;
};

type RegisterFormValues = {
  model_ability?: unknown;
  model_specs?: ModelSpecFormItem[];
  virtualenv?: { packages?: Array<string | { value: string }> };
  stop_token_ids?: Array<string | number>;
  stop?: string[];
  stopController?: { stop_token_id: string | number; stop?: string }[];
  [key: string]: unknown;
};

const Repository: React.FC = () => {
  const [currentTab, setCurrentTab] = useState<ModelType>(ModelType.LLM);
  const modelDefaultValue = MODEL_DEFAULT_VALUE[currentTab] || {};
  const [activePanel, setActivePanel] = useState<ActivePanel>('form');
  const [copied, setCopied] = useState(false);
  const [form] = Form.useForm();

  const handleTabChange = (key: string) => {
    const newValues = MODEL_DEFAULT_VALUE[key as ModelType] || {};
    setCurrentTab(key as ModelType);
    setTimeout(() => form.setFieldsValue(newValues), 10);
  };
  const transformFormValues = (needStringify?: boolean, isSubmit?: boolean) => {
    let values = form.getFieldsValue();
    // 音频模型 - 模型能力需传值为 string[] 格式，因社区版 这里radio 要不要改为checkbox待确认？
    if (currentTab === ModelType.audio) {
      values.model_ability = [values.model_ability];
    }
    if (size(values?.virtualenv?.packages)) {
      values.virtualenv.packages = values.virtualenv.packages.map(
        (item: { value: string }) => item.value,
      );
    }
    if ('model_specs' in values) {
      values.model_specs = values.model_specs.map((item: ModelSpecFormItem) => {
        let newItem = item;
        if (currentTab === ModelType.LLM && isSubmit) {
          const sizeVal = item?.model_size_in_billions;
          newItem.model_size_in_billions =
            typeof sizeVal === 'string' && sizeVal.includes('.')
              ? sizeVal.replace('.', '_')
              : Number(sizeVal || 0);
        }
        return newItem;
      });
    }
    if ('stopController' in values) {
      const otherValues = omit(values, ['stopController']);
      const stopController: { stop_token_id: number; stop: string }[] = values.stopController || [];
      const stop_token_ids = stopController.map((item) => item.stop_token_id);
      const stop = stopController.map((item) => item.stop);
      return needStringify
        ? JSON.stringify({ ...otherValues, stop_token_ids, stop }, null, 2)
        : { ...otherValues, stop_token_ids, stop };
    }
    values = omit(values, ['worker_ip']);
    return needStringify ? JSON.stringify(values, null, 2) : values;
  };

  const handleCopyJson = () => {
    const values = transformFormValues();
    copyToClipboard(JSON.stringify(values, null, 2));
    setCopied(true);
    setTimeout(() => setCopied(false), 1000);
  };

  // JSON 语法高亮
  const highlightJson = useCallback((json: string) => {
    return json.replace(
      /("(\\u[a-zA-Z0-9]{4}|\\[^u]|[^\\"])*"(\s*:)?|\b(true|false|null)\b|-?\d+(?:\.\d*)?(?:[eE][+-]?\d+)?)/g,
      (match) => {
        let cls = 'text-amber-600 dark:text-amber-400'; // number
        let text = match;

        if (/^"/.test(match)) {
          if (/:$/.test(match)) {
            cls = 'text-primary font-medium'; // key
            text = match.replace(/:$/, '');
            return `<span class="${cls}">${text}</span>:`;
          } else {
            cls = 'text-emerald-600 dark:text-emerald-400'; // string
          }
        } else if (/true|false/.test(match)) {
          cls = 'text-blue-600 dark:text-blue-400'; // boolean
        } else if (/null/.test(match)) {
          cls = 'text-gray-500'; // null
        }

        return `<span class="${cls}">${text}</span>`;
      },
    );
  }, []);
  const transformValuesToForm = (values: RegisterFormValues = {}) => {
    let newValues = { ...values };
    const stopIds = newValues.stop_token_ids;
    const stop = newValues.stop;
    if (size(stopIds) && size(stop)) {
      newValues.stopController = (stopIds || []).map((item, index) => ({
        stop_token_id: item,
        stop: (stop || [])[index],
      }));
      newValues = omit(newValues, ['stop_token_ids', 'stop']);
    }
    const packages = newValues.virtualenv?.packages;
    if (size(packages)) {
      newValues.virtualenv = {
        ...newValues.virtualenv,
        packages: (packages || []).map((item) =>
          typeof item === 'string' ? { value: item } : item,
        ),
      };
    }
    const specs = newValues.model_specs;
    if (size(specs) && specs) {
      newValues.model_specs = specs.map((item: ModelSpecFormItem) => ({
        ...item,
        model_size_in_billions:
          typeof item?.model_size_in_billions === 'string' &&
          item.model_size_in_billions.includes('_')
            ? item.model_size_in_billions.replace('_', '.')
            : item?.model_size_in_billions,
      }));
    }
    return newValues;
  };
  const onAutoFillCallBack = (values: RegisterFormValues) => {
    const newValues = transformValuesToForm(values);
    form.setFieldsValue(newValues);
  };
  useEffect(() => {
    form.setFieldsValue(modelDefaultValue);
  }, []);

  const isJsonPreview = activePanel === 'json';

  return (
    <PageContainer
      title={l('menu.models.register')}
      subTitle={l('models.register.subTitle')}
    >
      <div className="flex flex-col w-full gap-6">
        <div className="flex items-center justify-between gap-2 flex-wrap">
          <div className="overflow-x-auto pb-0.5 min-w-0">
            <PillTabs
              value={currentTab}
              options={REGISTER_TABS.map((item) => ({
                value: item,
                label: l(`global.model.type.${item}`),
              }))}
              onChange={(key) => handleTabChange(String(key))}
            />
          </div>
          <div className="flex items-center gap-2 shrink-0">
            <Button
              type={isJsonPreview ? 'primary' : 'default'}
              ghost={isJsonPreview}
              className="h-[36px]"
              icon={isJsonPreview ? <SlidersHorizontal size={16} /> : <FileCode2 size={16} />}
              onClick={() => setActivePanel((p) => (p === 'form' ? 'json' : 'form'))}
            >
              {isJsonPreview
                ? String(lGet('models.register.backToForm'))
                : String(lGet('models.register.jsonPreview'))}
            </Button>
            {currentTab === ModelType.LLM && <AutoFill submitCallBack={onAutoFillCallBack} />}
          </div>
        </div>
        {isJsonPreview ? (
          <ProCard
            title={l('models.register.jsonPreview')}
            bodyStyle={{ padding: '0px 0px 24px 0px' }}
            className="w-full rounded-2xl border border-border/50"
            extra={
              <div
                className="w-7 h-7 inline-flex items-center justify-center cursor-pointer rounded-md hover:bg-background"
                onClick={handleCopyJson}
              >
                {copied ? <Check size={16} className="text-green-500" /> : <Copy size={16} />}
              </div>
            }
          >
            <div className="h-[calc(100vh-220px)] overflow-y-auto">
              <Form form={form} component={false}>
                <Form.Item noStyle shouldUpdate>
                  {() => (
                    <pre
                      className="p-4 text-xs font-mono bg-background-muted/30 whitespace-pre-wrap break-all"
                      dangerouslySetInnerHTML={{
                        __html: highlightJson(
                          JSON.stringify(transformFormValues(false), null, 2),
                        ),
                      }}
                    />
                  )}
                </Form.Item>
              </Form>
            </div>
          </ProCard>
        ) : (
          <ProForm form={form} layout="vertical" submitter={false} grid>
            <div className="flex w-full">
              <RegisterForm
                currentTab={currentTab}
                form={form}
                transformFormValues={transformFormValues}
              />
            </div>
          </ProForm>
        )}
      </div>
    </PageContainer>
  );
};

export default Repository;
