import { FC } from 'react';
import { Button } from 'antd';
import { ProFormTextArea } from '@ant-design/pro-components';
import { ActionWithTips } from '@/components';
import { l } from '@/utils/intl';
import { useModel } from '@umijs/max';
import { parsePromptToObject } from '../utils';

interface PromptProps {
  loading: boolean;
  onGenerate: () => void;
}
const Prompt: FC<PromptProps> = ({ loading, onGenerate }) => {
  const { form } = useModel('management.drawingTool.model');
  const parsePromptToField = () => {
    const prompt = form.getFieldValue('prompt');
    // 提取字符串中的表单信息
    const result = parsePromptToObject(prompt);
    form.setFieldsValue(result);
  };
  const clearPrompt = () => {
    form.setFieldsValue({
      prompt: '',
      negative_prompt: '',
    });
  };
  return (
    <div className="flex gap-x-[20px] items-start mb-[20px]">
      <div className="flex-1">
        <ProFormTextArea
          name="prompt"
          label={l('management.drawingTool.prompt')}
          placeholder={l('management.drawingTool.prompt')}
          rules={[{ required: true }]}
          fieldProps={{ rows: 3 }}
          allowClear
        />
        <ProFormTextArea
          name="negative_prompt"
          label={l('management.drawingTool.negativePrompt')}
          placeholder={l('management.drawingTool.negativePrompt')}
          fieldProps={{ rows: 3 }}
          allowClear
        />
      </div>
      <div className="mt-[30px] w-[300px] flex flex-col gap-y-[10px]">
        <Button
          block
          type="primary"
          size="large"
          className="h-[58px]"
          onClick={onGenerate}
          loading={loading}
        >
          {l('management.drawingTool.stratgGenerate')}
        </Button>
        <div className="flex gap-x-[10px]">
          <ActionWithTips title={l('management.drawingTool.oneClickFill')}>
            <Button
              icon={
                <span className="text-[18px]" onClick={parsePromptToField}>
                  ↙️
                </span>
              }
            />
          </ActionWithTips>
          <ActionWithTips title={l('management.drawingTool.oneClickClear')}>
            <Button
              icon={
                <span className="text-[18px]" onClick={clearPrompt}>
                  🗑️
                </span>
              }
            />
          </ActionWithTips>
        </div>
      </div>
    </div>
  );
};
export default Prompt;
