import { Form, notification } from 'antd';
import type { FormInstance } from 'antd';
import { StepsForm } from '@ant-design/pro-components';
import { useRequest } from 'ahooks';
import { FC, useMemo, useState, useRef } from 'react';
import { Check, Server, Cpu, Brain, Database, FolderOutput, Layers } from 'lucide-react';

import { l } from '@/utils/intl';
import request from '@/utils/request';
import {
  TRAINING_STAGE,
  QUANTIZATION_BIT,
  ROPE_SCALING,
  BOOSTER,
  FINETUNING_TYPE,
  BADAM_MODE,
  BADAM_SWITCH_MODE,
  COMPUTE_TYPE,
  OPTIM,
} from '@/constants/finetune';
import { createFinetuneTask, updateFinetuneTask } from '@/pages/management/finetune/service';
import BaseSet from './BaseSet';
import ModelSet from './ModelSet';
import DataSet from './DataSet';
import SolutionSet from './SolutionSet';
import TrainSet from './TrainSet';
import OutputSet from './OutputSet';

const defaultValues = {
  model_args: {
    is_builtin: true,
    quantization_bit: QUANTIZATION_BIT.none,
    rope_scaling: ROPE_SCALING.none,
    booster: BOOSTER.none,
    visual_inputs: false,
    resize_vocab: false,
    upcast_layernorm: false,
    shift_attn: false,
  },
  data_args: {
    // dataset_dir: 不传,后端读 LLAMAFACTORY_DATA_DIR env 或 dirname x 3 兜底
    cutoff_len: 1024,
    max_samples: 100000,
    val_size: 0,
    packing: false,
  },
  tuning_args: {
    training_stage: TRAINING_STAGE.sft,
    finetuning_type: FINETUNING_TYPE.lora,
    use_llama_pro: false,
  },
  lora_args: {
    use_lora: false,
    lora_rank: 8,
    lora_alpha: 16,
    lora_dropout: 0.0,
    lora_target: 'all',
    loraplus_lr_ratio: 0,
    use_dora: false,
    use_rslora: false,
    create_new_adapter: false,
    additional_target: '',
  },
  galore_args: {
    use_galore: false,
    galore_rank: 16,
    galore_update_interval: 200,
    galore_target: 'all',
    galore_scale: 2,
  },
  badam_args: {
    use_badam: false,
    badam_mode: BADAM_MODE.layer,
    badam_switch_mode: BADAM_SWITCH_MODE.ascending,
    badam_switch_interval: 50,
    badam_update_ratio: 0.05,
  },
  train_args: {
    gpu_ids: [],
    learning_rate: 5e-5,
    num_train_epochs: 100,
    per_device_train_batch_size: 2,
    max_grad_norm: 1.0,
    gradient_accumulation_steps: 8,
    lr_scheduler_type: 'cosine',
    batch_size: 2,
    compute_type: COMPUTE_TYPE.fp16,
    ds_enable: false,
    ds_stage: 2,
    ds_offload: false,
  },
  output_args: {
    logging_steps: 5,
    save_steps: 100,
    warmup_steps: 0,
    neftune_alpha: 0,
    optim: OPTIM['adamw_torch'],
    report_to: false,
    output_dir: '/',
  },
};
interface TaskFormProps {
  taskId?: string;
  onSubmitCallback: () => void;
}

export interface StepFormProps {
  isEdit: boolean;
  formMap: React.MutableRefObject<FormInstance | undefined>[];
}
const TaskForm: FC<TaskFormProps> = ({ taskId, onSubmitCallback }) => {
  const isEdit = !!taskId;
  const formMapRef = useRef<React.MutableRefObject<FormInstance | undefined>[]>([]);
  const tuning_args = Form.useWatch(['tuning_args'], formMapRef?.current?.[3]?.current);
  const isLora = tuning_args?.finetuning_type === FINETUNING_TYPE.lora;

  const [currentStep, setCurrentStep] = useState(0);
  const steps = useMemo(
    () => [
      { id: 0, label: l('tasks.finetune.baseSet'), icon: Server },
      { id: 1, label: l('tasks.finetune.modelSet'), icon: Brain },
      { id: 2, label: l('tasks.finetune.dataSet'), icon: Database },
      { id: 3, label: l('tasks.finetune.solutionSet'), icon: Layers },
      { id: 4, label: l('tasks.finetune.trainingSet'), icon: Cpu },
      { id: 5, label: l('tasks.finetune.outputSet'), icon: FolderOutput },
    ],
    [],
  );

  useRequest(() => request(`/tasks/${taskId}`), {
    ready: !!taskId,
    onSuccess: (res) => {
      if (res.success) {
        const {
          task_name,
          worker_ip,
          model_config = {},
          data_config = {},
          tuning_config = {},
          lora_config = {},
          galore_config = {},
          badam_config = {},
          train_config = {},
          output_config = {},
        } = res?.data || {};
        formMapRef?.current?.forEach((formInstanceRef) => {
          formInstanceRef?.current?.setFieldsValue({
            task_name,
            worker_ip,
            model_args: model_config,
            data_args: data_config,
            tuning_args: tuning_config,
            lora_args: lora_config,
            galore_args: galore_config,
            badam_args: badam_config,
            train_args: train_config,
            output_args: output_config,
          });
        });
      }
    },
  });

  const { run: submit, loading: submitLoading } = useRequest(
    (data) => (data?.task_id ? updateFinetuneTask(data) : createFinetuneTask(data)),
    {
      manual: true,
      onSuccess: (res) => {
        if (res.success) {
          onSubmitCallback();
        }
      },
      onError: (e) => {
        notification.error({ message: e.message });
      },
    },
  );
  const handleSubmit = async (values: Record<string, unknown>) => {
    let newValues: Record<string, unknown> = { ...values, task_id: taskId };
    // 微调方法是lora 时，隐藏Galore配置，BAdam配置
    if (isLora) {
      newValues = {
        ...newValues,
        galore_args: {
          use_galore: false,
        },
        badam_args: {
          use_badam: false,
        },
      };
    }
    submit(newValues);
  };
  return (
    <StepsForm
      formMapRef={formMapRef}
      containerStyle={{ width: '100%' }}
      current={currentStep}
      onCurrentChange={(step) => setCurrentStep(step)}
      stepsRender={() => (
        <div className="@container">
          <div className="flex flex-col @[650px]:flex-row @[650px]:items-center w-full">
            {steps.map((step, i) => {
              const Icon = step.icon;
              const isActive = i === currentStep;
              const isDone = i < currentStep;
              return (
                <>
                  {/* step */}
                  <div
                    className="flex items-center gap-3 @[650px]:gap-2 shrink-0"
                    // onClick={() => setCurrentStep(i)}
                  >
                    <div
                      className={`flex items-center justify-center rounded-full shrink-0 h-9 w-9 @[650px]:h-8 @[650px]:w-8 text-sm font-semibold
                      ${
                        isActive
                          ? 'bg-primary text-white'
                          : isDone
                          ? 'bg-primary/10 text-primary border-2 border-primary/30'
                          : 'bg-background-muted text-muted border-2 border'
                      }`}
                    >
                      {isDone ? <Check className="h-4 w-4" /> : <Icon className="h-4 w-4" />}
                    </div>
                    <span
                      className={`text-sm font-medium whitespace-nowrap
                          ${isActive ? 'text-default' : isDone ? 'text-primary' : 'text-muted'}`}
                    >
                      {step.label}
                    </span>
                  </div>
                  {/* 横线 */}
                  {i < steps.length - 1 && (
                    <>
                      <div
                        className={`w-px h-5 ml-[17px] my-0.5 @[650px]:hidden 
                          ${i < currentStep ? 'bg-primary' : 'bg-border'}`}
                      />
                      <div
                        className={`hidden @[650px]:block flex-1 h-px mx-2 
                          ${i < currentStep ? 'bg-primary' : 'bg-border'}`}
                      />
                    </>
                  )}
                </>
              );
            })}
          </div>
        </div>
      )}
      onFinish={handleSubmit}
      submitter={{
        submitButtonProps: { loading: currentStep === 5 && submitLoading },
      }}
    >
      <StepsForm.StepForm name="base" grid initialValues={defaultValues}>
        <BaseSet isEdit={isEdit} formMap={formMapRef.current} />
      </StepsForm.StepForm>
      <StepsForm.StepForm name="model" grid initialValues={defaultValues}>
        <ModelSet isEdit={isEdit} formMap={formMapRef.current} />
      </StepsForm.StepForm>
      <StepsForm.StepForm name="data" grid initialValues={defaultValues}>
        <DataSet isEdit={isEdit} formMap={formMapRef.current} />
      </StepsForm.StepForm>
      <StepsForm.StepForm name="solution" grid initialValues={defaultValues}>
        <SolutionSet isEdit={isEdit} formMap={formMapRef.current} />
      </StepsForm.StepForm>
      <StepsForm.StepForm name="train" grid initialValues={defaultValues}>
        <TrainSet isEdit={isEdit} formMap={formMapRef.current} />
      </StepsForm.StepForm>
      <StepsForm.StepForm name="output" grid initialValues={defaultValues}>
        <OutputSet isEdit={isEdit} formMap={formMapRef.current} />
      </StepsForm.StepForm>
    </StepsForm>
  );
};
export default TaskForm;
