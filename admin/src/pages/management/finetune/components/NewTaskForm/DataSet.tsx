import {
  ProCard,
  ProFormGroup,
  ProFormSelect,
  ProFormText,
  ProFormField,
  ProFormSwitch,
} from '@ant-design/pro-components';
import { Form } from 'antd';
import { useRequest } from 'ahooks';
import { FC } from 'react';
import { Database } from 'lucide-react';

import { TRAINING_STAGE } from '@/constants/finetune';
import { InputNumberWithSlider } from '@/components';
import { l } from '@/utils/intl';
import request from '@/utils/request';
import type { StepFormProps } from './index';

const DataSet: FC<StepFormProps> = ({}) => {
  const form = Form.useFormInstance();
  const data_args = Form.useWatch(['data_args']);
  const trainingStage = Form.useWatch(['tuning_args', 'training_stage']);
  const { data: datasetRes, loading: dataSetLoading } = useRequest(
    () =>
      request<{ data: { data: string[] } }>('/tasks/tuning/dataset', {
        params: {
          training_stage: trainingStage,
          dataset_dir: data_args?.dataset_dir,
        },
      }),
    {
      ready: !!(data_args?.dataset_dir && trainingStage),
      refreshDeps: [trainingStage],
    },
  );
  const datasetOptions = (datasetRes?.data?.data || []).map((item) => item);
  const handleTrainingStage = () => {
    form.setFieldsValue({
      data_args: { dataset: [] },
    });
  };
  return (
    <ProCard
      title={
        <span className="flex items-center gap-2">
          <Database size={18} className="text-primary" />
          {l('tasks.finetune.dataSet')}
        </span>
      }
      bodyStyle={{ paddingInline: 20 }}
      extra={<span className="text-muted">3/6</span>}
    >
      <ProFormGroup rowProps={{ gutter: [16, 0] }}>
        <ProFormSelect
          name={['tuning_args', 'training_stage']}
          label={l('tasks.finetune.dataSet.trainingStage')}
          placeholder={l('tasks.finetune.dataSet.trainingStage.rule')}
          rules={[{ required: true, message: l('tasks.finetune.dataSet.trainingStage.rule') }]}
          valueEnum={TRAINING_STAGE}
          tooltip={l('tasks.finetune.dataSet.trainingStage.tips')}
          colProps={{ span: 12 }}
          fieldProps={{
            size: 'large',
            onChange: handleTrainingStage,
          }}
        />
        <ProFormSelect
          name={['data_args', 'dataset']}
          label={l('tasks.finetune.dataSet')}
          rules={[{ required: true, message: l('tasks.finetune.dataSet.rule') }]}
          placeholder={l('tasks.finetune.dataSet.rule')}
          mode="multiple"
          options={datasetOptions}
          fieldProps={{
            size: 'large',
            loading: dataSetLoading,
            maxTagCount: 'responsive',
          }}
          colProps={{ span: 12 }}
        />
        <ProFormText
          name={['data_args', 'dataset_dir']}
          label={l('tasks.finetune.dataSet.dir')}
          disabled
          tooltip={l('tasks.finetune.dataSet.dir.tips')}
          colProps={{ span: 12 }}
          fieldProps={{ size: 'large' }}
          required
        />
        <ProFormText
          name={['data_args', 'max_samples']}
          label={l('tasks.finetune.dataSet.maxSamples')}
          rules={[{ required: true, message: l('tasks.finetune.dataSet.maxSamples.rule') }]}
          placeholder={l('tasks.finetune.dataSet.maxSamples.rule')}
          colProps={{ span: 12 }}
          fieldProps={{ size: 'large' }}
        />
        <ProFormField
          name={['data_args', 'cutoff_len']}
          rules={[{ required: true, message: l('tasks.finetune.dataSet.cutoffLen.rule') }]}
          colProps={{ span: 12 }}
        >
          <InputNumberWithSlider
            label={l('tasks.finetune.dataSet.cutoffLen')}
            min={4}
            max={64 * 1024}
          />
        </ProFormField>
        <ProFormField
          name={['data_args', 'val_size']}
          rules={[{ required: true, message: l('tasks.finetune.dataSet.valSize.rule') }]}
          colProps={{ span: 12 }}
        >
          <InputNumberWithSlider
            label={l('tasks.finetune.dataSet.valSize')}
            min={0}
            max={1}
            precision={2}
            step={0.01}
          />
        </ProFormField>

        <ProFormSwitch
          name={['data_args', 'packing']}
          label={l('tasks.finetune.dataSet.packing')}
          required
          fieldProps={{ className: 'mt-2' }}
          colProps={{ span: 12 }}
          checkedChildren={l('global.yes')}
          unCheckedChildren={l('global.no')}
        />
      </ProFormGroup>
    </ProCard>
  );
};
export default DataSet;
