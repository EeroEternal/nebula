import { Button, Row, Col } from 'antd';
import { ProFormList, ProCard, ProFormText, ProFormDigit } from '@ant-design/pro-components';
import type { FormListActionType } from '@ant-design/pro-components';
import { DeleteOutlined, PlusOutlined } from '@ant-design/icons';
import { useRef } from 'react';

import { l } from '@/utils/intl';

const ModelStop = () => {
  const modelControlnetRef = useRef<FormListActionType>();
  const handleAdd = () => {
    modelControlnetRef.current?.add({
      stop_token_id: '',
      stop: '',
    });
  };

  return (
    <>
      <div className="flex items-center">
        {l('model.register.stop')}
        <Button
          className="ml-[8px]"
          type="primary"
          onClick={handleAdd}
          icon={<PlusOutlined />}
        >
          {l('model.register.more')}
        </Button>
      </div>
      <ProCard bordered className="my-[8px] w-full">
        <ProFormList
          name="stopController"
          className="w-full-pro-form-list"
          actionRef={modelControlnetRef}
          copyIconProps={false}
          creatorButtonProps={false}
          initialValue={[{ stop_token_id: '', stop: '' }]}
          min={1}
          actionRender={() => []}
        >
          {(meta, index, action, count) => {
            return (
              <div className="flex items-end gap-x-[20px]">
                <Row gutter={[10, 10]} className="flex-1">
                  <Col span={12}>
                    <ProFormDigit
                      name="stop_token_id"
                      label={l('model.register.stopTokenID')}
                      placeholder={l('model.register.stopTokenIDTips')}
                      rules={[
                        { required: true },
                        { pattern: /^[0-9]\d*$/, message: l('model.register.stopTokenIDRuleTips') },
                      ]}
                    />
                  </Col>
                  <Col span={12}>
                    <ProFormText
                      name="stop"
                      label={l('model.register.stop')}
                      placeholder={l('model.register.stopTips')}
                      rules={[{ required: true }]}
                    />
                  </Col>
                </Row>
                {count > 1 ? (
                  <div
                    key="delete"
                    className="mb-[8px] w-[30px] h-[30px] text-[18px] text-[#8c8c8c] bg-card rounded-full flex items-center justify-center hover:bg-primary hover:text-[#fff] cursor-pointer"
                    onClick={() => modelControlnetRef.current?.remove(index)}
                  >
                    <DeleteOutlined />
                  </div>
                ) : (
                  <div className="w-[30px]" />
                )}
              </div>
            );
          }}
        </ProFormList>
      </ProCard>
    </>
  );
};
export default ModelStop;
