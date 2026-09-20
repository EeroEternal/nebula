import { extend } from 'umi-request';
import type { RequestOptionsInit } from 'umi-request';

const request = extend({});
// 请求拦截
request.interceptors.request.use((url: string, options: RequestOptionsInit) => {
  const headers = {
    'Content-Type': 'application/json',
    'Access-Control-Allow-Origin': '*',
    Accept: 'application/json',
    Authorization: options?.Authorization,
  };

  return {
    url,
    options: { ...options, headers },
  };
});
// 响应拦截
request.interceptors.response.use(async (response: Response) => response);

export default request;
