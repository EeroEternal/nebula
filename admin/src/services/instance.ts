import request from '../utils/request';

// 获取节点数据api
export async function getClusterList(params){
  return request('/cluster/info', {
    method: 'GET',
    params,
  });
}
// 获取模版引擎
export async function getEngines(params) {
  return request(`/engines/${params.name}`, {
    method: 'GET',
  });
}

export async function getGpuMax() {
  return request('/cluster/devices', {
    method: 'GET',
  });
}

export async function editInstance(params) {
  return request('/models/instance', {
    method: 'PUT',
    data: params
  });
}

export async function getInstanceLatest(params) {
  return request(`/instance/latest`, {
    method: 'GET',
    params: params,
  });
}

export async function getVersionList(urlparams){
  const pageparams = {
    curPageNum: -1,
    numPerPage: -1,
  }
  return request(`/models/${urlparams.type}/${urlparams.name}/versions`, {
    method: 'GET',
    params: pageparams,
  });
}
