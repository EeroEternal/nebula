import { Cpu, Server } from "lucide-react"
import { Badge } from "@/components/ui/badge"
import { Progress } from "@/components/ui/progress"
import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card"
import { StatCard } from "@/components/ui/stat-card"
import { PageShell } from "@/components/layout/page-shell"
import { PageContainer } from "@/components/layout/page-container"
import { PageHeader } from "@/components/layout/page-header"
import { cn } from "@/lib/utils"
import { useI18n } from "@/lib/useI18n"
import { useClusterOverview } from "@/hooks/useClusterOverview"

export function NodesView() {
    const { t } = useI18n()
    const { data: overview, isLoading } = useClusterOverview()

    const fmtTime = (ms: number) => {
        if (ms < 1000) return `${ms}ms`
        if (ms < 60000) return `${Math.round(ms / 1000)}s`
        return `${Math.round(ms / 60000)}m`
    }

    const getGpuModel = (nodeId: string, gpuIdx: number) => {
        if (!overview) return null
        for (const p of overview.placements) {
            for (const a of p.assignments) {
                if (a.node_id === nodeId && a.gpu_index === gpuIdx) return p.model_uid
            }
        }
        return null
    }

    const gpuCount = overview?.nodes.reduce((acc, n) => acc + n.gpus.length, 0) ?? 0

    return (
        <PageShell className="overflow-y-auto">
            <PageContainer>
                <PageHeader title={t('nodes.title')} />
                {isLoading && !overview ? (
                    <p className="text-sm text-muted-foreground">{t('nodes.scanning')}</p>
                ) : !overview || overview.nodes.length === 0 ? (
                    <Card className="p-12 text-center">
                        <Server className="mx-auto mb-3 h-8 w-8 text-muted-foreground/40" />
                        <p className="text-sm text-muted-foreground">{t('nodes.emptyDesc')}</p>
                    </Card>
                ) : (
                    <div className="space-y-6 pb-6">
                        <div className="grid grid-cols-1 gap-4 md:grid-cols-2">
                            <StatCard title={t('nav.nodes')} value={overview.nodes.length} icon={Server} />
                            <StatCard title={t('nodes.totalGpuPower')} value={`${gpuCount} ${t('nodes.units')}`} icon={Cpu} />
                        </div>
                        {overview.nodes.map((node) => (
                            <Card key={node.node_id} className="gap-0 border-border py-0 shadow-sm">
                                <CardHeader className="flex flex-row items-center justify-between px-5 py-4">
                                    <div>
                                        <CardTitle>{node.node_id}</CardTitle>
                                        <p className="mt-1 text-meta-sm text-muted-foreground">
                                            {t('nodes.platform')}: {(node as { platform?: string }).platform || "nvidia-cuda"}
                                            {" · "}
                                            {t('nodes.heartbeatAgo', { time: fmtTime(node.last_heartbeat_ms) })}
                                            {" · "}
                                            {t('nodes.gpusDetected', { count: node.gpus.length })}
                                        </p>
                                    </div>
                                    <Badge variant="success">{t('nodes.operational')}</Badge>
                                </CardHeader>
                                <CardContent className="grid gap-4 px-5 pb-5 sm:grid-cols-2 lg:grid-cols-4">
                                    {node.gpus.map((gpu) => {
                                        const modelUid = getGpuModel(node.node_id, gpu.index)
                                        const usage = gpu.memory_total_mb > 0 ? Math.round((gpu.memory_used_mb / gpu.memory_total_mb) * 100) : 0
                                        return (
                                            <div key={gpu.index} className="rounded-lg border border-border p-3">
                                                <div className="mb-2 flex items-center justify-between">
                                                    <span className="text-sm font-medium">{t('nodes.gpu')} {gpu.index}</span>
                                                    <span className={cn("text-sm tabular-nums", usage > 80 ? "text-destructive" : "text-foreground")}>{usage}%</span>
                                                </div>
                                                <p className="mb-2 truncate text-meta-sm text-muted-foreground">
                                                    {(gpu as { name?: string }).name || t('nodes.gpu')}
                                                </p>
                                                <Progress value={usage} className="h-1.5" indicatorClassName={usage > 85 ? "bg-destructive" : undefined} />
                                                <div className="mt-3 grid grid-cols-2 gap-2 text-meta-sm text-muted-foreground">
                                                    <span>{gpu.temperature_c != null ? `${gpu.temperature_c}°C` : "—"}</span>
                                                    <span className="text-right">{gpu.utilization_gpu != null ? `${gpu.utilization_gpu}%` : "—"}</span>
                                                </div>
                                                <div className="mt-2">
                                                    {modelUid ? <Badge variant="outline">{modelUid}</Badge> : (
                                                        <span className="text-meta-sm text-success">{t('nodes.idleReady')}</span>
                                                    )}
                                                </div>
                                            </div>
                                        )
                                    })}
                                </CardContent>
                            </Card>
                        ))}
                    </div>
                )}
            </PageContainer>
        </PageShell>
    )
}
