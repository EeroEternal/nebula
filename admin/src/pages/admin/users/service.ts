import request from '@/utils/request';
import type {CreateUserParams, UpdateUserParams} from './data';

export async function getList(params: API.PageParams){
  return request('/users', {
    method: 'GET',
    params,
  });
}

export async function createUsers(params: CreateUserParams){
  return request('/users', {
    method: 'POST',
    data: params,
  });
}

export async function updateUsers(params: UpdateUserParams){
  return request('/users', {
    method: 'PUT',
    data: params,
  });
}

// 获取角色
export async function getRole(){
  const params: API.PageParams = {
    curPageNum: -1,
    numPerPage: -1,
  }
  return request('/roles', {
    method: 'GET',
    params,
  });
}


// 列举所有权限
export async function getPermissionList() {
  return request('/roles/permissions', {
    method: 'GET',
  });
}