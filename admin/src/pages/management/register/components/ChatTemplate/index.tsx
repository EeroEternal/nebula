import { Button, Form, Typography } from 'antd';
import type { FormInstance } from 'antd';
import { ProFormTextArea, ModalForm } from '@ant-design/pro-components';
import { ExpandAltOutlined, CheckCircleFilled, CloseCircleFilled } from '@ant-design/icons';
import { useImperativeHandle, forwardRef } from 'react';
import { useSetState } from 'ahooks';
import nunjucks from 'nunjucks';
import { l } from '@/utils/intl';
import { MODEL_TEMPLATE_TEST_MSG } from '@/constants/modelData';
import classNames from 'classnames';

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
    <div className="w-full flex gap-x-[10px]">
      <div className="flex-1">
        <ProFormTextArea
          label={l('model.register.chatTemplate')}
          rules={[{ required: true }]}
          name="chat_template"
          extra={l('model.register.chatTemplateTips')}
          fieldProps={{ autoSize: { minRows: 5, maxRows: 5 } }}
        />
      </div>

      <Button className="mt-[70px] shrink-0" type="primary" onClick={handleTest}>
        {l('model.register.chatTemplateTest')}
      </Button>
      <div className="shrink-0 w-[30%]">
        <div className=" border border-[#E5E7EB] rounded-[6px] px-[11px] py-[4px] mt-[30px] h-[120px] overflow-auto">
          <ModalForm
            title={l('model.register.chatTemplateExample')}
            trigger={
              <div className="cursor-pointer flex gap-x-[2px] items-center group">
                <span className="group-hover:text-primary">
                  {l('model.register.chatTemplateExample')}
                </span>
                <div className="w-[18px] h-[18px] text-[#666] bg-card rounded-full flex items-center justify-center group-hover:bg-primary group-hover:text-[#fff]">
                  <ExpandAltOutlined />
                </div>
              </div>
            }
            submitter={false}
          >
            <Typography>
              <pre>{JSON.stringify(MODEL_TEMPLATE_TEST_MSG, null, 2)}</pre>
            </Typography>
          </ModalForm>

          <div className="cursor-pointer flex gap-x-[2px] items-center">
            {l('model.register.chatTemplateResult')}
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
          className={classNames('text-[#787878]', {
            '!text-[#faad14]': isSuccess === false,
          })}
        >
          {l('model.register.chatTemplateResultTips')}
        </div>
      </div>
    </div>
  );
});
export default ChatTemplate;
