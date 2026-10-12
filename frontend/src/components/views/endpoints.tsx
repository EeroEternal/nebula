import { Globe, Server } from "lucide-react"
import { useEffect, useState } from "react"
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
import { v2 } from "@/lib/api"
import type { GatewayPrefixAffinity } from "@/lib/types"

export function EndpointsView() {
    const { t } = useI18n()
    const { data: overview } = useClusterOverview()
    const { token } = useAuthStore()
    const [affinity, setAffinity] = useState<GatewayPrefixAffinity | null>(null)
    const [affinityFailed, setAffinityFailed] = useState(false)

    // Assessment A2: poll like the rest of the page (overview refetches every
    // 10s) and distinguish fetch failure from "affinity not enabled".
    useEffect(() => {
        let cancelled = false
        const load = () => {
            v2.gatewayPrefixAffinity(token ?? undefined)
                .then((data) => { if (!cancelled) { setAffinity(data); setAffinityFailed(false) } })
                .catch(() => { if (!cancelled) setAffinityFailed(true) })
        }
        load()
        const timer = setInterval(load, 10_000)
        return () => { cancelled = true; clearInterval(timer) }
    }, [token])

    const endpoints = overview?.endpoints ?? []

    // Real values from live endpoint data (assessment P0-4: these used to be
    // hardcoded — a console must not display fabricated health).
    const flavors = Array.from(new Set(endpoints.map((ep) => ep.api_flavor))).filter(Boolean)
    const meshHealth = overview === undefined || endpoints.length === 0
        ? t('endpoints.unknown')
        : endpoints.every((ep) => ep.status === 'ready')
            ? t('endpoints.nominal')
            : t('endpoints.degraded')

    const assignmentFor = (modelUid: string, replicaId: number) => {
        for (const p of overview?.placements ?? []) {
            if (p.model_uid !== modelUid) continue
            return p.assignments.find((a) => a.replica_id === replicaId) ?? null
        }
        return null
    }

    return (
        <PageShell className="overflow-y-auto">
            <PageContainer>
                <PageHeader title={t('endpoints.apiTitle')} />
                <div className="space-y-6 pb-6">
                    <EngineAlertsBanner token={token ?? undefined} />
                    <div className="grid grid-cols-1 gap-4 md:grid-cols-3">
                        <StatCard title={t('endpoints.total')} value={endpoints.length} subtitle={t('endpoints.loadBalanced')} icon={Globe} />
                        <StatCard title={t('endpoints.activeProtocols')} value={flavors.length} subtitle={flavors.length > 0 ? flavors.join(' · ') : '—'} icon={Server} />
                        <StatCard title={t('endpoints.meshHealth')} value={meshHealth} icon={Server} />
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
                    <Card className="gap-0 overflow-hidden py-0">
                        <div className="flex items-baseline justify-between px-5 pt-4">
                            <div className="text-sm font-medium">{t('endpoints.prefixAffinity')}</div>
                            <div className="text-meta-sm text-muted-foreground">{t('endpoints.prefixAffinityDesc')}</div>
                        </div>
                        {affinityFailed ? (
                            <div className="h-16 px-5 pb-4 pt-2 text-meta-sm text-muted-foreground">
                                {t('endpoints.affinityUnavailable')}
                            </div>
                        ) : affinity && affinity.models.length > 0 ? (
                            <Table>
                                <TableHeader>
                                    <TableRow>
                                        <TableHead className="pl-5">{t('endpoints.model')}</TableHead>
                                        <TableHead className="text-right">{t('endpoints.lookups')}</TableHead>
                                        <TableHead className="text-right">{t('endpoints.hits')}</TableHead>
                                        <TableHead className="text-right">{t('endpoints.hitRate')}</TableHead>
                                        <TableHead className="text-right">{t('endpoints.indexEntries')}</TableHead>
                                        <TableHead className="pr-5 text-right">{t('endpoints.fallbacks')}</TableHead>
                                    </TableRow>
                                </TableHeader>
                                <TableBody>
                                    {affinity.models.map((m) => (
                                        <TableRow key={m.model_uid} className="hover:bg-muted/50">
                                            <TableCell className="pl-5 font-medium">{m.model_uid}</TableCell>
                                            <TableCell className="text-right text-sm">{m.lookups}</TableCell>
                                            <TableCell className="text-right text-sm">{m.hits}</TableCell>
                                            <TableCell className="text-right text-sm">{m.hit_rate}%</TableCell>
                                            <TableCell className="text-right text-sm">{m.index_entries}</TableCell>
                                            <TableCell className="pr-5 text-right text-meta-sm text-muted-foreground">
                                                {Object.entries(affinity.fallback)
                                                    .filter(([, v]) => v > 0)
                                                    .map(([k, v]) => `${k} ${v}`)
                                                    .join(' · ') || '—'}
                                            </TableCell>
                                        </TableRow>
                                    ))}
                                </TableBody>
                            </Table>
                        ) : (
                            <div className="h-16 px-5 pb-4 pt-2 text-meta-sm text-muted-foreground">
                                {t('endpoints.noAffinityData')}
                            </div>
                        )}
                    </Card>
                </div>
            </PageContainer>
        </PageShell>
    )
}
