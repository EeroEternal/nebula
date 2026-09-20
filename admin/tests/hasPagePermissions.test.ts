import { hasPagePermissions, canManageSystemSettings } from '@/utils/permissions';

describe('hasPagePermissions', () => {
  const pages: API.CurrentUser['permissions']['page'] = {
    models: ['repository', 'instances', 'register'],
    monitor: ['platform', 'logs'],
    admin: ['users'],
  };

  it('always allows dashboard and root', () => {
    expect(hasPagePermissions('/dashboard', pages)).toBe(true);
    expect(hasPagePermissions('/', pages)).toBe(true);
  });

  it('allows paths listed under page permissions', () => {
    expect(hasPagePermissions('/models/repository', pages)).toBe(true);
    expect(hasPagePermissions('/models/repository/LLM', pages)).toBe(true);
    expect(hasPagePermissions('/monitor/platform', pages)).toBe(true);
  });

  it('denies unlisted second segment', () => {
    expect(hasPagePermissions('/models/drawingTool', pages)).toBe(false);
    expect(hasPagePermissions('/admin/roles', pages)).toBe(false);
    // 独立 /models/downloads 不在 page.models；任务页须挂 repository 下
    expect(hasPagePermissions('/models/downloads', pages)).toBe(false);
    expect(hasPagePermissions('/models/repository/downloads', pages)).toBe(true);
  });

  it('treats GPU slice like cluster monitor permissions', () => {
    expect(hasPagePermissions('/monitor/hami', pages)).toBe(true);
    expect(
      hasPagePermissions('/monitor/hami', { monitor: ['cluster'] }),
    ).toBe(true);
  });

  it('allows single-segment paths by default', () => {
    // only checks when parts[0] && parts[1]
    expect(hasPagePermissions('/settings', pages)).toBe(true);
  });

  it('allows hidden internal diagnostic paths (page-level admin gate)', () => {
    expect(hasPagePermissions('/internal/flags', pages)).toBe(true);
  });
});

const fullAdminAction = {
  models: ['list', 'read', 'register', 'unregister', 'add'],
  instances: ['list', 'read', 'start', 'stop', 'delete'],
  users: ['add', 'modify', 'list', 'delete'],
  roles: ['add', 'modify', 'list', 'delete'],
  secrets: ['add', 'list', 'delete'],
  tasks: ['add', 'read', 'list', 'start', 'modify', 'delete', 'cancel'],
  caches: ['list', 'delete'],
  virtualenv: ['list', 'delete'],
};

describe('canManageSystemSettings', () => {
  it('allows users with full admin actions regardless of role name', () => {
    expect(
      canManageSystemSettings({
        role: 'custom-admin',
        permissions: { page: {}, action: fullAdminAction },
      } as API.CurrentUser),
    ).toBe(true);
  });

  it('denies role=administrator when actions are incomplete', () => {
    expect(
      canManageSystemSettings({
        role: 'administrator',
        permissions: { page: {}, action: {} },
      } as API.CurrentUser),
    ).toBe(false);
  });

  it('denies ordinary user without full admin actions', () => {
    expect(
      canManageSystemSettings({
        role: 'user',
        permissions: {
          page: { models: ['repository'] },
          action: { models: ['list', 'read'], secrets: [] },
        },
      } as API.CurrentUser),
    ).toBe(false);
  });
});
