import { useState, type ReactNode } from 'react';
import { formatDisplayTime } from '@/utils';
import { Alert } from 'antd';
import { Settings } from 'lucide-react';

import type { LicenseStatus } from '@/types/global';
import { useIntl, useModel } from '@umijs/max';
import { canManageSystemSettings } from '@/utils/permissions';

const SettingsIcon = (
  <Settings
    size={14}
    aria-hidden
    className="inline-block align-[-2px] mx-0.5 text-default"
  />
);

const getBannerMessage = (
  licenseStatus: LicenseStatus,
  intl: ReturnType<typeof useIntl>,
  settingsIcon: ReactNode,
) => {
  const values = { settingsIcon };
  switch (licenseStatus.state) {
    case 'expired':
      return intl.formatMessage(
        { id: 'global.license.banner.expired' },
        {
          ...values,
          expireAt: licenseStatus.expire_at
            ? formatDisplayTime(licenseStatus.expire_at)
            : '',
        },
      );
    case 'invalid':
      return intl.formatMessage({ id: 'global.license.banner.invalid' }, values);
    case 'missing':
      return intl.formatMessage({ id: 'global.license.banner.missing' }, values);
    case 'unknown':
      return intl.formatMessage({ id: 'global.license.banner.unknown' }, values);
    default:
      return '';
  }
};

const LicenseReminderBanner = () => {
  const { initialState } = useModel('@@initialState');
  const intl = useIntl();
  const [dismissed, setDismissed] = useState(false);
  const licenseStatus = initialState?.licenseStatus;

  // 非管理员无法打开 License 设置，不展示无效告警横幅
  if (!canManageSystemSettings(initialState?.currentUser)) {
    return null;
  }
  if (dismissed || !licenseStatus || licenseStatus.state === 'valid') {
    return null;
  }

  const message = getBannerMessage(licenseStatus, intl, SettingsIcon);

  return (
    <Alert
      banner
      closable
      showIcon
      type={licenseStatus.state === 'invalid' ? 'error' : 'warning'}
      message={message}
      onClose={() => setDismissed(true)}
    />
  );
};

export default LicenseReminderBanner;
