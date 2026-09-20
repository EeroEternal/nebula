import { useRequest } from 'ahooks';
import { Alert, Button, Form, Input, Result, Spin } from 'antd';
import { size } from 'lodash';
import { Copy, RefreshCw } from 'lucide-react';
import { FC, ReactNode, useEffect, useMemo, useRef, useState } from 'react';
import type { FormInstance } from 'antd/es/form';

import { useK8sRuntime } from '@/hooks/useK8sRuntime';
import type { ValueType } from '@/types/Public/data';
import { copyToClipboard, transformFormListToStringObj } from '@/utils';
import { readThrownMessage } from '@/utils/formatApiError';
import { l, lGet } from '@/utils/intl';
import request from '@/utils/request';

import { buildManifestLaunchKwargs } from './buildManifestKwargs';

const DOCKER_SKELETON = `# 先选 Worker，才能生成可执行清单。下列路径 / 端口仅为骨架占位，不是最终执行稿。
services:
  engine:
    image: # 必填，例如 vllm/vllm-openai:v0.8.0
    # network_mode: host
    # command: ["vllm", "serve", "/tmp/powerllm-manifest-preview/weights", "--host", "0.0.0.0", "--port", "8000"]
`;

const K8S_SKELETON = `# 先选 Worker，才能生成可执行清单。下列路径 / 端口仅为骨架占位，不是最终执行稿。
apiVersion: apps/v1
kind: Deployment
metadata:
  name: engine
spec:
  selector:
    matchLabels:
      app: engine
  template:
    metadata:
      labels:
        app: engine
    spec:
      hostNetwork: true
      containers:
        - name: engine
          image: # 必填
          args: ["vllm", "serve", "/tmp/powerllm-manifest-preview/weights", "--port", "8000"]
`;

type ReplicaPreview = {
  replica_uid?: string;
  devices?: Array<{
    worker_ip?: string;
    n_gpu?: string | number;
    gpu_idx?: number[];
    gpu_mem_gb?: number;
    gpu_cores?: number;
    gpu_type?: string;
    model_path?: string;
    role?: string;
  }>;
};

type ManifestPreviewItem = {
  key?: string;
  yaml?: string;
  replica_uid?: string;
  shard?: number | null;
  role?: string | null;
  final?: boolean;
  note?: string;
};

export type EngineManifestDraft = {
  key: string;
  yaml: string;
  replica_uid?: string;
  shard?: number | null;
  role?: string | null;
  /** 提交时是否写入磁盘：用户改 YAML / 部署配置变更触发的预览 / 显式重新生成 */
  shouldPersist?: boolean;
  /** 用户手改了 YAML 文本；表单部署不要丢这份稿 */
  userEdited?: boolean;
};

type ManifestYamlPreviewProps = {
  form: FormInstance;
  open: boolean;
  modelType?: string;
  /** Family ability (tools/reasoning) so preview CLI matches form launch. */
  modelAbility?: string[];
  /** 配置历史 / 失败现场回填的 YAML；预览刷新时优先生效 */
  seedManifests?: EngineManifestDraft[];
  onManifestsChange?: (items: EngineManifestDraft[]) => void;
};

/** Per-replica editable YAML tabs; ports are control-plane assigned at launch. */
const ManifestYamlPreview: FC<ManifestYamlPreviewProps> = ({
  form,
  open,
  modelType,
  modelAbility,
  seedManifests,
  onManifestsChange,
}) => {
  const modelUid = Form.useWatch('model_uid', form);
  const modelEngineWatched = Form.useWatch('model_engine', form);
  const engineVersion = Form.useWatch('engine_version', form);
  const modelVersion = Form.useWatch('model_version', form);
  const replica = Form.useWatch('replica', form);
  const replicaConfig = Form.useWatch('replica_config', form);
  const kwargs = Form.useWatch('kwargs', form);
  const virtualEnvConfig = Form.useWatch('virtual_env_config', form);
  // 切换面板时优先用 store 值，避免 watch 瞬时空
  const modelEngine = modelEngineWatched || form.getFieldValue('model_engine');
  const { isK8s } = useK8sRuntime();

  const [items, setItems] = useState<EngineManifestDraft[]>([]);
  const [runtime, setRuntime] = useState('');
  const [previewFinal, setPreviewFinal] = useState(true);
  const [previewNote, setPreviewNote] = useState('');
  const [formChangedWhileEdited, setFormChangedWhileEdited] = useState(false);
  /** 用户手改 YAML：后续预览保留 */
  const userEditedRef = useRef<Record<string, boolean>>({});
  /** seed 仅挡住「首次」预览覆盖；配置再变则用新预览并写盘 */
  const seedProtectRef = useRef<Record<string, boolean>>({});
  /** 提交时写盘 */
  const persistRef = useRef<Record<string, boolean>>({});
  const initialPreviewDoneRef = useRef(false);
  const forcePersistNextRef = useRef(false);

  const emitItems = (next: EngineManifestDraft[]) => {
    const withFlags = next.map((m) => ({
      ...m,
      shouldPersist: !!persistRef.current[m.key],
      userEdited: !!userEditedRef.current[m.key],
    }));
    onManifestsChange?.(withFlags);
    return withFlags;
  };

  // 配置历史回填：挡住首次 preview；未改配置则不写盘
  const seedSig = useMemo(
    () =>
      (seedManifests || [])
        .map((m) => `${m.key}:${(m.yaml || '').length}`)
        .join('|'),
    [seedManifests],
  );
  useEffect(() => {
    if (!open || !seedManifests?.length) return;
    const seeded = seedManifests.filter((m) => m?.key && m?.yaml);
    if (!seeded.length) return;
    const first = seeded[0];
    seedProtectRef.current[String(modelUid || first.key)] = true;
    seedProtectRef.current[first.key] = true;
    setItems(
      emitItems([
        {
          key: String(modelUid || first.key),
          yaml: first.yaml,
          replica_uid: String(modelUid || first.key),
        },
      ]),
    );
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [open, seedSig]);

  useEffect(() => {
    if (!open) {
      initialPreviewDoneRef.current = false;
      forcePersistNextRef.current = false;
      persistRef.current = {};
      userEditedRef.current = {};
      seedProtectRef.current = {};
      setFormChangedWhileEdited(false);
      setPreviewFinal(true);
      setPreviewNote('');
    }
  }, [open]);

  const replicaPayload = useMemo(() => {
    const list = (replicaConfig || form.getFieldValue('replica_config') || []).map(
      (rc: ReplicaPreview, idx: number) => ({
        replica_uid: rc?.replica_uid || `${modelUid || 'preview'}-${idx}`,
        devices: (rc?.devices || []).map((d) => ({
          worker_ip: d?.worker_ip,
          n_gpu: d?.n_gpu,
          gpu_idx: d?.gpu_idx,
          gpu_mem_gb: d?.gpu_mem_gb,
          gpu_cores: d?.gpu_cores,
          gpu_type: d?.gpu_type,
          model_path: d?.model_path,
          role: d?.role,
        })),
      }),
    );
    return list;
  }, [replicaConfig, modelUid, form]);

  const { loading, run, error } = useRequest(
    () => {
      const values = form.getFieldsValue(true);
      const engine = values.model_engine || modelEngine;
      const vec = values.virtual_env_config || virtualEnvConfig || {};
      const envsList = vec?.envs as ValueType[] | undefined;
      return request('/models/instance/manifest-preview', {
        method: 'post',
        data: {
          model_uid: values.model_uid || modelUid || 'preview',
          model_engine: engine,
          model_type: modelType || values.model_type || 'LLM',
          model_name: values.model_name || values.model_uid,
          model_ability: values.model_ability || modelAbility,
          model_path: values.model_path,
          engine_version: values.engine_version || engineVersion,
          model_format: String(values.model_version || modelVersion || '').split('--')[2],
          replica: values.replica || replica || replicaPayload.length || 1,
          replica_config: values.replica_config?.length
            ? values.replica_config.map((rc: ReplicaPreview, idx: number) => ({
                replica_uid: rc?.replica_uid || `${values.model_uid || 'preview'}-${idx}`,
                devices: (rc?.devices || []).map((d) => ({
                  worker_ip: d?.worker_ip,
                  n_gpu: d?.n_gpu,
                  gpu_idx: d?.gpu_idx,
                  gpu_mem_gb: d?.gpu_mem_gb,
                  gpu_cores: d?.gpu_cores,
                  gpu_type: d?.gpu_type,
                  model_path: d?.model_path,
                  role: d?.role,
                })),
              }))
            : replicaPayload,
          kwargs: buildManifestLaunchKwargs(values.kwargs || kwargs || {}),
          pd_separation: !!(values.kwargs || kwargs)?.enable_pd_separation,
          virtual_env_config: {
            envs: size(envsList) ? transformFormListToStringObj(envsList) : undefined,
          },
        },
      });
    },
    {
      manual: true,
      debounceWait: 450,
      onSuccess: (res) => {
        const body = res?.data?.data ?? res?.data ?? {};
        setRuntime(body.runtime || '');
        const firstItem = (body.items || [])[0] || {};
        setPreviewFinal(body.final !== false && firstItem.final !== false);
        setPreviewNote(String(firstItem.note || body.note || ''));
        const next: EngineManifestDraft[] = (body.items || []).map((it: ManifestPreviewItem) => ({
          key: it.key,
          yaml: it.yaml || '',
          replica_uid: it.replica_uid,
          shard: it.shard,
          role: it.role,
        }));
        setItems((prev) => {
          const cardKey = String(modelUid || body.key || next[0]?.key || 'instance');
          const previewYaml = (body.yaml || next[0]?.yaml || '') as string;
          const prevYaml = prev[0]?.yaml || '';
          const forceAll = forcePersistNextRef.current;
          const afterFirst = initialPreviewDoneRef.current;
          const keepUser =
            !!userEditedRef.current[cardKey] && !!prevYaml && !forceAll;
          const keepSeed =
            !forceAll && !!seedProtectRef.current[cardKey] && !!prevYaml;
          const yaml = keepUser || keepSeed ? prevYaml : previewYaml;
          if (keepUser) {
            setFormChangedWhileEdited(true);
          } else if (yaml) {
            persistRef.current[cardKey] = true;
            seedProtectRef.current[cardKey] = false;
            userEditedRef.current[cardKey] = false;
            setFormChangedWhileEdited(false);
          }
          forcePersistNextRef.current = false;
          initialPreviewDoneRef.current = true;
          void afterFirst;
          return emitItems([
            {
              key: cardKey,
              yaml,
              replica_uid: cardKey,
            },
          ]);
        });
      },
    },
  );

  useEffect(() => {
    if (!open) return;
    const engine = form.getFieldValue('model_engine') || modelEngine;
    if (!engine) {
      const cardKey = String(modelUid || 'preview');
      setPreviewFinal(false);
      setPreviewNote('');
      setItems((prev) => {
        if (prev[0]?.yaml) return prev;
        const yaml = isK8s ? K8S_SKELETON : DOCKER_SKELETON;
        return emitItems([{ key: cardKey, yaml, replica_uid: cardKey }]);
      });
      return;
    }
    const cardKey = String(modelUid || 'preview');
    if (initialPreviewDoneRef.current && userEditedRef.current[cardKey]) {
      setFormChangedWhileEdited(true);
      return;
    }
    seedProtectRef.current = {};
    run();
  }, [
    open,
    modelEngine,
    engineVersion,
    modelUid,
    modelVersion,
    replica,
    replicaPayload,
    kwargs,
    virtualEnvConfig,
    run,
    form,
    isK8s,
  ]);

  const updateYaml = (key: string, yaml: string) => {
    userEditedRef.current[key] = true;
    persistRef.current[key] = true;
    setItems((prev) => {
      const next = prev.map((it) => (it.key === key ? { ...it, yaml } : it));
      return emitItems(next);
    });
  };

  const regenerate = () => {
    userEditedRef.current = {};
    seedProtectRef.current = {};
    forcePersistNextRef.current = true;
    setFormChangedWhileEdited(false);
    run();
  };

  const card = items[0];

  const handleCopy = () => {
    if (!card?.yaml) return;
    copyToClipboard(card.yaml);
  };

  const emptyShell = (node: ReactNode) => (
    <div
      className="box-border flex w-full min-h-[min(72vh,640px)] items-center justify-center"
      style={{ width: '100%' }}
    >
      <div className="flex w-full max-w-xl flex-col items-center justify-center px-4 py-10 text-center">
        {node}
      </div>
    </div>
  );

  return (
    <div className="mt-2 w-full" style={{ width: '100%' }}>
      <div className="flex items-center justify-between mb-2 gap-2 flex-wrap">
        <div className="font-medium text-base">
          {l('models.deploy.manifest.title')}
          {runtime ? (
            <span className="ml-2 text-xs text-muted font-normal">({runtime})</span>
          ) : null}
        </div>
        <div className="flex gap-2">
          <Button
            size="small"
            icon={<RefreshCw size={14} />}
            onClick={regenerate}
            disabled={!(modelEngine || form.getFieldValue('model_engine'))}
          >
            {l('models.deploy.manifest.regenerate')}
          </Button>
          <Button size="small" icon={<Copy size={14} />} disabled={!card?.yaml} onClick={handleCopy}>
            {l('global.actions.copy')}
          </Button>
        </div>
      </div>
      <Alert
        type="info"
        showIcon
        className="mb-2"
        message={l('models.deploy.manifest.portHint',
        )}
      />
      {!previewFinal ? (
        <Alert
          type="warning"
          showIcon
          className="mb-2"
          message={
            previewNote ||
            l('models.deploy.manifest.needWorker',
            )
          }
        />
      ) : null}
      {formChangedWhileEdited ? (
        <Alert
          type="warning"
          showIcon
          className="mb-2"
          message={l('models.deploy.manifest.formChangedKeepYaml',
          )}
        />
      ) : null}
      <Spin spinning={loading} className="w-full">
        {error
          ? emptyShell(
              <Result
                className="!p-0 w-full"
                status="500"
                title={lGet('models.deploy.manifest.failed')}
                subTitle={readThrownMessage(error)}
              />,
            )
          : card?.yaml ? (
            <Input.TextArea
              value={card.yaml}
              onChange={(e) => updateYaml(card.key, e.target.value)}
              autoSize={{ minRows: 12, maxRows: 22 }}
              className="!font-mono text-xs"
            />
          ) : !loading ? (
            emptyShell(
              <Result
                className="!p-0 w-full"
                status="404"
                title={lGet('models.deploy.manifest.empty')}
                subTitle={lGet('models.deploy.manifest.emptyHint',
                )}
              />,
            )
          ) : (
            <div className="min-h-[min(72vh,640px)] w-full" />
          )}
      </Spin>
    </div>
  );
};

export default ManifestYamlPreview;
