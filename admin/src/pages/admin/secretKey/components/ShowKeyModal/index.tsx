import { l } from '@/utils/intl';
import { CopyOutlined } from '@ant-design/icons';
import { getIntl } from '@umijs/max';
import { Button, Input, Modal, message } from 'antd';
import ClipboardJS from 'clipboard';
import { useEffect, useRef } from 'react';
import type {} from '../../data';

type ShowProps = {
  visible: boolean;
  onSubmit: () => void;
  values: string;
};

const CreateForm = (props: ShowProps) => {
  const [messageApi] = message.useMessage();
  const clipboardRef = useRef(null);
  const { visible, onSubmit, values } = props;
  useEffect(() => {
    if (clipboardRef.current) {
      const clipboard = new ClipboardJS(clipboardRef.current, {
        text: () => values,
      });

      clipboard.on('success', function (e) {
        message.success(getIntl().formatMessage({ id: 'management.key.copySuccess' }));
        // messageApi.success(intl().formatMessage({ id: 'management.key.copySuccess' }));
        e.clearSelection(); // 清除选中状态
      });
      return () => {
        clipboard.destroy();
      };
    }
    return () => {};
  }, []);
  return (
    <Modal
      title={l('management.key.createKey')}
      open={visible}
      onCancel={onSubmit}
      footer={[
        <Button key="submit" type="primary" onClick={onSubmit}>
          {l('global.actions.finish')}
        </Button>,
      ]}
    >
      <div style={{ marginBottom: '20px' }}>{l('management.key.keyTip')}</div>
      <Input
        value={values}
        readOnly
        addonAfter={
          <div ref={clipboardRef}>
            <CopyOutlined />
          </div>
        }
      />
    </Modal>
  );
};
export default CreateForm;
