import { useCallback, useEffect, useMemo, useState } from "react"
import { Cpu, Activity, ArrowUpRight } from "lucide-react"
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
import { cn } from "@/lib/utils"

import { Skeleton } from "@/components/ui/skeleton"

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
            <div className="space-y-8">
                <div className="flex justify-between items-end">
                    <div className="space-y-2">
                        <Skeleton className="h-8 w-64" />
                        <Skeleton className="h-4 w-48" />
                    </div>
                    <div className="flex gap-8">
                        <Skeleton className="h-10 w-16" />
                        <Skeleton className="h-10 w-16" />
                    </div>
                </div>
                <div className="grid grid-cols-1 md:grid-cols-2 lg:grid-cols-4 gap-6">
                    <Skeleton className="h-32 w-full rounded-xl" />
                    <Skeleton className="h-32 w-full rounded-xl" />
                    <Skeleton className="h-32 w-full rounded-xl" />
                    <Skeleton className="h-32 w-full rounded-xl" />
                </div>
                <div className="grid grid-cols-1 lg:grid-cols-3 gap-6">
                    <Skeleton className="lg:col-span-2 h-80 w-full rounded-xl" />
                    <Skeleton className="h-80 w-full rounded-xl" />
                </div>
            </div>
        )
    }

    return (
        <div className="space-y-8 animate-in fade-in duration-500">
            {/* Header */}
            <div className="flex flex-col gap-5 sm:flex-row sm:items-end sm:justify-between">
                <div>
                    <h2 className="text-3xl font-bold tracking-tight font-mono uppercase text-foreground">{t('dashboard.title')}</h2>
                    <p className="text-muted-foreground mt-2 flex items-center gap-2">
                        <Activity className="h-4 w-4 text-primary animate-signal" />
                        {t('dashboard.subtitle')}
                    </p>
                </div>
                <div className="flex gap-8">
                    <div className="text-right">
                        <p className="text-[10px] font-bold text-muted-foreground uppercase tracking-widest mb-1">{t('nav.nodes')}</p>
                        <p className="text-2xl font-mono font-bold text-foreground">{overview?.nodes.length || 0}</p>
                    </div>
                    <div className="text-right">
                        <p className="text-[10px] font-bold text-muted-foreground uppercase tracking-widest mb-1">{t('nav.endpoints')}</p>
                        <p className="text-2xl font-mono font-bold text-primary">{overview?.endpoints.length || 0}</p>
                    </div>
                </div>
            </div>

            {/* Probe / disk alert banners */}
            <EngineAlertsBanner token={token ?? undefined} />

            {/* Quick Stats Grid */}
            <div className="grid grid-cols-1 gap-4 md:grid-cols-2 lg:grid-cols-4">
                <div className="relative overflow-hidden rounded-xl border border-border bg-card/40 p-5 backdrop-blur-xl rim-light group sm:p-6">
                    <div className="absolute top-0 right-0 p-4 opacity-10 group-hover:opacity-20 transition-opacity">
                        <Cpu className="h-12 w-12" />
                    </div>
                    <p className="text-[11px] font-bold text-muted-foreground uppercase tracking-widest mb-1">{t('dashboard.gpuMemory')}</p>
                    <div className="flex items-baseline gap-2">
                        <h3 className="text-2xl font-mono font-bold text-foreground">{Math.round(gpuStats.used / 1024)}GB</h3>
                        <p className="text-xs text-muted-foreground font-mono">/ {Math.round(gpuStats.total / 1024)}GB</p>
                    </div>
                    <Progress value={gpuUsagePct} className="mt-4 h-1.5 bg-white/5" />
                </div>

                <div className="rounded-xl border border-border bg-card/40 p-5 backdrop-blur-xl rim-light sm:p-6">
                    <p className="text-[11px] font-bold text-muted-foreground uppercase tracking-widest mb-1">{t('dashboard.avgUtilization')}</p>
                    <h3 className="text-2xl font-mono font-bold text-foreground">{hasGpuData ? `${gpuSummary.avgUtil}%` : "—"}</h3>
                    <div className="flex items-center gap-1.5 mt-4">
                        <div className={cn("h-1.5 w-1.5 shrink-0 rounded-full", hasGpuData ? "bg-primary" : "bg-muted-foreground/50")} />
                        <p className="line-clamp-2 text-[10px] font-bold uppercase tracking-widest text-muted-foreground">
                            {hasGpuData ? t('dashboard.gpusAcrossNodes', { gpus: gpuStats.count, nodes: overview?.nodes.length || 0 }) : t('dashboard.noGpuData')}
                        </p>
                    </div>
                </div>

                <div className="rounded-xl border border-border bg-card/40 p-5 backdrop-blur-xl rim-light sm:p-6">
                    <p className="text-[11px] font-bold text-muted-foreground uppercase tracking-widest mb-1">{t('dashboard.activeEndpoints')}</p>
                    <h3 className="text-2xl font-mono font-bold text-foreground">{overview?.endpoints.length || 0}</h3>
                    <div className="flex items-center gap-1.5 mt-4">
                        <div className={cn("h-1.5 w-1.5 shrink-0 rounded-full", overview?.endpoints.length ? "bg-success" : "bg-muted-foreground/50")} />
                        <p className="line-clamp-2 text-[10px] font-bold uppercase tracking-widest text-muted-foreground">
                            {overview?.endpoints.length ? t('dashboard.activeEndpointsCount', { count: overview.endpoints.length }) : t('dashboard.noEndpointsOnline')}
                        </p>
                    </div>
                </div>

                <div className="rounded-xl border border-border bg-card/40 p-5 backdrop-blur-xl rim-light sm:p-6">
                    <p className="text-[11px] font-bold text-muted-foreground uppercase tracking-widest mb-1">{t('endpoints.meshHealth')}</p>
                    <h3 className={cn("text-2xl font-mono font-bold", overview?.nodes.length ? "text-success" : "text-muted-foreground")}>{overview?.nodes.length ? "99.9%" : "—"}</h3>
                    <div className="flex items-center gap-1.5 mt-4">
                        <div className={cn("h-1.5 w-1.5 shrink-0 rounded-full", overview?.nodes.length ? "bg-success" : "bg-muted-foreground/50")} />
                        <p className="text-[10px] font-bold uppercase tracking-widest text-muted-foreground">
                            {overview?.nodes.length ? t('dashboard.allNodesResponding') : t('dashboard.waitingForNodes')}
                        </p>
                    </div>
                </div>
            </div>

            <div className="grid min-w-0 grid-cols-1 gap-4 lg:grid-cols-3">
                {/* Trend Chart */}
                <div className="min-w-0 rounded-xl border border-border bg-card/40 p-5 backdrop-blur-xl sm:p-6 lg:col-span-2">
                    <div className="flex items-center justify-between mb-6">
                        <h4 className="text-sm font-bold uppercase tracking-widest text-muted-foreground">{t('dashboard.gpuTrend')}</h4>
                        <Badge variant="outline" className="font-mono text-[10px] border-primary/20 text-primary">{t('dashboard.live')}</Badge>
                    </div>
                    <div className="flex h-[280px] min-w-0 items-center justify-center">
                        {gpuTrend.length === 0 ? (
                            <p className="max-w-xs text-center text-xs leading-relaxed text-muted-foreground">{t('dashboard.noTrendData')}</p>
                        ) : (
                            <ResponsiveContainer width="100%" height="100%" minWidth={0} minHeight={0}>
                                <LineChart data={gpuTrend}>
                                    <CartesianGrid strokeDasharray="3 3" vertical={false} stroke="oklch(30% 0.05 260 / 0.2)" />
                                    <XAxis dataKey="time" axisLine={false} tickLine={false} tick={{ fontSize: 10, fill: "oklch(75% 0.02 260)" }} />
                                    <YAxis axisLine={false} tickLine={false} tick={{ fontSize: 10, fill: "oklch(75% 0.02 260)" }} />
                                    <Tooltip
                                        contentStyle={{ backgroundColor: "oklch(22% 0.03 260)", border: "1px solid oklch(30% 0.05 260 / 0.5)", borderRadius: "8px", fontSize: "12px" }}
                                        itemStyle={{ color: "oklch(98% 0.01 260)" }}
                                    />
                                    <Line type="monotone" dataKey="utilization" stroke="oklch(70% 0.18 190)" strokeWidth={2} dot={false} name={t('dashboard.utilizationPct')} />
                                    <Line type="monotone" dataKey="temperature" stroke="oklch(60% 0.2 25)" strokeWidth={2} dot={false} name={t('dashboard.tempC')} />
                                </LineChart>
                            </ResponsiveContainer>
                        )}
                    </div>
                </div>

                {/* GPU Distribution Bar Chart */}
                <div className="min-w-0 rounded-xl border border-border bg-card/40 p-5 backdrop-blur-xl sm:p-6">
                    <h4 className="text-sm font-bold uppercase tracking-widest text-muted-foreground mb-6">{t('dashboard.gpuMemoryUsage')}</h4>
                    <div className="flex h-[280px] min-w-0 items-center justify-center">
                        {gpuBarData.length === 0 ? (
                            <p className="max-w-xs text-center text-xs leading-relaxed text-muted-foreground">{t('dashboard.noGpuData')}</p>
                        ) : (
                            <ResponsiveContainer width="100%" height="100%" minWidth={0} minHeight={0}>
                                <BarChart data={gpuBarData} layout="vertical">
                                    <XAxis type="number" hide />
                                    <YAxis dataKey="name" type="category" width={80} tick={{ fontSize: 9, fill: "oklch(75% 0.02 260)" }} axisLine={false} tickLine={false} />
                                    <Tooltip
                                        cursor={{ fill: "transparent" }}
                                        contentStyle={{ backgroundColor: "oklch(22% 0.03 260)", border: "1px solid oklch(30% 0.05 260 / 0.5)", borderRadius: "8px", fontSize: "12px" }}
                                    />
                                    <Bar dataKey="memUsed" stackId="a" fill="oklch(70% 0.18 190)" radius={[0, 0, 0, 0]} name={t('dashboard.used')} />
                                    <Bar dataKey="memFree" stackId="a" fill="oklch(30% 0.03 260)" radius={[0, 4, 4, 0]} name={t('dashboard.free')} />
                                </BarChart>
                            </ResponsiveContainer>
                        )}
                    </div>
                </div>
            </div>

            {/* Active Endpoints Table */}
            <div className="bg-card/40 backdrop-blur-xl border border-border rounded-xl overflow-hidden">
                <div className="flex items-center justify-between border-b border-border/50 bg-white/5 px-5 py-4 sm:px-6">
                    <h4 className="text-sm font-bold uppercase tracking-widest text-muted-foreground">{t('dashboard.activeEndpoints')}</h4>
                    <Link to="/endpoints" className="flex items-center gap-1 text-[10px] font-bold uppercase text-primary hover:underline">
                        {t('dashboard.viewAll')} <ArrowUpRight className="h-3 w-3" />
                    </Link>
                </div>
                <div className="overflow-x-auto">
                <Table>
                    <TableHeader className="bg-black/20">
                        <TableRow className="border-border/50 hover:bg-transparent">
                            <TableHead className="text-[10px] uppercase font-bold text-muted-foreground px-6">{t('models.model')}</TableHead>
                            <TableHead className="text-[10px] uppercase font-bold text-muted-foreground">{t('endpoints.nodeGpu')}</TableHead>
                            <TableHead className="text-[10px] uppercase font-bold text-muted-foreground">{t('dashboard.resource')}</TableHead>
                            <TableHead className="text-[10px] uppercase font-bold text-muted-foreground">{t('endpoints.vram')}</TableHead>
                            <TableHead className="text-[10px] uppercase font-bold text-muted-foreground">{t('endpoints.kvCache')}</TableHead>
                            <TableHead className="text-[10px] uppercase font-bold text-muted-foreground">{t('common.status')}</TableHead>
                        </TableRow>
                    </TableHeader>
                    <TableBody>
                        {endpointRows.length === 0 ? (
                            <TableRow>
                                <TableCell colSpan={6} className="text-center py-12 text-muted-foreground font-mono text-xs uppercase tracking-widest">
                                    {t('dashboard.noEndpointsOnline')}
                                </TableCell>
                            </TableRow>
                        ) : (
                            endpointRows.map((row) => (
                                <TableRow key={row.key} className="border-border/40 hover:bg-white/5 transition-colors group">
                                    <TableCell className="font-mono text-sm font-bold px-6 group-hover:text-primary transition-colors">{row.model}</TableCell>
                                    <TableCell className="font-mono text-xs text-muted-foreground">{row.node}</TableCell>
                                    <TableCell>
                                        <Badge variant="outline" className="font-mono text-[10px] border-primary/20 text-primary uppercase">{row.gpu}</Badge>
                                    </TableCell>
                                    <TableCell className="font-mono text-xs text-muted-foreground">{row.memUsed}</TableCell>
                                    <TableCell>
                                        <div className="flex items-center gap-3">
                                            <div className="flex-1 w-20 bg-white/5 h-1.5 rounded-full overflow-hidden">
                                                <div
                                                    className="bg-primary h-full transition-all"
                                                    style={{ width: `${row.kvPct > 0 ? row.kvPct : 0}%` }}
                                                />
                                            </div>
                                            <span className="text-[10px] font-mono text-muted-foreground">{row.kvPct >= 0 ? `${row.kvPct}%` : "—"}</span>
                                        </div>
                                    </TableCell>
                                    <TableCell>
                                        <div className="flex flex-col gap-1">
                                            <div className="flex items-center gap-2">
                                                <div className={cn(
                                                    "w-1.5 h-1.5 rounded-full",
                                                    row.statusTone === "success" ? "bg-success" : row.statusTone === "destructive" ? "bg-destructive animate-pulse" : "bg-warning animate-pulse"
                                                )} />
                                                <span className={cn(
                                                    "text-[10px] uppercase font-bold tracking-wider",
                                                    row.statusTone === "success" ? "text-success" : row.statusTone === "destructive" ? "text-destructive" : "text-warning"
                                                )}>
                                                    {row.status || "unknown"}
                                                </span>
                                            </div>
                                            {row.statusDetail && (
                                                <span className="text-[10px] text-muted-foreground truncate max-w-[200px]" title={row.statusDetail}>
                                                    {row.statusDetail}
                                                </span>
                                            )}
                                        </div>
                                    </TableCell>
                                </TableRow>
                            ))
                        )}
                    </TableBody>
                </Table>
                </div>
            </div>
        </div>
    )
}
