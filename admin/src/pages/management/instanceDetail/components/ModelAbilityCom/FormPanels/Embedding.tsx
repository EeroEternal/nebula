import { ProFormTextArea } from '@ant-design/pro-components';
import { FC } from 'react';

import { l } from '@/utils/intl';

const Embedding: FC = () => {
  return (
    <ProFormTextArea
      label={l('models.instances.detail.prompt')}
      name="input"
      rules={[{ required: true }]}
      placeholder={l('models.instances.detail.promptEmbed.placeholder')}
      fieldProps={{ rows: 8 }}
    />
  );
};
export default Embedding;
