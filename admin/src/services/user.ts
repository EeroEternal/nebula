/* eslint-disable */
import request from '../utils/request';
import type { GlobalConfig } from '@/types/global';
import { getCurrentRegion, isRegionModeEnabled } from '@/utils/region';
export interface LoginType {
  userName: string;
  password: string;
}

export async function login(params: API.LoginParams) {
  // 多地点模式：走 Console 统一登录（转发到认证站点；各站共享 AUTH_SECRET_KEY 时该 token 全站有效）
  if (isRegionModeEnabled() && getCurrentRegion()) {
    return request(`${window.location.origin}/api/console/auth/signin`, {
      method: 'POST',
      data: params,
    });
  }
  return request('/user/signin', {
    method: 'POST',
    data: params,
  });
}

export async function outLogin() {
  return request('/user/logout', {
    method: 'POST',
  });
}

// 获取角色
export async function getRole() {
  const params: API.PageParams = {
    curPageNum: -1,
    numPerPage: -1,
  };
  return request('/roles', {
    method: 'GET',
    params,
  });
}

// 更改密码
export async function updatePassword(params: { old_password: string; new_password: string }) {
  return request('/user/password', {
    method: 'POST',
    data: params,
  });
}
export function changeUserConfig(data: Partial<API.CurrentUser>) {
  return request('/user/me', { method: 'post', data: data });
}
export async function getGlobalConfig() {
  const [globalRes] = await Promise.allSettled([request('/setting/global')]);
  const config = (
    globalRes.status === 'fulfilled' ? globalRes.value?.data?.data : {}
  ) as GlobalConfig;
  return {
    globalConfig: config,
  };
}
