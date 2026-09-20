import { ModalForm, ProFormTextArea } from '@ant-design/pro-components';
import type { FormInstance, ModalFormProps } from '@ant-design/pro-components';
import { CopyOutlined } from '@ant-design/icons';
import { Button } from 'antd';
import { FC } from 'react';
import { isJSON } from '@/utils';
import { lGet } from '@/utils/intl';

const documentsExample = JSON.stringify(
  ['制定学习计划可以提高效率', '学习时保持专注很重要', '多做运动有助于提高记忆力'],
  null,
  2,
);
const CopyDocuments: FC<{ form: FormInstance }> = ({ form }) => {
  const onFinish: ModalFormProps['onFinish'] = (values) => {
    const inputText = values.copyText.trim().replace(/'([^']*)'/g, '"$1"');
    try {
      const value = JSON.parse(inputText);
      form.setFieldsValue({
        documents: value.map((item: string) => ({ corpus: item })),
      });
    } catch (error) {}
    return Promise.resolve(true);
  };
  return (
    <>
      <ModalForm
        title="JSON"
        trigger={<Button size="small" type="primary" shape="circle" icon={<CopyOutlined />} />}
        modalProps={{
          destroyOnClose: true,
        }}
        onFinish={onFinish}
      >
        <ProFormTextArea
          name="copyText"
          placeholder={documentsExample}
          fieldProps={{
            rows: 6,
          }}
          rules={[
            {
              required: true,
              validator: async (_, value) => {
                if (isJSON(value.trim().replace(/'([^']*)'/g, '"$1"'))) {
                  return;
                }
                throw new Error(lGet('model.running.rerank.documentsCopyError') as string);
              },
            },
          ]}
        />
      </ModalForm>
    </>
  );
};
export default CopyDocuments;
