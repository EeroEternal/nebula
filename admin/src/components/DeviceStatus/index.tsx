import { FC } from 'react';
import { Tag, Tooltip } from 'antd';
import { HelpCircle } from 'lucide-react';
import classNames from 'classnames';
import { DeviceInfo } from '@/types/Public/data';
import { l } from '@/utils/intl';

const DeviceStatus: FC<{ status: DeviceInfo['status']; className?: string }> = ({
  status,
  className = '',
}) => {
  const tagPorps = {
    bordered: false,
    className: classNames('mr-0 !px-2.5 rounded-full', className),
  };
  if (status === 'online') {
    return (
      <Tag {...tagPorps} color="success">
        {l('monitor.deviceInfo.status.online')}
      </Tag>
    );
  }
  if (status === 'offline') {
    return (
      <Tag {...tagPorps} color="error">
        {l('monitor.deviceInfo.status.offline')}
      </Tag>
    );
  }
  if (status === 'expired') {
    return (
      <Tooltip title={l('global.license.disabledTips')}>
        <Tag {...tagPorps} color="warning">
          {l('monitor.deviceInfo.status.unauthorized')} <HelpCircle />
        </Tag>
      </Tooltip>
    );
  }
  return null;
};
export default DeviceStatus;
