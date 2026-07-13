import { useQuery } from '@tanstack/react-query'
import { v2 } from '@/lib/api'
import { useAuthStore } from '@/store/useAuthStore'

export function useCells() {
  const { token } = useAuthStore()
  return useQuery({
    queryKey: ['cells'],
    queryFn: () => v2.listCells(token || ''),
    refetchInterval: 15_000,
  })
}
