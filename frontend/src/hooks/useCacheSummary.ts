import { useQuery } from '@tanstack/react-query'
import { apiGet } from '@/lib/api'
import { useAuthStore } from '@/store/useAuthStore'
import type { ModelCacheEntry } from '@/lib/types'

export interface CacheSummary {
  caches: Array<ModelCacheEntry & { matched_model_uids?: string[] }>
}

export function useCacheSummary() {
  const { token } = useAuthStore()
  
  return useQuery({
    queryKey: ['cache-summary'],
    queryFn: () => apiGet<CacheSummary>('/v2/cache/summary', token || ''),
    enabled: !!token,
    refetchInterval: 10000,
  })
}
