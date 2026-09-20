import { FC } from 'react';
import { AndroidFilled, EditOutlined } from '@ant-design/icons';
import { l } from '@/utils/intl';
import { useModel } from '@umijs/max';

const ChatTips: FC = () => {
  const { instanceDetail } = useModel('management.instanceDetail.model');
  return (
    <div className="justify-start flex gap-x-[10px]">
      <div className="shrink-0 bg-[#f0f0f0] w-[32px] h-[32px] rounded-full flex items-center justify-center">
        <AndroidFilled />
      </div>
      <div className="p-[8px] bg-[#f5f5f5] rounded-[10px] rounded-tl-none">
        {l('model.running.chat.tips.firstRow', undefined, {
          modelId: instanceDetail.model_uid,
          modelName: instanceDetail.model_name,
        })}
        <br />
        {l('model.running.chat.tips.secondRow')}
        <div>{l('model.running.chat.tips.threeRow')}</div>
        <div>{l('model.running.chat.tips.fourRow', undefined, { icon: <EditOutlined /> })}</div>
      </div>
    </div>
  );
};
export default ChatTips;
