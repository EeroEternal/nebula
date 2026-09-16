import { useState } from "react"
import { RefreshCw } from "lucide-react"
import { Button } from "@/components/ui/button"
import { Badge } from "@/components/ui/badge"
import { Card } from "@/components/ui/card"
import {
  Table, TableBody, TableCell, TableHead, TableHeader, TableRow,
} from "@/components/ui/table"
import { PageShell } from "@/components/layout/page-shell"
import { PageContainer } from "@/components/layout/page-container"
import { PageHeader } from "@/components/layout/page-header"
import { EntityListToolbar } from "@/components/entity-list/EntityListToolbar"
import { Pagination } from "@/components/ui/pagination"
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

  const statusVariant = (code: number | undefined) => {
    if (!code) return "secondary" as const
    if (code >= 500) return "destructive" as const
    if (code >= 400) return "warning" as const
    return "success" as const
  }

  const fmtTs = (iso: string) => {
    try {
      return new Date(iso).toLocaleTimeString([], { hour: "2-digit", minute: "2-digit", second: "2-digit", hour12: false })
    } catch {
      return iso
    }
  }

  const getStatus = (entry: AuditLogEntry): number | undefined =>
    entry.output?.status ?? entry.metadata?.status ?? undefined

  const getRole = (entry: AuditLogEntry): string => {
    const tag = entry.tags?.find((s: string) => s.startsWith("role:"))
    if (tag) return tag.slice(5)
    return entry.metadata?.role || "SYSTEM"
  }

  return (
    <PageShell className="overflow-y-auto">
      <PageContainer>
        <PageHeader title={t('audit.title')} />
        <Card className="gap-0 p-4 sm:p-6">
          <EntityListToolbar
            searchValue={filterUser}
            onSearchChange={(v) => {
              setFilterUser(v)
              setPage(1)
            }}
            searchPlaceholder={t('audit.searchPlaceholder')}
            resultCount={`${meta.totalItems} ${t('audit.totalEntries')}`}
            rightActions={
              <Button variant="outline" size="sm" onClick={() => refetch()}>
                <RefreshCw className={cn("h-3.5 w-3.5", isLoading ? "animate-spin" : "")} />
                {t('common.refresh')}
              </Button>
            }
          />
          <Table>
            <TableHeader>
              <TableRow>
                <TableHead>{t('audit.timestamp')}</TableHead>
                <TableHead>{t('audit.principal')}</TableHead>
                <TableHead>{t('audit.authorization')}</TableHead>
                <TableHead>{t('audit.actionSequence')}</TableHead>
                <TableHead>{t('common.status')}</TableHead>
                <TableHead className="text-right">{t('audit.performance')}</TableHead>
              </TableRow>
            </TableHeader>
            <TableBody>
              {data.length === 0 && !isLoading ? (
                <TableRow>
                  <TableCell colSpan={6} className="h-48 text-center text-sm text-muted-foreground">
                    {t('audit.noData')}
                  </TableCell>
                </TableRow>
              ) : (
                data.map((item) => {
                  const status = getStatus(item)
                  return (
                    <TableRow key={item.id} className="hover:bg-muted/50">
                      <TableCell className="tabular-nums text-muted-foreground">{fmtTs(item.timestamp)}</TableCell>
                      <TableCell>{item.userId || "GUEST"}</TableCell>
                      <TableCell><Badge variant="outline">{getRole(item)}</Badge></TableCell>
                      <TableCell>{item.name || "—"}</TableCell>
                      <TableCell>
                        {status != null ? <Badge variant={statusVariant(status)}>HTTP {status}</Badge> : null}
                      </TableCell>
                      <TableCell className="text-right tabular-nums text-muted-foreground">
                        {item.latency ? `${Math.round(item.latency * 1000)}ms` : "—"}
                      </TableCell>
                    </TableRow>
                  )
                })
              )}
            </TableBody>
          </Table>
          {meta.totalPages > 1 ? (
            <div className="mt-4">
              <Pagination
                currentPage={page}
                pageSize={meta.limit}
                totalCount={meta.totalItems}
                onPageChange={setPage}
              />
            </div>
          ) : null}
        </Card>
      </PageContainer>
    </PageShell>
  )
}
