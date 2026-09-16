import { useCallback, useEffect, useMemo, useState } from "react"
import { Cpu, Activity, ArrowUpRight, Server, Globe } from "lucide-react"
import { Link } from "react-router-dom"
import { Badge } from "@/components/ui/badge"
import { Progress } from "@/components/ui/progress"
import {
    Table, TableBody, TableCell, TableHead, TableHeader, TableRow,
} from "@/components/ui/table"
import {
    BarChart, Bar, LineChart, Line, XAxis, YAxis, ResponsiveContainer, CartesianGrid, Tooltip,
} from "recharts"
import { apiGet } from "@/lib/api"
import type { EndpointStats } from "@/lib/types"
import { endpointStatusTone } from "@/lib/endpoint-status"
import { EngineAlertsBanner } from "@/components/engine-alerts-banner"
import { useI18n } from "@/lib/useI18n"
import { useClusterOverview } from "@/hooks/useClusterOverview"
import { useEngineStats } from "@/hooks/useEngineStats"
import { useAuthStore } from "@/store/useAuthStore"
import { Skeleton } from "@/components/ui/skeleton"
import { PageShell } from "@/components/layout/page-shell"
import { PageContainer } from "@/components/layout/page-container"
import { PageHeader } from "@/components/layout/page-header"
import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card"
import { StatCard } from "@/components/ui/stat-card"
import {
    chartGridStroke,
    chartTick,
    chartTooltipStyle,
    chartPrimary,
    chartDestructive,
    chartMuted,
} from "@/lib/chart-theme"

interface MetricPoint {
    timestamp: string
    value: number
}

interface MetricQueryResponse {
    points: MetricPoint[]
}

export function DashboardView() {
    const { t } = useI18n()
    const { token } = useAuthStore()
    const { data: overview, isLoading: overviewLoading } = useClusterOverview()
    const { data: engineStats } = useEngineStats()

    const pct = (used: number, total: number) => total > 0 ? Math.round((used / total) * 100) : 0

    const gpuStats = useMemo(() => {
        if (!overview) return { total: 0, used: 0, count: 0 }
        let total = 0, used = 0, count = 0
        for (const node of overview.nodes) {
            for (const gpu of node.gpus) {
                total += gpu.memory_total_mb
                used += gpu.memory_used_mb
                count++
            }
        }
        return { total, used, count }
    }, [overview])

    const gpuUsagePct = gpuStats.count > 0 ? pct(gpuStats.used, gpuStats.total) : 0
    const hasGpuData = gpuStats.count > 0

    const gpuSummary = useMemo(() => {
        if (!overview) return { avgUtil: 0, maxTemp: 0 }
        let utilSum = 0, utilCount = 0, maxTemp = 0
        for (const node of overview.nodes) {
            for (const gpu of node.gpus) {
                if (gpu.utilization_gpu != null) { utilSum += gpu.utilization_gpu; utilCount++ }
                if (gpu.temperature_c != null && gpu.temperature_c > maxTemp) maxTemp = gpu.temperature_c
            }
        }
        return {
            avgUtil: utilCount > 0 ? Math.round(utilSum / utilCount) : 0,
            maxTemp,
        }
    }, [overview])

    // Alerts from v2 (disk + engine probe) — shared banner component

    // GPU utilization trend data from xtrace
    const [gpuTrend, setGpuTrend] = useState<{ time: string; utilization: number; temperature: number }[]>([])

    const fetchGpuTrend = useCallback(async () => {
        if (!token) return
        try {
            const now = new Date()
            const from = new Date(now.getTime() - 60 * 60 * 1000).toISOString() // 1h ago
            const to = now.toISOString()

            const [utilData, tempData] = await Promise.all([
                apiGet<MetricQueryResponse>(
                    `/observe/metrics/query?name=gpu_utilization&from=${from}&to=${to}&step=60`,
                    token
                ).catch(() => ({ points: [] })),
                apiGet<MetricQueryResponse>(
                    `/observe/metrics/query?name=gpu_temperature&from=${from}&to=${to}&step=60`,
                    token
                ).catch(() => ({ points: [] })),
            ])

            // Merge by timestamp
            const map = new Map<string, { utilization: number; temperature: number; count: number; tempCount: number }>()
            for (const p of utilData.points) {
                const key = p.timestamp.slice(11, 16) // HH:MM
                const existing = map.get(key) || { utilization: 0, temperature: 0, count: 0, tempCount: 0 }
                existing.utilization += p.value
                existing.count += 1
                map.set(key, existing)
            }
            for (const p of tempData.points) {
                const key = p.timestamp.slice(11, 16)
                const existing = map.get(key) || { utilization: 0, temperature: 0, count: 0, tempCount: 0 }
                existing.temperature += p.value
                existing.tempCount += 1
                map.set(key, existing)
            }

            const trend = Array.from(map.entries())
                .sort(([a], [b]) => a.localeCompare(b))
                .map(([time, v]) => ({
                    time,
                    utilization: v.count > 0 ? Math.round(v.utilization / v.count) : 0,
                    temperature: v.tempCount > 0 ? Math.round(v.temperature / v.tempCount) : 0,
                }))

            if (trend.length > 0) setGpuTrend(trend)
        } catch { /* xtrace may not be available */ }
    }, [token])

    useEffect(() => {
        const initialFetch = window.setTimeout(() => { void fetchGpuTrend() }, 0)
        const id = window.setInterval(() => { void fetchGpuTrend() }, 30000)
        return () => {
            window.clearTimeout(initialFetch)
            window.clearInterval(id)
        }
    }, [fetchGpuTrend])

    // Real GPU memory bar chart data
    const gpuBarData = useMemo(() => {
        if (!overview) return []
        const rows: { name: string; memUsed: number; memFree: number }[] = []
        for (const node of overview.nodes) {
            for (const gpu of node.gpus) {
                rows.push({
                    name: `${node.node_id} GPU ${gpu.index}`,
                    memUsed: gpu.memory_used_mb,
                    memFree: gpu.memory_total_mb - gpu.memory_used_mb,
                })
            }
        }
        return rows
    }, [overview])

    // Build stats lookup for endpoint table
    const statsMap = useMemo(() => {
        const m = new Map<string, EndpointStats>()
        if (engineStats) {
            for (const s of engineStats) {
                m.set(`${s.model_uid}-${s.replica_id}`, s)
            }
        }
        return m
    }, [engineStats])

    // Endpoint table rows from real data
    const endpointRows = useMemo(() => {
        if (!overview) return []
        return overview.endpoints.map((ep) => {
            let gpuIndex: number | null = null
            let gpuLabel = "CPU"
            for (const p of overview.placements) {
                if (p.model_uid === ep.model_uid) {
                    const a = p.assignments.find((a) => a.replica_id === ep.replica_id)
                    if (a?.gpu_indices?.length) {
                        gpuLabel = `GPU ${a.gpu_indices.join(",")}`
                        gpuIndex = a.gpu_indices[0] ?? null
                    } else if (a?.gpu_index != null) {
                        gpuIndex = a.gpu_index
                        gpuLabel = `GPU ${a.gpu_index}`
                    }
                    break
                }
            }
            let memUsed = ""
            for (const node of overview.nodes) {
                if (node.node_id === ep.node_id) {
                    const g = gpuIndex != null ? node.gpus.find((g) => g.index === gpuIndex) : null
                    if (g) memUsed = `${g.memory_used_mb.toLocaleString()} MB`
                    break
                }
            }

            const es = statsMap.get(`${ep.model_uid}-${ep.replica_id}`)
            const kvPct =
                typeof es?.kv_cache_usage === "number"
                    ? Math.round(es.kv_cache_usage * 100)
                    : -1

            return {
                key: `${ep.model_uid}-${ep.replica_id}`,
                model: ep.model_uid,
                node: ep.node_id,
                gpu: gpuLabel,
                memUsed: memUsed || "—",
                kvPct,
                pending: es?.pending_requests ?? 0,
                status: ep.status?.toLowerCase() ?? '',
                statusDetail: ep.status_detail ?? null,
                statusTone: endpointStatusTone(ep.status ?? ''),
            }
        })
    }, [overview, statsMap])

    if (overviewLoading && !overview) {
        return (
            <PageShell className="overflow-y-auto">
                <PageContainer>
                    <PageHeader title={t('dashboard.title')} />
                    <div className="grid grid-cols-1 gap-4 md:grid-cols-2 xl:grid-cols-4">
                        <Skeleton className="h-32 w-full rounded-lg" />
                        <Skeleton className="h-32 w-full rounded-lg" />
                        <Skeleton className="h-32 w-full rounded-lg" />
                        <Skeleton className="h-32 w-full rounded-lg" />
                    </div>
                </PageContainer>
            </PageShell>
        )
    }

    return (
        <PageShell className="overflow-y-auto">
            <PageContainer>
                <PageHeader title={t('dashboard.title')} />
                <div className="space-y-6 pb-6">
                    <EngineAlertsBanner token={token ?? undefined} />

                    <div className="grid grid-cols-1 gap-4 md:grid-cols-2 xl:grid-cols-4">
                        <StatCard
                            title={t('dashboard.gpuMemory')}
                            value={`${Math.round(gpuStats.used / 1024)} GB`}
                            subtitle={`/ ${Math.round(gpuStats.total / 1024)} GB · ${gpuUsagePct}%`}
                            icon={Cpu}
                        />
                        <StatCard
                            title={t('dashboard.avgUtilization')}
                            value={hasGpuData ? `${gpuSummary.avgUtil}%` : "—"}
                            subtitle={hasGpuData ? t('dashboard.gpusAcrossNodes', { gpus: gpuStats.count, nodes: overview?.nodes.length || 0 }) : t('dashboard.noGpuData')}
                            icon={Activity}
                        />
                        <StatCard
                            title={t('dashboard.activeEndpoints')}
                            value={overview?.endpoints.length || 0}
                            subtitle={overview?.endpoints.length ? t('dashboard.activeEndpointsCount', { count: overview.endpoints.length }) : t('dashboard.noEndpointsOnline')}
                            icon={Globe}
                        />
                        <StatCard
                            title={t('nav.nodes')}
                            value={overview?.nodes.length || 0}
                            subtitle={overview?.nodes.length ? t('dashboard.allNodesResponding') : t('dashboard.waitingForNodes')}
                            icon={Server}
                        />
                    </div>

                    <div className="grid min-w-0 grid-cols-1 gap-6 lg:grid-cols-3">
                        <Card className="min-w-0 gap-0 border-border py-0 shadow-sm lg:col-span-2">
                            <CardHeader className="flex flex-row items-center justify-between px-5 pb-3 pt-5">
                                <CardTitle>{t('dashboard.gpuTrend')}</CardTitle>
                                <Badge variant="outline">{t('dashboard.live')}</Badge>
                            </CardHeader>
                            <CardContent className="h-[280px] px-5 pb-5">
                                {gpuTrend.length === 0 ? (
                                    <div className="flex h-full items-center justify-center text-sm text-muted-foreground">{t('dashboard.noTrendData')}</div>
                                ) : (
                                    <ResponsiveContainer width="100%" height="100%">
                                        <LineChart data={gpuTrend}>
                                            <CartesianGrid strokeDasharray="3 3" vertical={false} stroke={chartGridStroke} />
                                            <XAxis dataKey="time" axisLine={false} tickLine={false} tick={chartTick} />
                                            <YAxis axisLine={false} tickLine={false} tick={chartTick} />
                                            <Tooltip contentStyle={chartTooltipStyle} />
                                            <Line type="monotone" dataKey="utilization" stroke={chartPrimary} strokeWidth={2} dot={false} name={t('dashboard.utilizationPct')} />
                                            <Line type="monotone" dataKey="temperature" stroke={chartDestructive} strokeWidth={2} dot={false} name={t('dashboard.tempC')} />
                                        </LineChart>
                                    </ResponsiveContainer>
                                )}
                            </CardContent>
                        </Card>
                        <Card className="min-w-0 gap-0 border-border py-0 shadow-sm">
                            <CardHeader className="px-5 pb-3 pt-5">
                                <CardTitle>{t('dashboard.gpuMemoryUsage')}</CardTitle>
                            </CardHeader>
                            <CardContent className="h-[280px] px-5 pb-5">
                                {gpuBarData.length === 0 ? (
                                    <div className="flex h-full items-center justify-center text-sm text-muted-foreground">{t('dashboard.noGpuData')}</div>
                                ) : (
                                    <ResponsiveContainer width="100%" height="100%">
                                        <BarChart data={gpuBarData} layout="vertical">
                                            <XAxis type="number" hide />
                                            <YAxis dataKey="name" type="category" width={80} tick={chartTick} axisLine={false} tickLine={false} />
                                            <Tooltip cursor={{ fill: "transparent" }} contentStyle={chartTooltipStyle} />
                                            <Bar dataKey="memUsed" stackId="a" fill={chartPrimary} name={t('dashboard.used')} />
                                            <Bar dataKey="memFree" stackId="a" fill={chartMuted} name={t('dashboard.free')} />
                                        </BarChart>
                                    </ResponsiveContainer>
                                )}
                            </CardContent>
                        </Card>
                    </div>

                    <Card className="gap-0 overflow-hidden border-border py-0 shadow-sm">
                        <CardHeader className="flex flex-row items-center justify-between px-5 py-4">
                            <CardTitle>{t('dashboard.activeEndpoints')}</CardTitle>
                            <Link to="/endpoints" className="inline-flex items-center gap-1 text-sm text-primary hover:underline">
                                {t('dashboard.viewAll')} <ArrowUpRight className="h-3.5 w-3.5" />
                            </Link>
                        </CardHeader>
                        <CardContent className="px-0 pb-0">
                            <Table>
                                <TableHeader>
                                    <TableRow>
                                        <TableHead className="pl-5">{t('models.model')}</TableHead>
                                        <TableHead>{t('endpoints.nodeGpu')}</TableHead>
                                        <TableHead>{t('dashboard.resource')}</TableHead>
                                        <TableHead>{t('endpoints.vram')}</TableHead>
                                        <TableHead>{t('endpoints.kvCache')}</TableHead>
                                        <TableHead>{t('common.status')}</TableHead>
                                    </TableRow>
                                </TableHeader>
                                <TableBody>
                                    {endpointRows.length === 0 ? (
                                        <TableRow>
                                            <TableCell colSpan={6} className="py-12 text-center text-sm text-muted-foreground">
                                                {t('dashboard.noEndpointsOnline')}
                                            </TableCell>
                                        </TableRow>
                                    ) : (
                                        endpointRows.map((row) => (
                                            <TableRow key={row.key} className="hover:bg-muted/50">
                                                <TableCell className="pl-5 font-medium">{row.model}</TableCell>
                                                <TableCell className="text-muted-foreground">{row.node}</TableCell>
                                                <TableCell><Badge variant="outline">{row.gpu}</Badge></TableCell>
                                                <TableCell className="tabular-nums text-muted-foreground">{row.memUsed}</TableCell>
                                                <TableCell>
                                                    <div className="flex items-center gap-3">
                                                        <Progress value={row.kvPct > 0 ? row.kvPct : 0} className="h-1.5 w-20" />
                                                        <span className="text-meta-sm tabular-nums text-muted-foreground">{row.kvPct >= 0 ? `${row.kvPct}%` : "—"}</span>
                                                    </div>
                                                </TableCell>
                                                <TableCell>
                                                    <Badge variant={row.statusTone === "success" ? "success" : row.statusTone === "destructive" ? "destructive" : "warning"}>
                                                        {row.status || "unknown"}
                                                    </Badge>
                                                    {row.statusDetail ? (
                                                        <p className="mt-1 max-w-[200px] truncate text-meta-sm text-muted-foreground" title={row.statusDetail}>{row.statusDetail}</p>
                                                    ) : null}
                                                </TableCell>
                                            </TableRow>
                                        ))
                                    )}
                                </TableBody>
                            </Table>
                        </CardContent>
                    </Card>
                </div>
            </PageContainer>
        </PageShell>
    )
}
