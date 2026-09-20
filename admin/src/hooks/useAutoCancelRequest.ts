import { useRequest } from 'ahooks';
import type { Options, Service } from 'ahooks/lib/useRequest/src/types';

interface ResponseData<TData> {
  data: TData;
  success: boolean; 
}

export default function useAutoCancelRequest<TData, TParams extends unknown[] = []>(
  service: Service<ResponseData<TData>, TParams>, // 服务函数返回 ResponseData<TData>
  options?: Options<ResponseData<TData>, TParams>
) {
  const { cancel, ...rest } = useRequest(service, {
    ...options,
    onSuccess: (response, params) => {
      if (options?.onSuccess) {
        options.onSuccess(response, params);
      }
      // 如果 success 为 false，取消请求
      if (!response.success) {
        cancel();
      }
    },
    onError: (error, params) => {
      if (options?.onError) {
        options.onError(error, params);
      }
      // 发生错误时取消请求
      cancel();
    },
  });

  return { ...rest, cancel };
}