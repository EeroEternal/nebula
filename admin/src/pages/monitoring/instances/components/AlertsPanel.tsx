import { Result } from 'antd';
import { l } from '@/utils/intl';

/**
 * 告警 Tab：产品内告警/通知未开放。
 * 展示 403 样式，文案为「功能还在开发」（非权限不足）。
 */
const AlertsPanel = () => (
  <Result
    status="403"
    title="403"
    subTitle={l('monitor.alerts.wip')}
  />
);

export default AlertsPanel;
