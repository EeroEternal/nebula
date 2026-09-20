import request from '@/utils/request';

export async function getList(params: API.PageParams){
  return request('/secrets', {
    method: 'GET',
    params,
  });
}

export async function deleteSerects(params){
  return request('/secrets', {
    method: 'DELETE',
    data: params
  });
}

export async function create(params){
  return request('/secrets', {
    method: 'POST',
    data: params
  });
}