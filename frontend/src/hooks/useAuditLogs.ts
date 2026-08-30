import { useQuery } from '@tanstack/react-query'
import { useAuthStore } from '@/store/useAuthStore'

const BASE_URL = import.meta.env.VITE_BFF_BASE_URL || "/api"

export interface AuditLogEntry {
  id: string
  timestamp: string
  userId?: string | null
  name?: string | null
  latency?: number | null
  tags?: string[]
  output?: {
    status?: number | null
  } | null
  metadata?: {
    status?: number | null
    role?: string | null
  } | null
}

export interface AuditLogsResponse {
  data: AuditLogEntry[]
  meta: {
    page: number
    limit: number
    totalItems: number
    totalPages: number
  }
}

export function useAuditLogs(page: number, userId: string = "") {
  const { token } = useAuthStore()
  
  return useQuery({
    queryKey: ['audit-logs', page, userId],
    queryFn: async () => {
      const params = new URLSearchParams({ page: String(page), limit: "50" })
      if (userId.trim()) params.set("userId", userId.trim())
      const resp = await fetch(`${BASE_URL}/audit-logs?${params}`, {
        headers: token ? { Authorization: `Bearer ${token}` } : {},
      })
      if (!resp.ok) {
          const text = await resp.text()
          throw new Error(text || `HTTP ${resp.status}`)
      }
      return resp.json() as Promise<AuditLogsResponse>
    },
    enabled: !!token,
    staleTime: 10000,
  })
}
