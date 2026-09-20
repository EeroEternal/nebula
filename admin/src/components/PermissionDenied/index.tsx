import { FC } from 'react';
import { Button, Result } from 'antd';
import { history } from '@umijs/max';
import { l } from '@/utils/intl';

export type PermissionDeniedScene = 'page' | 'settings';

interface PermissionDeniedProps {
  scene?: PermissionDeniedScene;
  /** page 场景下是否展示返回按钮；settings 弹窗内默认不展示 */
  showBack?: boolean;
}

/**
 * 无权限提示：与 404 同款 Result。
 * 注意：antd 在 status=403/404/500 时会忽略 icon prop，设置弹窗内缩放靠 CSS transform。
 */
const PermissionDenied: FC<PermissionDeniedProps> = ({ scene = 'page', showBack }) => {
  const showBackBtn = showBack ?? scene === 'page';
  const subTitle = l(
    scene === 'settings' ? 'app.settings.permission.subTitle' : 'pages.403.subTitle',
  );
  const isSettings = scene === 'settings';

  return (
    <div
      className={
        isSettings
          ? 'settings-permission-denied h-full min-h-0 overflow-hidden flex items-center justify-center'
          : undefined
      }
    >
      <Result
        status="403"
        title="403"
        subTitle={subTitle}
        className={isSettings ? 'settings-permission-denied-result' : undefined}
        extra={
          showBackBtn ? (
            <Button type="primary" onClick={() => history.push('/')}>
              {l('pages.403.buttonText')}
            </Button>
          ) : null
        }
      />
    </div>
  );
};

export default PermissionDenied;
