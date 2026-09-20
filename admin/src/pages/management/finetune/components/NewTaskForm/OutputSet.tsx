import {
  ProCard,
  ProFormField,
  ProFormSwitch,
  ProFormGroup,
  ProFormText,
  ProFormSelect,
} from '@ant-design/pro-components';
import { FC } from 'react';
import { FolderOutput } from 'lucide-react';
import { l } from '@/utils/intl';
import { OPTIM } from '@/constants/finetune';
import { InputNumberWithSlider } from '@/components';
import type { StepFormProps } from './index';

const OutputSet: FC<StepFormProps> = () => {
  return (
    <ProCard
      title={
        <span className="flex items-center gap-2">
          <FolderOutput size={18} className="text-primary" />
          {l('tasks.finetune.outputSet')}
        </span>
      }
      bodyStyle={{ paddingInline: 20 }}
      extra={<span className="text-muted">6/6</span>}
    >
      <ProFormGroup rowProps={{ gutter: [16, 0] }}>
        <ProFormField
          name={['output_args', 'logging_steps']}
          rules={[{ required: true }]}
          colProps={{ span: 12 }}
        >
          <InputNumberWithSlider
            label={l('tasks.finetune.outputSet.loggingSteps')}
            tooltip={l('tasks.finetune.outputSet.loggingSteps.tips')}
            min={1}
            max={1000}
            step={5}
          />
        </ProFormField>
        <ProFormField
          name={['output_args', 'save_steps']}
          rules={[{ required: true }]}
          colProps={{ span: 12 }}
        >
          <InputNumberWithSlider
            label={l('tasks.finetune.outputSet.saveSteps')}
            tooltip={l('tasks.finetune.outputSet.saveSteps.tips')}
            min={10}
            max={5000}
            step={10}
          />
        </ProFormField>
        <ProFormField
          name={['output_args', 'warmup_steps']}
          rules={[{ required: true }]}
          colProps={{ span: 12 }}
        >
          <InputNumberWithSlider
            label={l('tasks.finetune.outputSet.warmupSteps')}
            tooltip={l('tasks.finetune.outputSet.warmupSteps.tips')}
            min={0}
            max={5000}
          />
        </ProFormField>
        <ProFormField
          name={['output_args', 'neftune_alpha']}
          rules={[{ required: true }]}
          colProps={{ span: 12 }}
        >
          <InputNumberWithSlider
            label={l('tasks.finetune.outputSet.neftuneAlpha')}
            tooltip={l('tasks.finetune.outputSet.neftuneAlpha.tips')}
            min={0}
            max={10}
            step={0.1}
          />
        </ProFormField>
        <ProFormSelect
          name={['output_args', 'optim']}
          label={l('tasks.finetune.outputSet.optim')}
          rules={[{ required: true }]}
          valueEnum={OPTIM}
          colProps={{ span: 12 }}
          fieldProps={{ size: 'large' }}
        />
        <ProFormText
          name={['output_args', 'output_dir']}
          label={l('tasks.finetune.outputSet.outputDir')}
          rules={[{ required: true, type: 'string' }]}
          tooltip={l('tasks.finetune.outputSet.outputDir.tips')}
          colProps={{ span: 12 }}
          fieldProps={{ size: 'large' }}
        />
        <ProFormSwitch
          name={['output_args', 'report_to']}
          label={l('tasks.finetune.outputSet.reportTo')}
          required
          tooltip={l('tasks.finetune.outputSet.reportTo.tips')}
          checkedChildren={l('global.yes')}
          unCheckedChildren={l('global.no')}
        />
      </ProFormGroup>
    </ProCard>
  );
};
export default OutputSet;
