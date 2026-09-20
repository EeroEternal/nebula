import {
  ProCard,
  ProFormField,
  ProFormSwitch,
  ProFormGroup,
  ProFormText,
  ProFormSelect,
} from '@ant-design/pro-components';
import { FC } from 'react';
import { Cpu } from 'lucide-react';
import { Form } from 'antd';
import { l } from '@/utils/intl';
import { COMPUTE_TYPE, ZERO_STAGE_OPTIONS } from '@/constants/finetune';
import { InputNumberWithSlider } from '@/components';
import type { StepFormProps } from './index';

const TrainSet: FC<StepFormProps> = () => {
  const dsEnable = Form.useWatch(['train_args', 'ds_enable']);
  return (
    <ProCard
      title={
        <span className="flex items-center gap-2">
          <Cpu size={18} className="text-primary" />
          {l('tasks.finetune.trainingSet')}
        </span>
      }
      bodyStyle={{ paddingInline: 20 }}
      extra={<span className="text-muted">5/6</span>}
    >
      <ProFormGroup rowProps={{ gutter: [16, 0] }}>
        <ProFormText
          name={['train_args', 'learning_rate']}
          label={l('tasks.finetune.trainingSet.learningRate')}
          rules={[{ required: true }]}
          tooltip={l('tasks.finetune.trainingSet.learningRate.tips')}
          colProps={{ span: 12 }}
          fieldProps={{ size: 'large' }}
        />
        <ProFormText
          name={['train_args', 'num_train_epochs']}
          label={l('tasks.finetune.trainingSet.numTrainEpochs')}
          rules={[{ required: true }]}
          tooltip={l('tasks.finetune.trainingSet.numTrainEpochs.tips')}
          colProps={{ span: 12 }}
          fieldProps={{ size: 'large' }}
        />
        <ProFormText
          name={['train_args', 'max_grad_norm']}
          label={l('tasks.finetune.trainingSet.maxGradNorm')}
          rules={[{ required: true }]}
          tooltip={l('tasks.finetune.trainingSet.maxGradNorm.tips')}
          colProps={{ span: 12 }}
          fieldProps={{ size: 'large' }}
        />
        <ProFormSelect
          name={['train_args', 'compute_type']}
          label={l('tasks.finetune.trainingSet.computeType')}
          rules={[{ required: true }]}
          valueEnum={COMPUTE_TYPE}
          tooltip={l('tasks.finetune.trainingSet.computeType.tips')}
          colProps={{ span: 12 }}
          fieldProps={{ size: 'large' }}
        />
        <ProFormField
          name={['train_args', 'batch_size']}
          rules={[{ required: true }]}
          colProps={{ span: 12 }}
        >
          <InputNumberWithSlider
            label={l('tasks.finetune.trainingSet.batchSize')}
            tooltip={l('tasks.finetune.trainingSet.batchSize.tips')}
            min={1}
            max={1024}
          />
        </ProFormField>
        <ProFormField
          name={['train_args', 'gradient_accumulation_steps']}
          rules={[{ required: true }]}
          colProps={{ span: 12 }}
        >
          <InputNumberWithSlider
            label={l('tasks.finetune.trainingSet.gradientAccumulationSteps')}
            tooltip={l('tasks.finetune.trainingSet.gradientAccumulationSteps.tips')}
            min={1}
            max={1024}
          />
        </ProFormField>

        <ProFormText
          name={['train_args', 'lr_scheduler_type']}
          label={l('tasks.finetune.trainingSet.lrSchedulerType')}
          rules={[{ required: true }]}
          tooltip={l('tasks.finetune.trainingSet.lrSchedulerType.tips')}
          colProps={{ span: 12 }}
          fieldProps={{ size: 'large' }}
        />
        <ProFormSwitch
          name={['train_args', 'ds_enable']}
          label="DeepSpeed"
          tooltip={l('tasks.finetune.trainingSet.deepSpeed.tips')}
          colProps={{ span: 12 }}
          rules={[{ required: true }]}
          checkedChildren={l('global.yes')}
          unCheckedChildren={l('global.no')}
        />

        <ProFormSelect
          name={['train_args', 'ds_stage']}
          label="Zero Stage"
          tooltip={l('tasks.finetune.trainingSet.zeroStage.tips')}
          options={ZERO_STAGE_OPTIONS}
          disabled={!dsEnable}
          rules={[{ required: true }]}
          colProps={{ span: 12 }}
          fieldProps={{ size: 'large' }}
        />
        <ProFormSwitch
          name={['train_args', 'ds_offload']}
          label="Offload"
          tooltip={l('tasks.finetune.trainingSet.offload.tips')}
          disabled={!dsEnable}
          rules={[{ required: true }]}
          colProps={{ span: 12 }}
        />
      </ProFormGroup>
    </ProCard>
  );
};
export default TrainSet;
