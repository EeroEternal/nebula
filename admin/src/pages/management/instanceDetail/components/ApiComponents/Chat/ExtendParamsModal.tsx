import { FC } from 'react';
import { Wrench, Copy } from 'lucide-react';
import { Button, Tooltip } from 'antd';
import {
  ModalForm,
  ProForm,
  ProFormSelect,
  ProFormSwitch,
  ProFormTextArea,
} from '@ant-design/pro-components';
import { InputNumberWithSlider } from '@/components';
import { l } from '@/utils/intl';
import { has } from 'lodash';
import { copyToClipboard, isJSON } from '@/utils';

const toolsExample = JSON.stringify(
  [
    {
      type: 'function',
      function: {
        name: 'get_current_weather',
        description: 'Get the current weather',
        parameters: {
          type: 'object',
          properties: {
            location: {
              type: 'string',
              description: 'The city and country, eg. San Francisco, USA',
            },
            format: { type: 'string', enum: ['celsius', 'fahrenheit'] },
          },
          required: ['location', 'format'],
        },
      },
    },
  ],
  null,
  2,
);

export type ChatExtendParams = {
  temperature?: number;
  max_tokens?: number;
  top_k?: number;
  enable_thinking?: boolean;
  lora_name?: string;
  tools?: unknown;
};

interface ExtendParamsModalProps {
  showTools: boolean;
  showThinking: boolean;
  maxTokens: number;
  loraOptions: { label: string; value: string }[];
  initialValues: ChatExtendParams;
  submit: (v: ChatExtendParams) => void;
}

const ExtendParamsModal: FC<ExtendParamsModalProps> = ({
  showTools,
  showThinking,
  maxTokens,
  loraOptions,
  initialValues,
  submit,
}) => {
  const settingExtendParams = async (values: ChatExtendParams) => {
    const next: ChatExtendParams = { ...values };
    if (has(values, 'tools') && typeof values.tools === 'string' && isJSON(values.tools)) {
      next.tools = JSON.parse(values.tools);
    }
    if (!showThinking) {
      delete next.enable_thinking;
    }
    submit(next);
    return true;
  };
  const copyToolsExample = () => {
    copyToClipboard(toolsExample);
  };
  const tokenCap = Math.max(1, Math.floor(maxTokens) || 8192);

  return (
    <ModalForm<ChatExtendParams>
      title={l('global.setting')}
      width={520}
      initialValues={initialValues}
      modalProps={{
        centered: true,
        destroyOnHidden: true,
      }}
      trigger={
        <Tooltip title={l('model.running.chat.setParams')}>
          <Button className="border-[#fff]" shape="circle" icon={<Wrench size={14} />} />
        </Tooltip>
      }
      onFinish={settingExtendParams}
    >
      <div className="flex flex-col gap-4">
        <ProForm.Item name="temperature" className="!mb-0">
          <InputNumberWithSlider
            label="temperature"
            min={0}
            max={1}
            step={0.01}
            precision={2}
          />
        </ProForm.Item>
        <ProForm.Item name="top_k" className="!mb-0">
          <InputNumberWithSlider label="top_k" min={0} max={1} step={0.01} precision={2} />
        </ProForm.Item>
        <ProForm.Item name="max_tokens" className="!mb-0">
          <InputNumberWithSlider
            label="max_tokens"
            min={0}
            max={tokenCap}
            step={1}
            precision={0}
          />
        </ProForm.Item>
        {showThinking && (
          <ProFormSwitch
            name="enable_thinking"
            label={l('model.running.chat.enableThinking')}
            checkedChildren={l('global.yes')}
            unCheckedChildren={l('global.no')}
          />
        )}
        {loraOptions.length > 0 && (
          <ProFormSelect
            name="lora_name"
            label="lora_name"
            options={loraOptions}
            placeholder={l('model.running.placeholder.select')}
          />
        )}
        {showTools && (
          <ProFormTextArea
            label="tools"
            name="tools"
            fieldProps={{
              autoSize: { minRows: 5 },
            }}
            tooltip={{
              title: (
                <div className="whitespace-pre-wrap text-[12px] leading-none relative">
                  {toolsExample}
                  <Copy
                    size={14}
                    className="absolute right-[5px] top-[5px] cursor-pointer"
                    onClick={copyToolsExample}
                  />
                </div>
              ),
              overlayClassName: '!max-w-[300px]',
            }}
            placeholder="A list of tools the model may generate JSON inputs for."
          />
        )}
      </div>
    </ModalForm>
  );
};
export default ExtendParamsModal;
