import { Button, Form } from 'antd';
import type { FormInstance } from 'antd';
import { ProFormTextArea } from '@ant-design/pro-components';
import { CheckCircleFilled, CloseCircleFilled } from '@ant-design/icons';
import { useImperativeHandle, forwardRef } from 'react';
import { useSetState } from 'ahooks';
import nunjucks from 'nunjucks';
import classNames from 'classnames';

import { l, lGet } from '@/utils/intl';
import { MODEL_TEMPLATE_TEST_MSG } from '@/constants/modelData';

interface ChatTemplateProps {
  // testCallBack: (b: boolean) => void;
  form: FormInstance;
}
interface ChatTemplateState {
  testResult: string;
  isSuccess?: boolean;
}
export interface ChatTemplateMethod {
  resetStatus: () => void;
}
const ChatTemplate = forwardRef<ChatTemplateMethod, ChatTemplateProps>(({ form }, ref) => {
  const chatTemplateValue = Form.useWatch('chat_template', form);
  const [{ testResult, isSuccess }, setState] = useSetState<ChatTemplateState>({
    testResult: '',
    isSuccess: undefined,
  });
  const handleTest = () => {
    if (chatTemplateValue) {
      try {
        nunjucks.configure({ autoescape: false });
        const test_res = nunjucks.renderString(chatTemplateValue, {
          messages: MODEL_TEMPLATE_TEST_MSG,
        });
        setState({ testResult: test_res, isSuccess: !!test_res });
        // testCallBack(!!test_res);
      } catch (error) {
        setState({ testResult: `${error}`, isSuccess: false });
        // testCallBack(false);
      }
    }
  };
  const resetStatus = () => {
    form.setFieldValue('chat_template', '');
    setState({
      testResult: '',
      isSuccess: undefined,
    });
  };
  useImperativeHandle(ref, () => ({
    resetStatus,
  }));
  return (
    <div className="w-full flex items-start gap-x-[10px]">
      <div className="flex-1">
        <ProFormTextArea
          label={l('models.register.chatTemplate')}
          rules={[{ required: true }]}
          name="chat_template"
          extra={l('models.register.chatTemplateTips')}
          fieldProps={{ autoSize: { minRows: 5, maxRows: 5 } }}
          tooltip={
            <>
              <div>{lGet('models.register.chatTemplateExample')}</div>
              <pre>{JSON.stringify(MODEL_TEMPLATE_TEST_MSG, null, 2)}</pre>
            </>
          }
        />
      </div>

      <Button className="mt-[60px] shrink-0" type="primary" onClick={handleTest}>
        {l('models.register.chatTemplateTest')}
      </Button>
      <div className="shrink-0 w-[30%]">
        <div className="mt-[22px] border bg-background rounded-md px-[11px] py-[4px] h-[120px] overflow-auto">
          <div className="cursor-pointer flex gap-x-[2px] items-center">
            {l('models.register.chatTemplateResult')}
            {isSuccess === undefined ? (
              ''
            ) : isSuccess ? (
              <CheckCircleFilled className="text-[#52c41a] text-[18px] ml-[2px]" />
            ) : (
              <CloseCircleFilled className="text-[#ff4d4f] text-[18px] ml-[2px]" />
            )}
          </div>
          <div>{testResult}</div>
        </div>
        <div
          className={classNames('mt-2 text-muted text-xs', {
            '!text-[#faad14]': isSuccess === false,
          })}
        >
          {l('models.register.chatTemplateResultTips')}
        </div>
      </div>
    </div>
  );
});
export default ChatTemplate;
