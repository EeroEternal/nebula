import { ModalForm, ProFormTextArea } from '@ant-design/pro-components';
import { SquareTerminal } from 'lucide-react';
import { Form } from 'antd';
import { FC, useState } from 'react';
import { size, omit } from 'lodash';

import { ActionWithTips, IconButton } from '@/components';
import { l } from '@/utils/intl';
import { isJSON } from '@/utils';

interface CommandLineModalProps {
  onSubmitBack: (values: Record<string, unknown>) => void;
}

const CommandLineModal: FC<CommandLineModalProps> = ({ onSubmitBack }) => {
  const [open, setOpen] = useState(false);
  const [form] = Form.useForm();

  const parseCommandToObject = (command: string) => {
    // 初始化参数对象
    let params: Record<string, unknown> = {};
    const peftModelConfig: {
      lora_list: { lora_name: string; local_path: string }[];
      image_lora_load_kwargs: Record<string, string>;
      image_lora_fuse_kwargs: Record<string, string>;
    } = {
      lora_list: [],
      image_lora_load_kwargs: {},
      image_lora_fuse_kwargs: {},
    };
    const ensureRec = (parent: Record<string, unknown>, key: string) => {
      const cur = parent[key];
      if (cur && typeof cur === 'object' && !Array.isArray(cur)) {
        return cur as Record<string, unknown>;
      }
      const next: Record<string, unknown> = {};
      parent[key] = next;
      return next;
    };
    // 移除命令前缀 'powerllm launch' 并去除前后空白字符
    let newCommand = command.replace('powerllm launch', '').trim();
    // 使用正则表达式匹配所有参数
    const args =
      newCommand.match(/--[\w-]+(?:\s+(?:"[^"]*"|'[^']*'|(?:[^\s-][^\s]*\s*)+))?/g) || [];
    args.forEach((arg: string) => {
      // 提取键值对
      const match = arg.trim().match(/^--([\w-]+)(?:\s+(?:"([^"]+)"|'([^']+)'|(.+)))?$/);
      if (!match) return;
      const key = match[1]; // 参数名
      const value = match[2] || match[3] || match[4] || ''; // 参数值
      const normalizedKey = key.replace(/-/g, '_'); // 将参数名中的 - 替换为 _

      switch (normalizedKey) {
        case 'lora_modules': {
          // 处理 lora-modules，将其拆分为 lora_name 和 local_path
          const loraPairs: string[] = value.split(/\s+/);
          // [{lora_name: 'name', local_path: 'path'}]
          peftModelConfig.lora_list = loraPairs.reduce((acc, _, i, arr) => {
            if (i % 2 === 0 && arr[i] && arr[i + 1]) {
              acc.push({ lora_name: arr[i], local_path: arr[i + 1] });
            }
            return acc;
          }, peftModelConfig.lora_list || []);
          break;
        }
        case 'replica':
        case 'request_limits':
        case 'replica_concurrency':
          params[normalizedKey] = Number(value);
          break;
        case 'image_lora_load_kwargs':
        case 'image_lora_fuse_kwargs': {
          // 处理 image_lora_load_kwargs, image_lora_fuse_kwargs -> { name: 1 }
          const [loadParam, loadValue] = value.split(/\s+/);
          peftModelConfig[normalizedKey] ||= {};
          peftModelConfig[normalizedKey][loadParam] = loadValue;
          break;
        }
        case 'cpu_offload':
        case 'reasoning_content':
          // 将布尔值字段转换为布尔类型（flag 无值视为 true）
          ensureRec(params, 'kwargs')[normalizedKey] = value === '' || value === 'true';
          break;
        case 'enable_thinking':
          // --enable-thinking / --enable-thinking true
          ensureRec(params, 'kwargs').enable_thinking = value === '' || value === 'true';
          break;
        case 'no_enable_thinking':
          // --no-enable-thinking（与 UI Switch 关态一致）
          ensureRec(params, 'kwargs').enable_thinking = false;
          break;
        case 'engine_version':
          params.engine_version = value;
          break;
        case 'quantization_config': {
          const [qKey, qVal] = value.split(/\s+/);
          ensureRec(ensureRec(params, 'kwargs'), normalizedKey)[qKey] = qVal;
          break;
        }
        case 'enable_virtual_env':
        case 'disable_virtual_env':
          ensureRec(params, 'virtual_env_config').enable_virtual_env =
            normalizedKey === 'enable_virtual_env';
          break;
        case 'virtual_env_package':
          ensureRec(params, 'virtual_env_config').virtual_env_packages =
            value.split(/\s+/) || [];
          break;
        case 'env': {
          const [envKey, envVal] = value.split(/\s+/);
          ensureRec(ensureRec(params, 'virtual_env_config'), 'envs')[envKey] = envVal;
          break;
        }
        case 'replica_config': {
          if (isJSON(value)) {
            params[normalizedKey] = JSON.parse(value);
          }
          break;
        }
        default:
          // 其他字段直接赋值
          params[normalizedKey] = value;
          break;
      }
    });

    if (size(peftModelConfig.lora_list)) {
      params.peft_model_config = peftModelConfig;
    }
    // 获取模型版本
    if (params.model_format && params.quantization && params.size_in_billions) {
      // 过滤掉size_in_billions, model_format, quantization，使用表单的model_version
      params.model_version = `${params.model_name}--${params.size_in_billions}B--${params.model_format}--${params.quantization}`;
      params = omit(params, ['size_in_billions', 'model_format', 'quantization']);
    }
    return params;
  };

  const onFinish = async (values: { command_line: string }) => {
    const commandValue = values.command_line;
    if (commandValue) {
      const params = parseCommandToObject(commandValue);
      onSubmitBack(params);
    }
    setOpen(false);
  };
  const onCancel = () => {
    form.resetFields();
    setOpen(false);
  };
  return (
    <ModalForm
      width={600}
      open={open}
      form={form}
      trigger={
        <ActionWithTips
          title={l('models.deploy.commandLineParsing')}
          mouseEnterDelay={0.1}
          placement="top"
        >
          <IconButton
            className="!w-8 !h-8 text-muted !rounded-[6px] hover:!bg-black-06 hover:!text-black"
            onClick={() => setOpen(true)}
          >
            <SquareTerminal size={18} />
          </IconButton>
        </ActionWithTips>
      }
      title={l('models.deploy.commandLineParsing')}
      onFinish={onFinish}
      modalProps={{
        onCancel,
      }}
    >
      <div></div>
      <ProFormTextArea
        fieldProps={{
          autoSize: { minRows: 4 },
        }}
        name="command_line"
      />
    </ModalForm>
  );
};
export default CommandLineModal;
