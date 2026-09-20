import { DASHBOARD_PATH } from '@/constants';

/**
 * 根据 currentUser.permissions.page 判断路由是否可访问。
 * 纯函数，便于单测（避免 barrel `@/utils` 循环依赖）。
 */
export const hasPagePermissions = (
  pathname: string,
  pagePermissions: API.CurrentUser['permissions']['page'],
) => {
  if ([DASHBOARD_PATH, '/'].includes(pathname)) return true;
  const parts = pathname.replace(/^\/+/, '').split('/'); // ['models', 'repository']
  // Console / regions / 隐藏诊断页由页内鉴权，不走站点 page 权限
  if (parts[0] === 'console' || parts[0] === 'regions' || parts[0] === 'internal') {
    return true;
  }
  // 集群监控：兼容旧 platform / deviceInfo 权限
  if (parts[0] === 'monitor' && (parts[1] === 'cluster' || parts[1] === 'hami')) {
    const mon = pagePermissions?.monitor || [];
    return (
      mon.includes('cluster') || mon.includes('platform') || mon.includes('deviceInfo')
    );
  }
  // 模型实例：兼容旧 modelUsage 权限键
  if (parts[0] === 'monitor' && parts[1] === 'instances') {
    const mon = pagePermissions?.monitor || [];
    return mon.includes('instances') || mon.includes('modelUsage');
  }
  // 对话链路：兼容旧 traces / modelUsage 权限键
  if (parts[0] === 'monitor' && parts[1] === 'traffic') {
    const mon = pagePermissions?.monitor || [];
    return (
      mon.includes('traffic') || mon.includes('traces') || mon.includes('modelUsage')
    );
  }
  // 根据user中的permissions->page 去过滤
  if (parts[0] && parts[1] && !(pagePermissions?.[parts[0]] || []).includes(parts[1])) {
    return false;
  }
  return true;
};

/**
 * 是否具备系统级设置（数据库/认证/监控/License）权限。
 * 与后端 is_administrator（按 action 权限全集判断，再发 JWT admin scope）对齐：
 * 不以 role 名字符串短路。
 */
export const canManageSystemSettings = (user?: API.CurrentUser | null): boolean => {
  if (!user) return false;
  const action = user.permissions?.action || {};
  const required: Record<string, string[]> = {
    models: ['list', 'read', 'register', 'unregister', 'add'],
    instances: ['list', 'read', 'start', 'stop', 'delete'],
    users: ['add', 'modify', 'list', 'delete'],
    roles: ['add', 'modify', 'list', 'delete'],
    secrets: ['add', 'list', 'delete'],
    tasks: ['add', 'read', 'list', 'start', 'modify', 'delete', 'cancel'],
    caches: ['list', 'delete'],
    virtualenv: ['list', 'delete'],
  };
  return Object.entries(required).every(([key, acts]) =>
    acts.every((act) => (action[key] || []).includes(act)),
  );
};
