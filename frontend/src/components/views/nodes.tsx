import { Cpu, Server, Activity, Thermometer, Gauge, ShieldCheck, Zap } from "lucide-react"
import { Badge } from "@/components/ui/badge"
import { Progress } from "@/components/ui/progress"
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
                if (a.node_id === nodeId && a.gpu_index === gpuIdx) {
                    return p.model_uid
                }
            }
        }
        return null
    }

    if (isLoading && !overview) {
        return (
            <div className="h-64 flex flex-col items-center justify-center gap-4 text-muted-foreground">
                <Server className="h-8 w-8 animate-pulse text-primary" />
                 <p className="text-[10px] font-mono uppercase tracking-widest">{t('nodes.scanning')}</p>
            </div>
        )
    }

    if (!overview || overview.nodes.length === 0) {
        return (
            <div className="bg-card/40 backdrop-blur-xl border border-border rounded-2xl p-20 flex flex-col items-center justify-center text-center">
                <Server className="h-16 w-16 text-muted-foreground/20 mb-4" />
                <h3 className="text-xl font-bold font-mono uppercase tracking-tight text-foreground">{t('nodes.emptyTitle')}</h3>
                <p className="text-sm text-muted-foreground mt-2 max-w-sm">
                    {t('nodes.emptyDesc')}
                </p>
            </div>
        )
    }

    return (
        <div className="space-y-10 animate-in fade-in duration-500">
            <div className="flex justify-between items-end">
                <div>
                    <h2 className="text-3xl font-bold tracking-tight font-mono uppercase text-foreground">{t('nodes.title')}</h2>
                    <p className="text-muted-foreground mt-2 flex items-center gap-2">
                        <ShieldCheck className="h-4 w-4 text-success" />
                        {t('nodes.subtitle')}
                    </p>
                </div>
                <div className="text-right">
                     <p className="text-[10px] font-bold text-muted-foreground uppercase tracking-widest mb-1">{t('nodes.totalGpuPower')}</p>
                    <p className="text-2xl font-mono font-bold text-foreground">
                         {overview.nodes.reduce((acc, n) => acc + n.gpus.length, 0)} {t('nodes.units')}
                    </p>
                </div>
            </div>

            <div className="space-y-12">
                {overview.nodes.map((node) => (
                    <div key={node.node_id} className="space-y-6">
                        {/* Node Header */}
                        <div className="flex items-center justify-between bg-white/5 px-6 py-4 rounded-xl border border-border/50 backdrop-blur-md">
                            <div className="flex items-center gap-4">
                                <div className="h-10 w-10 rounded-lg bg-primary text-primary-foreground flex items-center justify-center rim-light">
                                    <Server className="h-5 w-5" />
                                </div>
                                <div>
                                    <h3 className="text-lg font-bold font-mono text-foreground tracking-tight">{node.node_id}</h3>
                                    <div className="flex items-center gap-3 mt-0.5">
                                        <p className="text-[10px] font-bold text-muted-foreground/60 uppercase tracking-widest">
                                             {t('nodes.platform')}: {(node as { platform?: string }).platform || 'nvidia-cuda'}
                                        </p>
                                        <div className="w-1 h-1 rounded-full bg-muted-foreground/30" />
                                        <p className="text-[10px] font-bold text-muted-foreground/60 uppercase tracking-widest">
                                             {t('nodes.heartbeatAgo', { time: fmtTime(node.last_heartbeat_ms) })}
                                        </p>
                                        <div className="w-1 h-1 rounded-full bg-muted-foreground/30" />
                                        <p className="text-[10px] font-bold text-muted-foreground/60 uppercase tracking-widest">
                                             {t('nodes.gpusDetected', { count: node.gpus.length })}
                                        </p>
                                    </div>
                                </div>
                            </div>
                            <div className="flex items-center gap-4">
                                <Badge className="bg-success/10 text-success border border-success/20 font-mono text-[10px] px-3 py-1 uppercase tracking-widest">
                                     {t('nodes.operational')}
                                </Badge>
                            </div>
                        </div>

                        {/* GPUs Grid */}
                        <div className="grid gap-6 sm:grid-cols-2 lg:grid-cols-4">
                            {node.gpus.map((gpu) => {
                                const modelUid = getGpuModel(node.node_id, gpu.index)
                                const usage = gpu.memory_total_mb > 0 ? Math.round((gpu.memory_used_mb / gpu.memory_total_mb) * 100) : 0
                                return (
                                    <div key={gpu.index} className="bg-card/40 backdrop-blur-xl border border-border rounded-xl p-6 rim-light transition-all duration-300 space-y-6 group">
                                        <div className="flex items-center justify-between">
                                            <div className="flex items-center gap-3">
                                                <div className="p-2 rounded-lg bg-white/5 text-muted-foreground group-hover:text-primary transition-colors">
                                                    <Cpu className="h-4 w-4" />
                                                </div>
                                                 <span className="text-xs font-bold font-mono text-foreground tracking-widest uppercase">{t('nodes.gpu')} {gpu.index}</span>
                                            </div>
                                            <div className="flex items-center gap-1.5">
                                                <Zap className={cn("h-3 w-3", usage > 1 ? "text-primary animate-signal" : "text-muted-foreground/30")} />
                                                <span className={cn("text-xs font-mono font-bold tracking-tighter", usage > 80 ? "text-destructive" : "text-primary")}>
                                                    {usage}%
                                                </span>
                                            </div>
                                        </div>
                                        <p className="text-[10px] font-mono text-muted-foreground truncate">
                                             {(gpu as { name?: string }).name || t('nodes.gpu')}
                                            {(gpu as { driver_version?: string }).driver_version
                                                ? ` · drv ${(gpu as { driver_version?: string }).driver_version}`
                                                : ''}
                                            {(gpu as { cuda_version?: string }).cuda_version
                                                ? ` · cuda ${(gpu as { cuda_version?: string }).cuda_version}`
                                                : ''}
                                        </p>

                                        <div className="space-y-3">
                                            <Progress value={usage} className="h-1.5 bg-white/5" indicatorClassName={usage > 85 ? "bg-destructive" : "bg-primary"} />
                                            <div className="flex items-center justify-between text-[10px] font-bold text-muted-foreground/60 uppercase tracking-widest">
                                                 <span>{t('nodes.memoryUsage')}</span>
                                                <span className="text-foreground font-mono">
                                                    {(gpu.memory_used_mb / 1024).toFixed(1)}G / {(gpu.memory_total_mb / 1024).toFixed(1)}G
                                                </span>
                                            </div>
                                        </div>

                                        {/* Temperature & Utilization */}
                                        <div className="grid grid-cols-2 gap-4 py-1 border-y border-border/30">
                                            <div className="space-y-1.5">
                                                <div className="flex items-center gap-1.5 text-[9px] font-bold text-muted-foreground/50 uppercase tracking-widest">
                                                    <Thermometer className="h-3 w-3" />
                                                     <span>{t('nodes.coreTemp')}</span>
                                                </div>
                                                <p className={cn(
                                                    "text-sm font-mono font-bold",
                                                    gpu.temperature_c != null && gpu.temperature_c > 75 ? "text-destructive" : "text-foreground"
                                                )}>
                                                    {gpu.temperature_c != null ? `${gpu.temperature_c}°C` : "—"}
                                                </p>
                                            </div>
                                            <div className="space-y-1.5">
                                                <div className="flex items-center gap-1.5 text-[9px] font-bold text-muted-foreground/50 uppercase tracking-widest">
                                                    <Gauge className="h-3 w-3" />
                                                     <span>{t('nodes.coreUtil')}</span>
                                                </div>
                                                <p className={cn(
                                                    "text-sm font-mono font-bold",
                                                    gpu.utilization_gpu != null && gpu.utilization_gpu > 80 ? "text-destructive" : "text-foreground"
                                                )}>
                                                    {gpu.utilization_gpu != null ? `${gpu.utilization_gpu}%` : "—"}
                                                </p>
                                            </div>
                                        </div>

                                        <div className="pt-2">
                                            <div className="flex items-center justify-between">
                                                 <span className="text-[9px] font-bold text-muted-foreground/50 uppercase tracking-widest">{t('nodes.activeWorkload')}</span>
                                                {modelUid ? (
                                                    <Badge className="px-2 py-0 h-5 text-[9px] font-bold font-mono bg-primary/10 text-primary border border-primary/20 uppercase tracking-widest">
                                                        {modelUid}
                                                    </Badge>
                                                ) : (
                                                    <div className="flex items-center gap-1.5">
                                                        <Activity className="h-3 w-3 text-success/50" />
                                                         <span className="text-[9px] font-bold text-success/70 uppercase tracking-widest">{t('nodes.idleReady')}</span>
                                                    </div>
                                                )}
                                            </div>
                                        </div>
                                    </div>
                                )
                            })}
                        </div>
                    </div>
                ))}
            </div>
        </div>
    )
}
