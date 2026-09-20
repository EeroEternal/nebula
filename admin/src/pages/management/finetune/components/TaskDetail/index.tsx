import { Drawer, Tag, Collapse } from 'antd';
import { FC, PropsWithChildren, useMemo, useState, useEffect, useRef } from 'react';
import { Server, Brain, Database, Layers, Cpu, FolderOutput, FileText } from 'lucide-react';
import cn from 'classnames';
import { Prism as SyntaxHighlighter } from 'react-syntax-highlighter';
import { atomDark } from 'react-syntax-highlighter/dist/esm/styles/prism';

import { getLocal } from '@/utils';
import { lGet } from '@/utils/intl';
import { formatTime } from '@/utils/fomatData';
import { parseSecondStr } from '@/utils/function';
import { TASK_STATUS_COLORS } from '@/constants/finetune';
import type { FinetuneListItem } from '@/types/Public/data';

type RowItem =
  | { type?: never; label: React.ReactNode; value: React.ReactNode }
  | { type: 'divider' }
  | { type?: never; title: React.ReactNode };
interface InfoRowProps {
  rows: RowItem[];
  className?: string;
}
const InfoRows: FC<InfoRowProps> = ({ rows, className }) => {
  return (
    <div className={cn('flex flex-col gap-1.5 text-xs', className)}>
      {rows.map((item, index) => {
        if (item.type === 'divider') {
          return <div key={`divider${index}`} className="border-b my-0.5" />;
        }
        if ('title' in item) {
          return (
            <div key={`title${index}`} className="font-medium text-muted">
              {item.title}
            </div>
          );
        }
        return (
          <div key={`item${index}`} className="flex items-center justify-between text-muted">
            {item.label}
            <span className="text-default font-medium">{item.value}</span>
          </div>
        );
      })}
    </div>
  );
};
interface TaskDetailProps {
  taskDetail: FinetuneListItem;
  onClose: () => void;
}
const TaskDetail: FC<PropsWithChildren<TaskDetailProps>> = ({ taskDetail, onClose }) => {
  const logScrollRef = useRef<HTMLDivElement>(null);
  const controllerRef = useRef<AbortController | null>(null);
  const [logs, setLogs] = useState<string[]>([]);
  const {
    task_id,
    task_name,
    task_status,
    create_ts,
    finish_ts,
    worker_ip,
    train_config,
    model_config,
    data_config,
    tuning_config,
    lora_config,
    galore_config,
    badam_config,
    output_config,
  } = (taskDetail || {}) as FinetuneListItem;
  const baseInfo = useMemo(
    () => [
      {
        label: lGet('tasks.finetune.taskId'),
        value: task_id,
      },
      {
        label: lGet('tasks.finetune.createTime'),
        value: create_ts !== null ? formatTime(create_ts) : '-',
      },
      {
        label: lGet('tasks.finetune.endTime'),
        value: finish_ts !== null ? formatTime(finish_ts) : '-',
      },
      {
        label: lGet('tasks.finetune.executionDuration'),
        value: finish_ts && create_ts ? parseSecondStr(finish_ts - create_ts) : '-',
      },
    ],
    [taskDetail],
  );
  const baseSet = useMemo(
    () => [
      {
        label: 'Worker IP',
        value: worker_ip,
      },
      {
        label: 'GPU Index',
        value: (train_config?.gpu_ids || []).join('、'),
      },
    ],
    [taskDetail],
  );
  const modelSet = useMemo(
    () => [
      { label: lGet('tasks.finetune.modelType'), value: model_config?.model_type },
      { label: lGet('tasks.finetune.modelName'), value: model_config?.model_name },
      { label: lGet('tasks.finetune.modelVersion'), value: model_config?.model_version },
      { label: lGet('tasks.finetune.modelSet.template'), value: data_config?.template },
      { label: lGet('tasks.finetune.modelSet.modelPath'), value: model_config?.model_path },
      { type: 'divider' as const },
      { title: lGet('tasks.finetune.modelSet.advancedOptions') },
      {
        label: lGet('tasks.finetune.modelSet.advancedOptions.quantizationBit'),
        value: model_config?.quantization_bit,
      },
      {
        label: lGet('tasks.finetune.modelSet.advancedOptions.ropeScaling'),
        value: model_config?.rope_scaling,
      },
      {
        label: lGet('tasks.finetune.modelSet.advancedOptions.booster'),
        value: model_config?.booster,
      },
      {
        label: lGet('tasks.finetune.modelSet.advancedOptions.visualInputs'),
        value: model_config?.visual_inputs ? lGet('global.yes') : lGet('global.no'),
      },
      {
        label: lGet('tasks.finetune.modelSet.advancedOptions.resizeVocabe'),
        value: model_config?.resize_vocab ? lGet('global.yes') : lGet('global.no'),
      },
      {
        label: lGet('tasks.finetune.modelSet.advancedOptions.upcastLayernorm'),
        value: model_config?.upcast_layernorm ? lGet('global.yes') : lGet('global.no'),
      },
      {
        label: 'S² Attention',
        value: model_config?.shift_attn ? lGet('global.yes') : lGet('global.no'),
      },
    ],
    [taskDetail],
  );
  const dataSet = useMemo(
    () => [
      { label: lGet('tasks.finetune.dataSet.trainingStage'), value: tuning_config?.training_stage },
      { label: lGet('tasks.finetune.dataSet'), value: (data_config?.dataset || []).join('、') },
      { label: lGet('tasks.finetune.dataSet.dir'), value: data_config?.dataset_dir },
      { label: lGet('tasks.finetune.dataSet.maxSamples'), value: data_config?.max_samples },
      { label: lGet('tasks.finetune.dataSet.cutoffLen'), value: data_config?.cutoff_len },
      { label: lGet('tasks.finetune.dataSet.valSize'), value: data_config?.val_size },
      {
        label: lGet('tasks.finetune.dataSet.packing'),
        value: data_config?.packing ? lGet('global.yes') : lGet('global.no'),
      },
    ],
    [taskDetail],
  );
  const solutionSet = useMemo(() => {
    return [
      {
        label: lGet('tasks.finetune.solutionSet.finetuneMethod'),
        value: tuning_config?.finetuning_type,
      },
      {
        label: lGet('tasks.finetune.solutionSet.useLlamaPro'),
        value: tuning_config?.use_llama_pro ? lGet('global.yes') : lGet('global.no'),
      },
      { type: 'divider' as const },
      ...(tuning_config.finetuning_type === 'lora'
        ? [
            { title: lGet('tasks.finetune.solutionSet.loraConfig') },
            {
              label: lGet('tasks.finetune.solutionSet.loraConfig.loraRank'),
              value: lora_config?.lora_rank,
            },
            {
              label: lGet('tasks.finetune.solutionSet.loraConfig.loraAlpha'),
              value: lora_config?.lora_alpha,
            },
            {
              label: lGet('tasks.finetune.solutionSet.loraConfig.loraDropout'),
              value: lora_config?.lora_dropout,
            },
            {
              label: lGet('tasks.finetune.solutionSet.loraConfig.loraplusLrRatio'),
              value: lora_config?.loraplus_lr_ratio,
            },
            {
              label: lGet('tasks.finetune.solutionSet.loraConfig.loraTarget'),
              value: lora_config?.lora_target,
            },
            {
              label: lGet('tasks.finetune.solutionSet.loraConfig.additionalTarget'),
              value: lora_config?.additional_target || '-',
            },
            {
              label: lGet('tasks.finetune.solutionSet.loraConfig.createNewAdapter'),
              value: lora_config?.create_new_adapter ? lGet('global.yes') : lGet('global.no'),
            },
            {
              label: lGet('tasks.finetune.solutionSet.loraConfig.useRslora'),
              value: lora_config?.use_rslora ? lGet('global.yes') : lGet('global.no'),
            },
            {
              label: lGet('tasks.finetune.solutionSet.loraConfig.useDora'),
              value: lora_config?.use_dora ? lGet('global.yes') : lGet('global.no'),
            },
          ]
        : [
            {
              label: lGet('tasks.finetune.solutionSet.galoreConfig'),
              value: galore_config?.use_galore ? lGet('global.yes') : lGet('global.no'),
            },
            ...(galore_config?.use_galore
              ? [
                  {
                    label: lGet('tasks.finetune.solutionSet.galoreConfig.galoreRank'),
                    value: galore_config?.galore_rank,
                  },
                  {
                    label: lGet('tasks.finetune.solutionSet.galoreConfig.galoreUpdateInterval'),
                    value: galore_config?.galore_update_interval,
                  },
                  {
                    label: lGet('tasks.finetune.solutionSet.galoreConfig.galoreScale'),
                    value: galore_config?.galore_scale,
                  },
                  {
                    label: lGet('tasks.finetune.solutionSet.galoreConfig.galoreTarget'),
                    value: galore_config?.galore_target,
                  },
                ]
              : []),
            {
              label: lGet('tasks.finetune.solutionSet.badamConfig'),
              value: badam_config?.use_badam ? lGet('global.yes') : lGet('global.no'),
            },
            ...(badam_config?.use_badam
              ? [
                  {
                    label: lGet('tasks.finetune.solutionSet.badamConfig.badamMode'),
                    value: badam_config?.badam_mode,
                  },
                  {
                    label: lGet('tasks.finetune.solutionSet.badamConfig.badamSwitchMode'),
                    value: badam_config?.badam_switch_mode,
                  },
                  {
                    label: lGet('tasks.finetune.solutionSet.badamConfig.badamSwitchInterval'),
                    value: badam_config?.badam_switch_interval,
                  },
                  {
                    label: lGet('tasks.finetune.solutionSet.badamConfig.badamUpdateRatio'),
                    value: badam_config?.badam_update_ratio,
                  },
                ]
              : []),
          ]),
    ];
  }, [taskDetail]);
  const trainingSet = useMemo(
    () => [
      {
        label: lGet('tasks.finetune.trainingSet.learningRate'),
        value: train_config?.learning_rate,
      },
      {
        label: lGet('tasks.finetune.trainingSet.numTrainEpochs'),
        value: train_config?.num_train_epochs,
      },
      { label: lGet('tasks.finetune.trainingSet.maxGradNorm'), value: train_config?.max_grad_norm },
      { label: lGet('tasks.finetune.trainingSet.computeType'), value: train_config?.compute_type },
      { label: lGet('tasks.finetune.trainingSet.batchSize'), value: train_config?.batch_size },
      {
        label: lGet('tasks.finetune.trainingSet.gradientAccumulationSteps'),
        value: train_config?.gradient_accumulation_steps,
      },
      {
        label: lGet('tasks.finetune.trainingSet.lrSchedulerType'),
        value: train_config?.lr_scheduler_type,
      },
      {
        label: 'DeepSpeed',
        value: train_config?.ds_enable ? lGet('global.yes') : lGet('global.no'),
      },
      { label: 'Zero Stage', value: train_config?.ds_stage },
      {
        label: 'Offload',
        value: train_config?.ds_offload ? lGet('global.yes') : lGet('global.no'),
      },
    ],
    [],
  );
  const outputSet = useMemo(
    () => [
      { label: lGet('tasks.finetune.outputSet.loggingSteps'), value: output_config?.logging_steps },
      { label: lGet('tasks.finetune.outputSet.saveSteps'), value: output_config?.save_steps },
      { label: lGet('tasks.finetune.outputSet.warmupSteps'), value: output_config?.warmup_steps },
      { label: lGet('tasks.finetune.outputSet.neftuneAlpha'), value: output_config?.neftune_alpha },
      { label: lGet('tasks.finetune.outputSet.optim'), value: output_config?.optim },
      { label: lGet('tasks.finetune.outputSet.outputDir'), value: output_config?.output_dir },
      {
        label: lGet('tasks.finetune.outputSet.reportTo'),
        value: output_config?.report_to ? lGet('global.yes') : lGet('global.no'),
      },
    ],
    [],
  );

  const collapseItems = useMemo(
    () => [
      {
        key: 'baseSet',
        classNames: { header: '!px-0 !py-2', body: '!py-1' },
        label: (
          <div className="flex items-center gap-1 font-medium">
            <Server className="text-primary" size={14} />
            {lGet('tasks.finetune.baseSet')}
          </div>
        ),
        children: <InfoRows rows={baseSet} />,
      },
      {
        key: 'modelSet',
        classNames: { header: '!px-0 !py-2', body: '!py-1' },
        label: (
          <div className="flex items-center gap-1 font-medium">
            <Brain className="text-primary" size={14} />
            {lGet('tasks.finetune.modelSet')}
          </div>
        ),
        children: <InfoRows rows={modelSet} />,
      },
      {
        key: 'dataSet',
        classNames: { header: '!px-0 !py-2', body: '!py-1' },
        label: (
          <div className="flex items-center gap-1 font-medium">
            <Database className="text-primary" size={14} />
            {lGet('tasks.finetune.dataSet')}
          </div>
        ),
        children: <InfoRows rows={dataSet} />,
      },
      {
        key: 'solutionSet',
        classNames: { header: '!px-0 !py-2', body: '!py-1' },
        label: (
          <div className="flex items-center gap-1 font-medium">
            <Layers className="text-primary" size={14} />
            {lGet('tasks.finetune.solutionSet')}
          </div>
        ),
        children: <InfoRows rows={solutionSet} />,
      },
      {
        key: 'trainingSet',
        classNames: { header: '!px-0 !py-2', body: '!py-1' },
        label: (
          <div className="flex items-center gap-1 font-medium">
            <Cpu className="text-primary" size={14} />
            {lGet('tasks.finetune.trainingSet')}
          </div>
        ),
        children: <InfoRows rows={trainingSet} />,
      },
      {
        key: 'outputSet',
        classNames: { header: '!px-0 !py-2', body: '!py-1' },
        label: (
          <div className="flex items-center gap-1 font-medium">
            <FolderOutput className="text-primary" size={14} />
            {lGet('tasks.finetune.outputSet')}
          </div>
        ),
        children: <InfoRows rows={outputSet} />,
      },
      {
        key: 'logs',
        classNames: { header: '!px-0 !py-2', body: '!py-1' },
        label: (
          <div className="flex items-center gap-1 font-medium">
            <FileText className="text-primary" size={14} />
            {lGet('tasks.finetune.logs')}
          </div>
        ),
        children: logs.length ? (
          <div
            ref={logScrollRef}
            style={{
              height: 400,
              overflow: 'auto', // 允许滚动
              background: '#000',
              borderRadius: 6,
              scrollBehavior: 'smooth',
            }}
          >
            <SyntaxHighlighter
              language="log"
              style={atomDark}
              showLineNumbers
              wrapLongLines={false}
              customStyle={{
                margin: 0,
                padding: 8,
                background: '#1e1e1e',
                transition: 'height 0.2s',
                whiteSpace: 'pre-wrap',
              }}
              codeTagProps={{
                style: {
                  fontFamily: 'monospace',
                  whiteSpace: 'pre-wrap',
                  wordBreak: 'break-all',
                },
              }}
            >
              {logs.join('\n')}
            </SyntaxHighlighter>
          </div>
        ) : (
          <div className="h-[100px] text-center leading-[100px] text-xs text-muted">
            {lGet('global.data.empty')}
          </div>
        ),
      },
    ],
    [baseSet, modelSet, dataSet, solutionSet, trainingSet, outputSet, logs],
  );

  const fetchLog = async () => {
    const { token, token_type } = getLocal('user') || {};
    const controller = new AbortController();
    controllerRef.current = controller;
    const response = await fetch(`${window.DOMAIN_API}/tasks/${task_id}/log`, {
      method: 'GET',
      headers: {
        'Content-Type': 'application/json',
        Authorization: `${token_type} ${token}`,
      },
      signal: controller.signal,
    });
    const reader = response.body?.getReader();
    const decoder = new TextDecoder('utf-8');

    if (!reader) return;

    let buffer = '';

    while (true) {
      const { done, value } = await reader.read();
      if (done) break;

      buffer += decoder.decode(value, { stream: true });

      // 按行切分
      const lines = buffer.split('\n');
      buffer = lines.pop() || ''; // 剩余半行

      lines.forEach((line) => {
        if (line.startsWith('data:')) {
          const text = line.replace(/^data:\s*/, '');
          setLogs((prev) => [...prev, text]);
        }
      });
    }
  };

  useEffect(() => {
    if (['running', 'retrying', 'cancelled', 'finished', 'failed'].includes(task_status)) {
      fetchLog();
    }
    return () => {
      controllerRef.current?.abort();
    };
  }, [task_id]);

  useEffect(() => {
    if (logScrollRef.current) {
      requestAnimationFrame(() => {
        logScrollRef.current!.scrollTop = logScrollRef.current!.scrollHeight;
      });
    }
  }, [logs]);
  return (
    <Drawer
      title={task_name}
      width={'50%'}
      maskClosable
      open
      onClose={onClose}
      extra={
        <Tag
          className="mr-0"
          bordered={false}
          color={TASK_STATUS_COLORS[task_status as keyof typeof TASK_STATUS_COLORS] || 'default'}
        >
          {lGet(`tasks.finetune.status.${task_status}`)}
        </Tag>
      }
    >
      <InfoRows rows={baseInfo} className="!text-sm" />
      <div className="border-b mt-4 mb-2" />
      <Collapse defaultActiveKey={['baseSet', 'logs']} ghost items={collapseItems} />
    </Drawer>
  );
};
export default TaskDetail;
