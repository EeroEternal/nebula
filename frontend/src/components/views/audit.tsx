import { useState } from "react"
import { Shield, RefreshCw, ChevronLeft, ChevronRight, Search, Clock, User, Fingerprint, Timer, Lock } from "lucide-react"
import { Button } from "@/components/ui/button"
import { Badge } from "@/components/ui/badge"
import {
  Table,
  TableBody,
  TableCell,
  TableHead,
  TableHeader,
  TableRow,
} from "@/components/ui/table"
import { useI18n } from "@/lib/useI18n"
import { useAuditLogs, type AuditLogEntry } from "@/hooks/useAuditLogs"
import { cn } from "@/lib/utils"

export function AuditView() {
  const { t } = useI18n()
  const [page, setPage] = useState(1)
  const [filterUser, setFilterUser] = useState("")
  const { data: response, isLoading, refetch } = useAuditLogs(page, filterUser)

  const data = response?.data || []
  const meta = response?.meta || { page: 1, limit: 50, totalItems: 0, totalPages: 0 }

  const statusColor = (code: number | undefined) => {
    if (!code) return "bg-muted text-muted-foreground border-border"
    if (code >= 500) return "bg-destructive/10 text-destructive border-destructive/20"
    if (code >= 400) return "bg-warning/10 text-warning border-warning/20"
    return "bg-success/10 text-success border-success/20"
  }

  const fmtTs = (iso: string) => {
    try {
      const d = new Date(iso)
      return d.toLocaleTimeString([], { hour: '2-digit', minute: '2-digit', second: '2-digit', hour12: false })
    } catch { return iso }
  }

  const getStatus = (t: AuditLogEntry): number | undefined => {
    if (t.output?.status) return t.output.status
    if (t.metadata?.status) return t.metadata.status
    return undefined
  }

  const getRole = (t: AuditLogEntry): string => {
    const tag = t.tags?.find((s: string) => s.startsWith("role:"))
    if (tag) return tag.slice(5)
    return t.metadata?.role || "SYSTEM"
  }

  return (
    <div className="space-y-8 animate-in fade-in duration-500">
      <div className="flex flex-col md:flex-row md:items-end justify-between gap-4">
        <div>
          <h2 className="text-3xl font-bold tracking-tight font-mono uppercase text-foreground">{t('audit.title')}</h2>
          <p className="text-muted-foreground mt-2 flex items-center gap-2">
            <Lock className="h-4 w-4 text-primary" />
            {t('audit.subtitle')}
          </p>
        </div>
        <div className="flex gap-3">
          <div className="relative w-64 group">
            <Search className="absolute left-3 top-1/2 -translate-y-1/2 h-4 w-4 text-muted-foreground group-focus-within:text-primary transition-colors" />
            <input
              type="text"
               placeholder={t('audit.searchPlaceholder')}
              value={filterUser}
              onChange={(e) => setFilterUser(e.target.value)}
              className="w-full bg-black/20 border border-border/50 rounded-lg pl-10 pr-4 py-2 text-xs font-mono focus:outline-none focus:border-primary/50 transition-all"
            />
          </div>
          <Button
            variant="outline"
            size="sm"
            onClick={() => refetch()}
            className="h-10 px-4 bg-white/5 border-border/50 font-mono text-[10px] uppercase tracking-widest"
          >
            <RefreshCw className={cn("h-3.5 w-3.5 mr-2", isLoading ? "animate-spin" : "")} />
            {t('common.refresh')}
          </Button>
        </div>
      </div>

      <div className="bg-card/40 backdrop-blur-xl border border-border rounded-xl overflow-hidden">
        <div className="px-6 py-4 border-b border-border/50 flex items-center justify-between bg-white/5">
            <h3 className="text-xs font-bold font-mono uppercase tracking-widest text-muted-foreground">{t('audit.operationLedger')}</h3>
            <Badge variant="outline" className="font-mono text-[10px] border-primary/20 text-primary uppercase">
                {meta.totalItems} {t('audit.totalEntries')}
            </Badge>
        </div>

        <Table>
          <TableHeader className="bg-black/20">
            <TableRow className="border-border/50 hover:bg-transparent">
              <TableHead className="text-[10px] uppercase font-bold text-muted-foreground px-6 py-4">{t('audit.timestamp')}</TableHead>
              <TableHead className="text-[10px] uppercase font-bold text-muted-foreground">{t('audit.principal')}</TableHead>
              <TableHead className="text-[10px] uppercase font-bold text-muted-foreground">{t('audit.authorization')}</TableHead>
              <TableHead className="text-[10px] uppercase font-bold text-muted-foreground">{t('audit.actionSequence')}</TableHead>
              <TableHead className="text-[10px] uppercase font-bold text-muted-foreground">{t('common.status')}</TableHead>
              <TableHead className="text-right text-[10px] uppercase font-bold text-muted-foreground pr-6">{t('audit.performance')}</TableHead>
            </TableRow>
          </TableHeader>
          <TableBody>
            {data.length === 0 && !isLoading ? (
              <TableRow>
                <TableCell colSpan={6} className="h-64 text-center">
                  <div className="flex flex-col items-center justify-center opacity-30 gap-3">
                    <Shield className="h-12 w-12" />
                    <p className="text-[10px] font-mono uppercase tracking-widest">{t('audit.noData')}</p>
                  </div>
                </TableCell>
              </TableRow>
            ) : (
                data.map((item) => {
                    const status = getStatus(item)
                    const role = getRole(item)
                    return (
                        <TableRow key={item.id} className="border-border/40 hover:bg-white/5 transition-colors group">
                            <TableCell className="px-6 py-4">
                                <div className="flex items-center gap-2 text-muted-foreground">
                                    <Clock className="h-3 w-3" />
                                    <span className="text-[11px] font-mono font-bold tracking-widest">{fmtTs(item.timestamp)}</span>
                                </div>
                            </TableCell>
                            <TableCell>
                                <div className="flex items-center gap-2">
                                    <User className="h-3 w-3 text-primary/50" />
                                    <span className="text-xs font-mono font-bold text-foreground">{item.userId || "GUEST"}</span>
                                </div>
                            </TableCell>
                            <TableCell>
                                <Badge variant="outline" className="font-mono text-[9px] uppercase border-border/50 text-muted-foreground">
                                    {role}
                                </Badge>
                            </TableCell>
                            <TableCell>
                                <div className="flex items-center gap-2">
                                    <Fingerprint className="h-3 w-3 text-muted-foreground/50" />
                                    <span className="text-[11px] font-mono uppercase tracking-tight text-foreground group-hover:text-primary transition-colors">{item.name || "UNSPECIFIED"}</span>
                                </div>
                            </TableCell>
                            <TableCell>
                                {status != null && (
                                    <Badge variant="outline" className={cn("text-[9px] font-bold h-5 px-1.5 uppercase", statusColor(status))}>
                                        HTTP {status}
                                    </Badge>
                                )}
                            </TableCell>
                            <TableCell className="text-right pr-6">
                                <div className="flex items-center justify-end gap-1.5 text-muted-foreground">
                                    <Timer className="h-3 w-3" />
                                    <span className="text-[11px] font-mono">{item.latency ? `${Math.round(item.latency * 1000)}ms` : "—"}</span>
                                </div>
                            </TableCell>
                        </TableRow>
                    )
                })
            )}
          </TableBody>
        </Table>
      </div>

      {meta.totalPages > 1 && (
        <div className="flex items-center justify-between bg-card/20 p-4 rounded-xl border border-border/50">
          <p className="text-[10px] font-mono text-muted-foreground uppercase tracking-widest">
            {t('audit.page', { page: meta.page, total: meta.totalPages })} ● ENTRIES { (meta.page - 1) * meta.limit + 1 } - { Math.min(meta.page * meta.limit, meta.totalItems) }
          </p>
          <div className="flex gap-2">
            <Button
              variant="outline"
              size="sm"
              className="h-8 bg-black/20 border-border/50 hover:bg-white/10"
              disabled={page <= 1}
              onClick={() => setPage((p) => Math.max(1, p - 1))}
            >
              <ChevronLeft className="h-4 w-4" />
            </Button>
            <Button
              variant="outline"
              size="sm"
              className="h-8 bg-black/20 border-border/50 hover:bg-white/10"
              disabled={page >= meta.totalPages}
              onClick={() => setPage((p) => p + 1)}
            >
              <ChevronRight className="h-4 w-4" />
            </Button>
          </div>
        </div>
      )}
    </div>
  )
}
