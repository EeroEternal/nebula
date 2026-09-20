import { useEffect, useState } from 'react';
import request from '@/utils/request';

export type ClusterRuntime = {
  runtime?: 'docker' | 'k8s' | string;
  k8s?: boolean;
  gpu_mode?: string | null;
  hami_enabled?: boolean;
  namespace?: string | null;
};

/** 探活引擎运行时。默认非 K8s / 非 HAMi，避免侧栏先闪出 GPU 切片入口。 */
export function useK8sRuntime(): {
  isK8s: boolean;
  hamiEnabled: boolean;
  probed: boolean;
  runtime: ClusterRuntime | null;
} {
  const [isK8s, setIsK8s] = useState(false);
  const [hamiEnabled, setHamiEnabled] = useState(false);
  const [probed, setProbed] = useState(false);
  const [runtime, setRuntime] = useState<ClusterRuntime | null>(null);

  useEffect(() => {
    let mounted = true;
    request<{ data: { data?: ClusterRuntime } & ClusterRuntime }>('/cluster/runtime', {
      skipNotification: true,
    })
      .then((res) => {
        if (!mounted) return;
        const payload = (res?.data?.data ?? res?.data) as ClusterRuntime | undefined;
        const k8s = Boolean(payload?.k8s || payload?.runtime === 'k8s');
        setRuntime(payload || null);
        setIsK8s(k8s);
        setHamiEnabled(Boolean(payload?.hami_enabled));
        setProbed(true);
      })
      .catch(() => {
        if (!mounted) return;
        setIsK8s(false);
        setHamiEnabled(false);
        setRuntime(null);
        setProbed(true);
      });
    return () => {
      mounted = false;
    };
  }, []);

  return { isK8s, hamiEnabled, probed, runtime };
}
