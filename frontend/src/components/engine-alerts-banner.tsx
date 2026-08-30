import { useCallback, useEffect, useMemo, useState } from "react"
import { AlertTriangle } from "lucide-react"
import { v2 } from "@/lib/api"
import { engineAlertLabel, isEngineAlertCritical } from "@/lib/endpoint-status"
import { useI18n } from "@/lib/useI18n"
import type { AlertsSummary } from "@/lib/types"

interface EngineAlertsBannerProps {
  token?: string
  /** When set, only show alerts for this model_uid. */
  modelUid?: string
  pollMs?: number
}

export function EngineAlertsBanner({ token, modelUid, pollMs = 10000 }: EngineAlertsBannerProps) {
  const { t } = useI18n()
  const [alerts, setAlerts] = useState<AlertsSummary>({ disk: [], engine: [] })

  const refresh = useCallback(() => {
    if (!token) return
    v2.listAlerts(token)
      .then(setAlerts)
      .catch(() => setAlerts({ disk: [], engine: [] }))
  }, [token])

  useEffect(() => {
    refresh()
    const id = setInterval(refresh, pollMs)
    return () => clearInterval(id)
  }, [refresh, pollMs])

  const rows = useMemo(() => {
    const out: Array<{
      key: string
      critical: boolean
      label: string
      message: string
      meta: string
    }> = []
    for (const alert of alerts.disk) {
      out.push({
        key: `disk-${alert.node_id}-${alert.alert_type}-${alert.created_at_ms}`,
        critical: alert.alert_type === "disk_critical",
        label: alert.alert_type === "disk_critical" ? t("dashboard.critical") : t("dashboard.warning"),
        message: alert.message,
        meta: alert.node_id,
      })
    }
    for (const alert of alerts.engine) {
      if (modelUid && alert.model_uid !== modelUid) continue
      out.push({
        key: `engine-${alert.node_id}-${alert.model_uid}-${alert.replica_id}-${alert.created_at_ms}`,
        critical: isEngineAlertCritical(alert.alert_type),
        label: engineAlertLabel(alert.alert_type, t),
        message: alert.message,
        meta: `${alert.model_uid} · ${alert.node_id} · r${alert.replica_id}`,
      })
    }
    return out
  }, [alerts, modelUid, t])

  if (rows.length === 0) return null

  return (
    <div className="space-y-2">
      {rows.map((row) => (
        <div
          key={row.key}
          className={`flex items-start gap-3 rounded-xl border px-4 py-3 text-sm ${
            row.critical
              ? "border-destructive/40 bg-destructive/5 text-destructive"
              : "border-warning/40 bg-warning/5 text-warning"
          }`}
        >
          <AlertTriangle className="h-4 w-4 mt-0.5 shrink-0" />
          <div className="min-w-0">
            <p className="font-semibold text-[11px] uppercase tracking-widest">{row.label}</p>
            <p className="text-foreground/90 break-words">{row.message}</p>
            <p className="text-[10px] font-mono text-muted-foreground mt-1">{row.meta}</p>
          </div>
        </div>
      ))}
    </div>
  )
}
