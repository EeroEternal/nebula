import request from '@/utils/request';

interface OperationsParams {
  curPageNum?: number;
  numPerPage?: number;
}
export function getOperations(params?: OperationsParams) {
  return request('/user/operations', {
    method: 'get',
    params: params || { curPageNum: 1, numPerPage: 20 },
  });
}
