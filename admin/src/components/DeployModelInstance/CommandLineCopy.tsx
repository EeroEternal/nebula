import type { FormInstance } from 'antd';
import { FC } from 'react';
import { omit } from 'lodash';
import { Copy } from 'lucide-react';
import { ActionWithTips, IconButton } from '@/components';
import { l } from '@/utils/intl';
import { copyToClipboard } from '@/utils';
import { ValueType } from '@/types/Public/data';
import { VIRTUAL_ENV_UNSET } from '@/constants/modelData';
import type { FormValues } from './index';

type ExtendedKeys = 'extend_config' | 'quantization_config';

type AllKeys = keyof FormValues | ExtendedKeys | 'enable_thinking' | 'enable_ib';

/** 不需要 _ 转化为 - 的 key（后端命名参数用下划线；CLI 正式选项见下） */
const noNeedReplaceKeys = [
  'download_hub',
  'reasoning_content',
  'lightning_version',
  'lightning_model_path',
];

function processItems<T extends Record<string, unknown>>(
  items: T[],
  mapperOrPrefix: ((item: T) => string) | string,
  key1: string = 'key',
  key2: string = 'value',
  noKey1: boolean = false,
): string[] {
  return (items || [])
    .filter((item: T) => (noKey1 ? item?.[key2] : item?.[key1] && item?.[key2]))
    .map((item) => {
      if (typeof mapperOrPrefix === 'function') {
        return mapperOrPrefix(item);
      } else {
        return noKey1
          ? `${mapperOrPrefix} ${String(item?.[key2] ?? '')}`
          : `${mapperOrPrefix} ${String(item?.[key1] ?? '')} ${String(item?.[key2] ?? '')}`;
      }
    });
}
const CommandLineCopy: FC<{ form: FormInstance }> = ({ form }) => {
  const handleCopy = () => {
    let formValues = form.getFieldsValue();
    const isLLM = formValues.model_type === 'LLM';
    if (!isLLM) {
      // 非大语言模型，过滤掉模型版本
      formValues = omit(formValues, ['model_version']);
    }
    const processField = ([key, value]: [AllKeys, unknown]): string | null => {
      const rec =
        value && typeof value === 'object' && !Array.isArray(value)
          ? (value as Record<string, unknown>)
          : {};
      if (key === 'virtual_env_config') {
        // LLM 容器路径：虚环开关/包无意义，仅保留 --env
        const enableVirtualEnv =
          isLLM || rec.enable_virtual_env === VIRTUAL_ENV_UNSET
            ? ''
            : rec.enable_virtual_env
            ? '--enable-virtual-env'
            : '--disable-virtual-env';
        const virtualPackages = isLLM
          ? []
          : processItems<ValueType>(
              rec.virtual_env_packages as ValueType[],
              '--virtual-env-package',
              'key',
              'value',
              true,
            );
        const envs = processItems<ValueType>(rec.envs as ValueType[], '--env');
        return (
          ([] as string[])
            .concat(enableVirtualEnv ? [enableVirtualEnv] : [])
            .concat([...virtualPackages, ...envs])
            .join(' ') || null
        );
      }
      if (key === 'peft_model_config') {
        if (isLLM) return null;
        const loraListArgs = processItems<{ lora_name: string; local_path: string }>(
          rec.lora_list as { lora_name: string; local_path: string }[],
          '--lora-modules',
          'lora_name',
          'local_path',
        );
        const imageLoraLoadKwargsArgs = processItems<ValueType>(
          rec.image_lora_load_kwargs as ValueType[],
          '--image-lora-load-kwargs',
        );
        const imageLoraFuseKwargsArgs = processItems<ValueType>(
          rec.image_lora_fuse_kwargs as ValueType[],
          '--image-lora-fuse-kwargs',
        );
        return (
          [...loraListArgs, ...imageLoraLoadKwargsArgs, ...imageLoraFuseKwargsArgs].join(' ') ||
          null
        );
      } else if (key === 'model_version') {
        if (!value) return null;
        const [, sizeInBillions, modelFormat, quantization] = String(value).split('--');
        return `--model-format ${modelFormat} --size-in-billions ${sizeInBillions.replace(
          /B/g,
          '',
        )} --quantization ${quantization || 'none'}`;
      } else if (key === 'extend_config') {
        return (
          processItems<ValueType>(
            value as ValueType[],
            (item) => `--${item.key} ${item.value}`,
          ).join(' ') || null
        );
      } else if (key === 'quantization_config') {
        return processItems<ValueType>(value as ValueType[], '--quantization-config').join(' ') || null;
      } else if (key === 'replica_config') {
        const replicaRows = Array.isArray(value) ? value : [];
        const newValue = JSON.stringify(
          replicaRows.map((item: { replica_uid?: string; devices?: unknown }) => ({
            replica_uid: item.replica_uid,
            devices: item.devices,
          })),
        );
        return `--replica-config '${newValue}'`;
      } else if (key === 'engine_version') {
        if (value == null || value === '') return null;
        return `--engine-version ${value}`;
      } else if (key === 'kwargs') {
        // 如果是kwargs, 则继续循环kwargs 对象，解析extend_config， quantization_config，enable_thinking等参数
        return Object.entries(rec)
          .map((item) => processField(item as [AllKeys, unknown]))
          .filter((arg) => arg !== null)
          .join(' ');
      } else if (key === 'enable_thinking') {
        if (value === true) return '--enable-thinking';
        if (value === false) return '--no-enable-thinking';
        return null;
      } else if (key === 'enable_ib') {
        return null;
      }
      if (value === null || value === undefined) return null;
      if (typeof value === 'boolean') {
        // 其它布尔：所见即所得（开/关用 flag）
        const flag = `--${String(key).replace(/_/g, '-')}`;
        return value ? flag : null;
      }

      return `--${noNeedReplaceKeys.includes(key) ? key : key.replace(/_/g, '-')} ${value}`;
    };

    // kwargs.enable_thinking → 顶层 flag（与 UI Switch 一致）
    const kwargs = formValues.kwargs || {};
    if (Object.prototype.hasOwnProperty.call(kwargs, 'enable_thinking')) {
      formValues = {
        ...formValues,
        enable_thinking: kwargs.enable_thinking,
        kwargs: omit(kwargs, ['enable_thinking', 'enable_ib']),
      };
    }

    const args = Object.entries(formValues)
      .map((item) => processField(item as [AllKeys, unknown]))
      .filter((arg) => arg !== null && arg !== '')
      .join(' ');

    copyToClipboard(`powerllm launch ${args}`);
  };
  return (
    <ActionWithTips title={l('models.deploy.commandLineCopy')} mouseEnterDelay={0.1} placement="top">
      <IconButton
        className="!w-8 !h-8 text-muted !rounded-[6px] hover:!bg-black-06 hover:!text-black"
        onClick={handleCopy}
      >
        <Copy size={18} />
      </IconButton>
    </ActionWithTips>
  );
};
export default CommandLineCopy;
