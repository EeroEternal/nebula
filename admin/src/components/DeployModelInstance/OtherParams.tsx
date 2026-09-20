import { Button, Form, AutoComplete, Input, Switch, Tooltip } from 'antd';
import type { FormInstance } from 'antd';
import { Plus, Trash2 } from 'lucide-react';
import {
  useRef,
  useState,
  forwardRef,
  useImperativeHandle,
  useMemo,
  useEffect,
} from 'react';
import { ProFormList, ProFormText } from '@ant-design/pro-components';
import type { FormListActionType } from '@ant-design/pro-components';
import { size } from 'lodash';
import { IconButton } from '@/components';
import { l, lGet } from '@/utils/intl';
import {
  ModelType,
  MODEL_QUANTIZATION_TEMPLATE,
} from '@/constants/modelData';
import { sleep } from '@/utils';
import { isSystemExtendItem } from '@/utils/ggufVllmDtype';
import { extendConfigOptionsForEngine } from '@/utils/vllmExtendDefaults';
import type { ValueType } from '@/types/Public/data';
import ProCardForConfig from './ProCardForConfig';
import {
  applyIbEnv,
  inferIbOwnedKeys,
  removeIbEnv,
  saveIbEnvCache,
  snapshotOwned,
} from './ibDeployEnv';

const baseFormListProps = {
  initialValue: [],
  copyIconProps: false as false,
  creatorButtonProps: false as false,
  className: '!mb-0',
};
/** 首行 label 高度 */
const labelHeight = 48;
/** formList 单行高度 */
const lineHeight = 48;
export interface OtherParamsMethod {
  updateActivePanels: (newPanels: string[]) => void;
}

interface OtherParamsProps {
  form: FormInstance;
  modelType: ModelType;
  modalBodyRef: HTMLDivElement | null;
}
const OtherParams = forwardRef<OtherParamsMethod, OtherParamsProps>(
  ({ form, modelType, modalBodyRef }, ref) => {
    const isVideo = modelType === ModelType.video;
    const showLora = [ModelType.image, ModelType.video].includes(modelType);
    const showLoraKwargs = [ModelType.image, ModelType.video].includes(modelType);
    const peftModelConfigValue = Form.useWatch(['peft_model_config'], form);
    const extendConfigValue = Form.useWatch(['kwargs', 'extend_config'], form);
    const quantizationConfigValue = Form.useWatch(['kwargs', 'quantization_config'], form);
    const modelEngineValue = Form.useWatch('model_engine', form);
    const virtualEnvConfigValue = Form.useWatch(['virtual_env_config'], form);
    const enableIbValue = Form.useWatch(['kwargs', 'enable_ib'], form);
    const [activePanels, setActivePanels] = useState<string[]>([]);
    const showQuantizationConfig = modelEngineValue === 'Transformers';
    const showIbToggle = ['vllm', 'vllm-ascend'].includes(
      String(modelEngineValue || '').toLowerCase(),
    );
    const ibOwnedRef = useRef<string[]>([]);
    const ibPreRef = useRef<Record<string, string>>({});

    const loraConfigRef = useRef<FormListActionType>();
    const extendConfigRef = useRef<FormListActionType>();
    const imageLoraLoadKwargsRef = useRef<FormListActionType>();
    const imageLoraFuseKwargsRef = useRef<FormListActionType>();
    const quantizationConfigRef = useRef<FormListActionType>();
    const envsConfigRef = useRef<FormListActionType>();

    const panelConfigMap = useMemo(() => {
      return {
        lora_list: { ref: loraConfigRef, hasValue: size(peftModelConfigValue?.lora_list) },
        extend_config: { ref: extendConfigRef, hasValue: size(extendConfigValue) },
        image_lora_load_kwargs: {
          ref: imageLoraLoadKwargsRef,
          hasValue: size(peftModelConfigValue?.image_lora_load_kwargs),
        },
        image_lora_fuse_kwargs: {
          ref: imageLoraFuseKwargsRef,
          hasValue: size(peftModelConfigValue?.image_lora_fuse_kwargs),
        },
        quantization_config: {
          ref: quantizationConfigRef,
          hasValue: size(quantizationConfigValue),
        },
        envs_config: { ref: envsConfigRef, hasValue: size(virtualEnvConfigValue?.envs) },
      };
    }, [
      loraConfigRef,
      extendConfigRef,
      imageLoraLoadKwargsRef,
      imageLoraFuseKwargsRef,
      quantizationConfigRef,
      envsConfigRef,
      peftModelConfigValue,
      extendConfigValue,
      quantizationConfigValue,
      virtualEnvConfigValue,
    ]);

    const modadScroll = async (num: number) => {
      await sleep(200);
      if (modalBodyRef) {
        modalBodyRef.scrollTo({
          top: modalBodyRef.scrollTop + num,
          behavior: 'smooth',
        });
      }
    };

    const addButton = async (e: React.MouseEvent, key: string) => {
      e.stopPropagation();
      const isOpen = activePanels.includes(key);
      // 点击card右侧的加号，判断当前的card是否展开，没有的话，则设为打开
      if (!isOpen) {
        setActivePanels([...activePanels, key]);
      }
      const { ref } = panelConfigMap[key as keyof typeof panelConfigMap];
      ref.current?.add?.({});
      const length = size(ref.current?.getList?.()) || 1;
      // 若当前panel打开，length必有值，固定向下滚动 单行高度， 否则向下滚动动态计算，判断当前formlist 有几行 * 单行高度，在加上label的高度
      modadScroll(isOpen ? lineHeight : labelHeight + length * lineHeight);
    };
    const onRemove = (name: number, key: string) => {
      const { ref } = panelConfigMap[key as keyof typeof panelConfigMap];
      ref.current?.remove?.(name);
      // 如果都删除了，则把当前的card收起
      if (size(ref.current?.getList?.()) === 0) {
        setActivePanels(activePanels.filter((item) => item !== key));
      }
    };
    const renderButton = (key: string) => {
      return (
        <Button
          size="small"
          type="primary"
          shape="circle"
          onClick={(e) => addButton(e, key)}
          icon={<Plus size={14} />}
        />
      );
    };
    const updateActivePanels = (newPanels: string[]) => {
      setActivePanels([...new Set([...activePanels, ...newPanels])]);
    };
    useImperativeHandle(ref, () => ({
      updateActivePanels,
    }));

    useEffect(() => {
      if (!enableIbValue || ibOwnedRef.current.length) return;
      const inferred = inferIbOwnedKeys(virtualEnvConfigValue?.envs);
      if (inferred.length) ibOwnedRef.current = inferred;
    }, [enableIbValue, virtualEnvConfigValue?.envs]);

    useEffect(() => {
      if (!enableIbValue || !ibOwnedRef.current.length) return;
      const snap = snapshotOwned(virtualEnvConfigValue?.envs, ibOwnedRef.current);
      if (Object.keys(snap).length) saveIbEnvCache(snap);
    }, [enableIbValue, virtualEnvConfigValue?.envs]);

    const handleIbToggle = (checked: boolean) => {
      form.setFieldValue(['kwargs', 'enable_ib'], checked);
      const envs = (form.getFieldValue(['virtual_env_config', 'envs']) || []) as ValueType[];
      if (checked) {
        const { next, owned, pre } = applyIbEnv(envs);
        ibOwnedRef.current = owned;
        ibPreRef.current = pre;
        form.setFieldValue(['virtual_env_config', 'envs'], next);
        updateActivePanels(['envs_config']);
        return;
      }
      const { next, cache } = removeIbEnv(envs, ibOwnedRef.current, ibPreRef.current);
      saveIbEnvCache(cache);
      ibOwnedRef.current = [];
      ibPreRef.current = {};
      form.setFieldValue(['virtual_env_config', 'envs'], next);
    };

    const cardConfig = [
      {
        key: 'envs_config',
        // Container LLM: env vars are injected into the engine container.
        show: true,
        title: l('models.deploy.envsConfigConfig'),
        extra: (
          <div
            className="flex items-center gap-2"
            onClick={(e) => e.stopPropagation()}
          >
            {showIbToggle ? (
              <Tooltip
                title={lGet('models.deploy.enableIb.tips',
                )}
              >
                <span className="inline-flex items-center gap-1.5 text-xs">
                  <span>{l('models.deploy.enableIb')}</span>
                  <Switch
                    size="small"
                    checked={!!enableIbValue}
                    onChange={handleIbToggle}
                  />
                </span>
              </Tooltip>
            ) : null}
            {renderButton('envs_config')}
          </div>
        ),
        children: (
          <>
            <Form.Item name={['kwargs', 'enable_ib']} hidden valuePropName="checked">
              <Input type="hidden" />
            </Form.Item>
            <ProFormList
              {...baseFormListProps}
              actionRef={envsConfigRef}
              name={['virtual_env_config', 'envs']}
              actionRender={(field) => [
                <IconButton
                  key="delete"
                  onClick={() => onRemove(field.name, 'envs_config')}
                  className="-mt-1 !w-6 !h-6 rounded-md group"
                >
                  <Trash2 size={16} className="group-hover:text-danger text-muted" />
                </IconButton>,
              ]}
            >
              <div className="grid grid-cols-2 gap-3">
                <ProFormText
                  placeholder="key"
                  name="key"
                  label="key"
                  rules={[{ required: true, message: 'key is required' }]}
                />
                <ProFormText
                  placeholder="value"
                  name="value"
                  label="value"
                />
              </div>
            </ProFormList>
          </>
        ),
      },
      {
        key: 'lora_list',
        show: showLora,
        title: l('models.deploy.loraConfig'),
        extra: renderButton('lora_list'),
        children: (
          <ProFormList
            {...baseFormListProps}
            actionRef={loraConfigRef}
            name={['peft_model_config', 'lora_list']}
            actionRender={(field) => [
              <IconButton
                key="delete"
                onClick={() => onRemove(field.name, 'lora_list')}
                className="-mt-1 !w-6 !h-6 rounded-md group"
              >
                <Trash2 size={16} className="group-hover:text-danger text-muted" />
              </IconButton>,
            ]}
          >
            <div className="grid grid-cols-2 gap-3">
              <ProFormText
                placeholder="Lora Name"
                name="lora_name"
                label="Lora Name"
                rules={[{ required: true, message: 'Lora Name is required' }]}
              />
              <ProFormText
                placeholder="Local Path"
                name="local_path"
                label="Local Path"
                rules={[{ required: true, message: 'Lora Path is required' }]}
              />
            </div>
          </ProFormList>
        ),
      },
      {
        key: 'image_lora_load_kwargs',
        show: showLoraKwargs,
        title: l('models.deploy.loraLoadKwargs', undefined, {
          value: isVideo ? 'Video' : 'Image',
        }),
        extra: renderButton('image_lora_load_kwargs'),
        children: (
          <ProFormList
            {...baseFormListProps}
            actionRef={imageLoraLoadKwargsRef}
            name={['peft_model_config', 'image_lora_load_kwargs']}
            actionRender={(field) => [
              <IconButton
                key="delete"
                onClick={() => onRemove(field.name, 'image_lora_load_kwargs')}
                className="-mt-1 !w-6 !h-6 rounded-md group"
              >
                <Trash2 size={16} className="group-hover:text-danger text-muted" />
              </IconButton>,
            ]}
          >
            <div className="grid grid-cols-2 gap-3">
              <ProFormText
                placeholder="key"
                name="key"
                label="key"
                rules={[{ required: true, message: 'key is required' }]}
              />
              <ProFormText
                placeholder="value"
                name="value"
                label="value"
                rules={[{ required: true, message: 'value is required' }]}
              />
            </div>
          </ProFormList>
        ),
      },
      {
        key: 'image_lora_fuse_kwargs',
        show: showLoraKwargs,
        title: l('models.deploy.loraFuseKwargs', undefined, {
          value: isVideo ? 'Video' : 'Image',
        }),
        extra: renderButton('image_lora_fuse_kwargs'),
        children: (
          <ProFormList
            {...baseFormListProps}
            actionRef={imageLoraFuseKwargsRef}
            name={['peft_model_config', 'image_lora_fuse_kwargs']}
            actionRender={(field) => [
              <IconButton
                key="delete"
                onClick={() => onRemove(field.name, 'image_lora_fuse_kwargs')}
                className="-mt-1 !w-6 !h-6 rounded-md group"
              >
                <Trash2 size={16} className="group-hover:text-danger text-muted" />
              </IconButton>,
            ]}
          >
            <div className="grid grid-cols-2 gap-3">
              <ProFormText
                placeholder="key"
                name="key"
                label="key"
                rules={[{ required: true, message: 'key is required' }]}
              />
              <ProFormText
                placeholder="value"
                name="value"
                label="value"
                rules={[{ required: true, message: 'value is required' }]}
              />
            </div>
          </ProFormList>
        ),
      },
      {
        key: 'quantization_config',
        show: showQuantizationConfig,
        title: l('models.deploy.quantizationConfig'),
        extra: renderButton('quantization_config'),
        children: (
          <ProFormList
            {...baseFormListProps}
            actionRef={quantizationConfigRef}
            name={['kwargs', 'quantization_config']}
            actionRender={(field) => [
              <IconButton
                key="delete"
                onClick={() => onRemove(field.name, 'quantization_config')}
                className="-mt-1 !w-6 !h-6 rounded-md group"
              >
                <Trash2 size={16} className="group-hover:text-danger text-muted" />
              </IconButton>,
            ]}
          >
            <div className="grid grid-cols-2 gap-3">
              <ProFormText
                placeholder="key"
                name="key"
                label="key"
                rules={[{ required: true, message: 'key is required' }]}
              >
                <AutoComplete options={MODEL_QUANTIZATION_TEMPLATE} placeholder="key" allowClear />
              </ProFormText>
              <ProFormText
                placeholder="value"
                name="value"
                label="value"
                rules={[{ required: true, message: 'value is required' }]}
              />
            </div>
          </ProFormList>
        ),
      },
      {
        key: 'extend_config',
        show: true,
        title: `${l('models.deploy.extendConfig')}${
          modelEngineValue ? `：${modelEngineValue}` : ''
        }`,
        extra: renderButton('extend_config'),
        children: (
          <ProFormList
            {...baseFormListProps}
            actionRef={extendConfigRef}
            name={['kwargs', 'extend_config']}
            actionRender={(field) => [
              <IconButton
                key="delete"
                onClick={() => onRemove(field.name, 'extend_config')}
                className="-mt-1 !w-6 !h-6 rounded-md group"
              >
                <Trash2 size={16} className="group-hover:text-danger text-muted" />
              </IconButton>,
            ]}
          >
            {(meta) => {
              const row = form.getFieldValue([
                'kwargs',
                'extend_config',
                meta.name,
              ]) as ValueType | undefined;
              const isSystem = isSystemExtendItem(row);
              return (
                <div className="grid grid-cols-2 gap-3">
                  {/* preserve system flag across form edits */}
                  <Form.Item name="system" hidden>
                    <Input type="hidden" />
                  </Form.Item>
                  <ProFormText
                    placeholder="key"
                    name="key"
                    label="key"
                    rules={[{ required: true, message: 'key is required' }]}
                    disabled={isSystem}
                    tooltip={isSystem ? l('models.deploy.extendConfig.system') : undefined}
                  >
                    <AutoComplete
                      options={extendConfigOptionsForEngine(modelEngineValue as string)}
                      placeholder="key"
                      allowClear={!isSystem}
                      disabled={isSystem}
                    />
                  </ProFormText>
                  <ProFormText
                    placeholder="value"
                    name="value"
                    label="value"
                    rules={[{ required: true, message: 'value is required' }]}
                  />
                </div>
              );
            }}
          </ProFormList>
        ),
      },
    ];
    const handleCardCollapse = (collapsed: boolean, key: keyof typeof panelConfigMap) => {
      if (!panelConfigMap[key].hasValue) {
        panelConfigMap[key].ref.current?.add?.({});
      }
      setActivePanels(
        collapsed ? activePanels.filter((item) => item !== key) : [...activePanels, key],
      );
    };
    return (
      <>
        {cardConfig.map((item) => {
          if (item.show) {
            return (
              <ProCardForConfig
                key={item.key}
                title={item.title}
                collapsed={!activePanels.includes(item.key)}
                onCollapse={(collapsed) =>
                  handleCardCollapse(collapsed, item.key as keyof typeof panelConfigMap)
                }
                extra={item?.extra}
              >
                {item.children}
              </ProCardForConfig>
            );
          }
          return null;
        })}
      </>
    );
  },
);
export default OtherParams;
