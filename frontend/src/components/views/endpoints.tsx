import { Globe, Server } from "lucide-react"
import { Badge } from "@/components/ui/badge"
import { Table, TableBody, TableCell, TableHead, TableHeader, TableRow } from "@/components/ui/table"
import { Card } from "@/components/ui/card"
import { StatCard } from "@/components/ui/stat-card"
import { PageShell } from "@/components/layout/page-shell"
import { PageContainer } from "@/components/layout/page-container"
import { PageHeader } from "@/components/layout/page-header"
import { EngineAlertsBanner } from "@/components/engine-alerts-banner"
import { useClusterOverview } from "@/hooks/useClusterOverview"
import { useAuthStore } from "@/store/useAuthStore"
import { endpointStatusTone, formatNodeGpu } from "@/lib/endpoint-status"
import { useI18n } from "@/lib/useI18n"

export function EndpointsView() {
    const { t } = useI18n()
    const { data: overview } = useClusterOverview()
    const { token } = useAuthStore()

    const assignmentFor = (modelUid: string, replicaId: number) => {
        for (const p of overview?.placements ?? []) {
            if (p.model_uid !== modelUid) continue
            return p.assignments.find((a) => a.replica_id === replicaId) ?? null
        }
        return null
    }

    const endpoints = overview?.endpoints ?? []

    return (
        <PageShell className="overflow-y-auto">
            <PageContainer>
                <PageHeader title={t('endpoints.apiTitle')} />
                <div className="space-y-6 pb-6">
                    <EngineAlertsBanner token={token ?? undefined} />
                    <div className="grid grid-cols-1 gap-4 md:grid-cols-3">
                        <StatCard title={t('endpoints.total')} value={endpoints.length} subtitle={t('endpoints.loadBalanced')} icon={Globe} />
                        <StatCard title={t('endpoints.activeProtocols')} value="2" subtitle="REST/OAI · gRPC" icon={Server} />
                        <StatCard title={t('endpoints.meshHealth')} value={t('endpoints.nominal')} icon={Server} />
                    </div>
                    <Card className="gap-0 overflow-hidden py-0">
                        <Table>
                            <TableHeader>
                                <TableRow>
                                    <TableHead className="pl-5">{t('endpoints.identity')}</TableHead>
                                    <TableHead>{t('endpoints.computingResource')}</TableHead>
                                    <TableHead>{t('endpoints.interface')}</TableHead>
                                    <TableHead>{t('endpoints.targetUrl')}</TableHead>
                                    <TableHead className="pr-5 text-right">{t('endpoints.connectivity')}</TableHead>
                                </TableRow>
                            </TableHeader>
                            <TableBody>
                                {endpoints.length === 0 ? (
                                    <TableRow>
                                        <TableCell colSpan={5} className="h-48 text-center text-sm text-muted-foreground">
                                            {t('endpoints.noActive')}
                                        </TableCell>
                                    </TableRow>
                                ) : (
                                    endpoints.map((ep) => {
                                        const tone = endpointStatusTone(ep.status)
                                        return (
                                            <TableRow key={`${ep.model_uid}-${ep.replica_id}`} className="hover:bg-muted/50">
                                                <TableCell className="pl-5">
                                                    <div className="font-medium">{ep.model_uid}</div>
                                                    <div className="text-meta-sm text-muted-foreground">{t('endpoints.replicaId')}: {ep.replica_id}</div>
                                                </TableCell>
                                                <TableCell className="text-sm">{formatNodeGpu(ep.node_id, assignmentFor(ep.model_uid, ep.replica_id))}</TableCell>
                                                <TableCell><Badge variant="outline">{ep.api_flavor}</Badge></TableCell>
                                                <TableCell className="max-w-[280px] truncate text-meta-sm text-muted-foreground">
                                                    {ep.base_url || ep.grpc_target || "—"}
                                                </TableCell>
                                                <TableCell className="pr-5 text-right">
                                                    <Badge variant={tone === "success" ? "success" : tone === "destructive" ? "destructive" : "warning"}>
                                                        {ep.status}
                                                    </Badge>
                                                    {ep.status_detail ? (
                                                        <p className="mt-1 truncate text-meta-sm text-muted-foreground" title={ep.status_detail}>{ep.status_detail}</p>
                                                    ) : null}
                                                </TableCell>
                                            </TableRow>
                                        )
                                    })
                                )}
                            </TableBody>
                        </Table>
                    </Card>
                </div>
            </PageContainer>
        </PageShell>
    )
}
