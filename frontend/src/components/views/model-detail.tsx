import { useCallback, useEffect, useState } from "react"
import {
    ArrowLeft, Play, Square, Trash2, Loader2, Server, HardDrive,
    Activity, RefreshCw, ScaleIcon,
} from "lucide-react"
import { Badge } from "@/components/ui/badge"
import { Button } from "@/components/ui/button"
import { Progress } from "@/components/ui/progress"
import {
    Table, TableBody, TableCell, TableHead, TableHeader, TableRow,
} from "@/components/ui/table"
import { cn } from "@/lib/utils"
import { v2 } from "@/lib/api"
import type { ModelDetailView, AggregatedModelState } from "@/lib/types"
import { useI18n } from "@/lib/i18n"

const STATE_BADGE: Record<AggregatedModelState, { key: string; cls: string }> = {
    running: { key: "state.running", cls: "bg-success/10 text-success border-success/20" },
    stopped: { key: "state.stopped", cls: "bg-muted text-muted-foreground border-border" },
    downloading: { key: "state.downloading", cls: "bg-blue-500/10 text-blue-600 border-blue-500/20 animate-pulse" },
    starting: { key: "state.starting", cls: "bg-yellow-500/10 text-yellow-600 border-yellow-500/20 animate-pulse" },
    degraded: { key: "state.degraded", cls: "bg-orange-500/10 text-orange-600 border-orange-500/20" },
    failed: { key: "state.failed", cls: "bg-destructive/10 text-destructive border-destructive/20" },
    stopping: { key: "state.stopping", cls: "bg-muted text-muted-foreground border-border animate-pulse" },
}

interface ModelDetailProps {
    modelUid: string
    token: string
    onBack: () => void
}

export function ModelDetailView_Page({ modelUid, token, onBack }: ModelDetailProps) {
    const { t } = useI18n()
    const [detail, setDetail] = useState<ModelDetailView | null>(null)
    const [loading, setLoading] = useState(true)
    const [error, setError] = useState<string | null>(null)
    const [acting, setActing] = useState(false)

    const refresh = useCallback(async () => {
        try {
            const d = await v2.getModel(modelUid, token)
            setDetail(d)
            setError(null)
        } catch (err) {
            setError(err instanceof Error ? err.message : t('modelDetail.failedLoad'))
        } finally {
            setLoading(false)
        }
    }, [modelUid, token, t])

    useEffect(() => { refresh() }, [refresh])
    useEffect(() => {
        const id = setInterval(refresh, 8000)
        return () => clearInterval(id)
    }, [refresh])

    const act = async (fn: () => Promise<unknown>) => {
        setActing(true)
        try { await fn(); await refresh() } catch (err) {
            setError(err instanceof Error ? err.message : t('modelDetail.actionFailed'))
        } finally { setActing(false) }
    }

    if (loading) return (
        <div className="flex items-center justify-center py-24 text-muted-foreground">
            <Loader2 className="h-5 w-5 animate-spin mr-2" /> {t('modelDetail.loading')}
        </div>
    )
    if (error && !detail) return (
        <div className="space-y-4">
            <Button variant="ghost" size="sm" onClick={onBack}><ArrowLeft className="h-4 w-4 mr-1" />{t('common.back')}</Button>
            <p className="text-destructive text-sm">{error}</p>
        </div>
    )
    if (!detail) return null

    const st = STATE_BADGE[detail.state]
    const fmtTime = (ms: number) => ms ? new Date(ms).toLocaleString() : t('common.n_a')
    const fmtBytes = (b: number) => {
        if (b >= 1e9) return `${(b / 1e9).toFixed(1)} GB`
        if (b >= 1e6) return `${(b / 1e6).toFixed(1)} MB`
        return `${b} B`
    }

    return (
        <div className="space-y-6 animate-in fade-in slide-in-from-bottom-4 duration-700">
            {/* Header */}
            <div className="flex items-center justify-between">
                <div className="flex items-center gap-3">
                    <Button variant="ghost" size="sm" onClick={onBack} className="rounded-xl">
                        <ArrowLeft className="h-4 w-4 mr-1" /> {t('common.back')}
                    </Button>
                    <div>
                        <h2 className="text-2xl font-bold text-foreground">{detail.model_uid}</h2>
                        <p className="text-sm text-muted-foreground font-mono">{detail.model_name}</p>
                    </div>
                    <Badge className={cn("ml-2", st.cls)}>{t(st.key)}</Badge>
                </div>
                <div className="flex items-center gap-2">
                    <Button variant="outline" size="sm" className="rounded-xl" onClick={refresh} disabled={acting}>
                        <RefreshCw className="h-4 w-4" />
                    </Button>
                    {(detail.state === "stopped" || detail.state === "failed") && (
                        <Button size="sm" className="rounded-xl" onClick={() => act(() => v2.startModel(modelUid, {}, token))} disabled={acting}>
                            <Play className="h-4 w-4 mr-1" /> {t('models.start')}
                        </Button>
                    )}
                    {detail.state === "running" && (
                        <Button variant="outline" size="sm" className="rounded-xl" onClick={() => act(() => v2.stopModel(modelUid, token))} disabled={acting}>
                            <Square className="h-4 w-4 mr-1" /> {t('models.stopService')}
                        </Button>
                    )}
                    {(detail.state === "stopped" || detail.state === "failed") && (
                        <Button variant="destructive" size="sm" className="rounded-xl" onClick={() => act(() => v2.deleteModel(modelUid, token))} disabled={acting}>
                            <Trash2 className="h-4 w-4 mr-1" /> {t('common.delete')}
                        </Button>
                    )}
                </div>
            </div>

            {error && <p className="text-destructive text-sm bg-destructive/5 rounded-xl px-4 py-2">{error}</p>}

            {/* Info cards row */}
            <div className="grid grid-cols-4 gap-4">
                <InfoCard label={t('modelDetail.replicas')} value={`${detail.replicas.ready} / ${detail.replicas.desired}`} sub={detail.replicas.unhealthy > 0 ? `${detail.replicas.unhealthy} ${t('models.unhealthy')}` : undefined} />
                <InfoCard label={t('modelDetail.engine')} value={detail.engine_type ?? "default"} />
                <InfoCard label={t('modelDetail.created')} value={fmtTime(detail.created_at_ms)} />
                <InfoCard label={t('modelDetail.updated')} value={fmtTime(detail.updated_at_ms)} />
            </div>

            {/* Download progress */}
            {detail.download_progress && detail.download_progress.replicas.length > 0 && (
                <Section title={t('modelDetail.downloadProgress')} icon={<Loader2 className="h-4 w-4 animate-spin" />}>
                    <div className="space-y-3">
                        {detail.download_progress.replicas.map((dp) => {
                            const pct = dp.total_bytes > 0 ? Math.round((dp.downloaded_bytes / dp.total_bytes) * 100) : 0
                            return (
                                <div key={`${dp.replica_id}-${dp.node_id}`} className="space-y-1">
                                    <div className="flex items-center justify-between text-xs">
                                        <span className="font-medium">Replica {dp.replica_id} on {dp.node_id}</span>
                                        <span className="text-muted-foreground">{fmtBytes(dp.downloaded_bytes)} / {fmtBytes(dp.total_bytes)} ({pct}%) · {dp.files_done}/{dp.files_total} files</span>
                                    </div>
                                    <Progress value={pct} className="h-2" />
                                </div>
                            )
                        })}
                    </div>
                </Section>
            )}

            {/* Deployment info */}
            {detail.deployment && (
                <Section title={t('modelDetail.deployment')} icon={<ScaleIcon className="h-4 w-4" />}>
                    <div className="grid grid-cols-3 gap-4 text-sm">
                        <div><span className="text-muted-foreground">{t('modelDetail.desiredState')}:</span> <span className="font-medium">{detail.deployment.desired_state}</span></div>
                        <div><span className="text-muted-foreground">{t('modelDetail.replicas')}:</span> <span className="font-medium">{detail.deployment.replicas}</span></div>
                        <div><span className="text-muted-foreground">{t('modelDetail.version')}:</span> <span className="font-medium">{detail.deployment.version}</span></div>
                        {detail.deployment.node_affinity && <div><span className="text-muted-foreground">{t('modelDetail.nodeAffinity')}:</span> <span className="font-medium">{detail.deployment.node_affinity}</span></div>}
                        {detail.deployment.gpu_affinity && <div><span className="text-muted-foreground">{t('modelDetail.gpuAffinity')}:</span> <span className="font-medium">{detail.deployment.gpu_affinity.join(", ")}</span></div>}
                    </div>
                </Section>
            )}

            {/* Endpoints */}
            {detail.endpoints.length > 0 && (
                <Section title={t('modelDetail.endpoints')} icon={<Activity className="h-4 w-4" />}>
                    <Table>
                        <TableHeader>
                            <TableRow className="hover:bg-transparent">
                                <TableHead className="text-[11px] font-bold uppercase">{t('endpoints.replica')}</TableHead>
                                <TableHead className="text-[11px] font-bold uppercase">{t('endpoints.nodeGpu')}</TableHead>
                                <TableHead className="text-[11px] font-bold uppercase">{t('modelDetail.kind')}</TableHead>
                                <TableHead className="text-[11px] font-bold uppercase">{t('common.status')}</TableHead>
                                <TableHead className="text-[11px] font-bold uppercase">{t('modelDetail.baseUrl')}</TableHead>
                            </TableRow>
                        </TableHeader>
                        <TableBody>
                            {detail.endpoints.map((ep) => (
                                <TableRow key={`${ep.model_uid}-${ep.replica_id}`}>
                                    <TableCell className="font-mono text-xs">{ep.replica_id}</TableCell>
                                    <TableCell className="font-mono text-xs">{ep.node_id}</TableCell>
                                    <TableCell className="text-xs">{ep.endpoint_kind}</TableCell>
                                    <TableCell><Badge variant="outline" className="text-[10px]">{ep.status}</Badge></TableCell>
                                    <TableCell className="font-mono text-[10px] text-muted-foreground truncate max-w-[240px]">{ep.base_url || "—"}</TableCell>
                                </TableRow>
                            ))}
                        </TableBody>
                    </Table>
                </Section>
            )}

            {(detail.capabilities?.length ?? 0) > 0 && (
                <Section title={t('modelDetail.capabilities')} icon={<HardDrive className="h-4 w-4" />}>
                    <div className="space-y-3">
                        {detail.capabilities.map((rc) => (
                            <div key={rc.replica_id} className="grid grid-cols-2 md:grid-cols-4 gap-3 text-sm border border-border/40 rounded-xl p-3">
                                <div>
                                    <p className="text-[10px] uppercase text-muted-foreground tracking-widest">Replica</p>
                                    <p className="font-mono font-bold">{rc.replica_id}</p>
                                </div>
                                <div>
                                    <p className="text-[10px] uppercase text-muted-foreground tracking-widest">Source</p>
                                    <p className="font-mono">{rc.capability.source}</p>
                                </div>
                                <div>
                                    <p className="text-[10px] uppercase text-muted-foreground tracking-widest">Engine</p>
                                    <p className="font-mono">{rc.capability.engine_type}{rc.capability.engine_version ? ` @ ${rc.capability.engine_version}` : ""}</p>
                                </div>
                                <div>
                                    <p className="text-[10px] uppercase text-muted-foreground tracking-widest">Obs</p>
                                    <p className="font-mono text-xs">
                                        kv={rc.capability.observability?.kv_cache_usage ?? "n/a"} ·
                                        pending={rc.capability.observability?.pending_requests ?? "n/a"}
                                    </p>
                                </div>
                            </div>
                        ))}
                    </div>
                </Section>
            )}

            )}

            {/* Cache status */}
            {detail.cache_status && (
                <Section title={t('modelDetail.cacheStatus')} icon={<HardDrive className="h-4 w-4" />}>
                    <div className="flex items-center gap-6 text-sm">
                        <div><span className="text-muted-foreground">{t('modelDetail.totalSize')}:</span> <span className="font-medium">{fmtBytes(detail.cache_status.total_size_bytes)}</span></div>
                        <div className="flex items-center gap-2">
                            <span className="text-muted-foreground">{t('modelDetail.cachedOn')}:</span>
                            {detail.cache_status.cached_on_nodes.length > 0
                                ? detail.cache_status.cached_on_nodes.map((n) => (
                                    <Badge key={n} variant="secondary" className="text-[10px]"><Server className="h-3 w-3 mr-1" />{n}</Badge>
                                ))
                                : <span className="text-muted-foreground">{t('modelDetail.noNodes')}</span>
                            }
                        </div>
                    </div>
                </Section>
            )}

            {/* Labels */}
            {Object.keys(detail.labels).length > 0 && (
                <Section title={t('modelDetail.labels')}>
                    <div className="flex flex-wrap gap-2">
                        {Object.entries(detail.labels).map(([k, val]) => (
                            <Badge key={k} variant="outline" className="text-xs font-mono">{k}={val}</Badge>
                        ))}
                    </div>
                </Section>
            )}
        </div>
    )
}

// ---------------------------------------------------------------------------
// Helper components
// ---------------------------------------------------------------------------

function InfoCard({ label, value, sub }: { label: string; value: string; sub?: string }) {
    return (
        <div className="bg-card border border-border rounded-2xl p-4">
            <p className="text-xs text-muted-foreground mb-1">{label}</p>
            <p className="text-lg font-bold text-foreground">{value}</p>
            {sub && <p className="text-[10px] text-destructive mt-0.5">{sub}</p>}
        </div>
    )
}

function Section({ title, icon, children }: { title: string; icon?: React.ReactNode; children: React.ReactNode }) {
    return (
        <div className="bg-card border border-border rounded-2xl overflow-hidden">
            <div className="px-6 py-4 border-b border-border bg-accent/30 flex items-center gap-2">
                {icon}
                <h3 className="text-sm font-bold text-foreground">{title}</h3>
            </div>
            <div className="p-6">{children}</div>
        </div>
    )
}

