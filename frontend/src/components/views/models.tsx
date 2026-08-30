import { useState } from "react"
import { Plus, Trash2, Box, Play, Square, Loader2, ExternalLink, Copy, Check, Search, Filter } from "lucide-react"
import { Badge } from "@/components/ui/badge"
import { Button } from "@/components/ui/button"
import { Progress } from "@/components/ui/progress"
import { Table, TableBody, TableCell, TableHead, TableHeader, TableRow } from "@/components/ui/table"
import type { AggregatedModelState } from "@/lib/types"
import { v2 } from "@/lib/api"
import { cn } from "@/lib/utils"
import { useI18n } from "@/lib/useI18n"
import { useModels } from "@/hooks/useModels"
import { useAuthStore } from "@/store/useAuthStore"
import { toast } from "sonner"

const STATE_CONFIG: Record<AggregatedModelState, { key: string; color: string; animate?: boolean }> = {
    running: { key: "state.running", color: "text-success bg-success/10 border-success/20" },
    stopped: { key: "state.stopped", color: "text-muted-foreground bg-white/5 border-border" },
    downloading: { key: "state.downloading", color: "text-primary bg-primary/10 border-primary/20", animate: true },
    starting: { key: "state.starting", color: "text-warning bg-warning/10 border-warning/20", animate: true },
    degraded: { key: "state.degraded", color: "text-destructive bg-destructive/10 border-destructive/20" },
    failed: { key: "state.failed", color: "text-destructive bg-destructive/10 border-destructive/20" },
    stopping: { key: "state.stopping", color: "text-muted-foreground bg-white/5 border-border", animate: true },
}

export function ModelsView() {
    const { t } = useI18n()
    const { token } = useAuthStore()
    const { data: models = [], isLoading: initialLoading, refetch } = useModels()
    const [acting, setActing] = useState<string | null>(null)
    const [filter, setFilter] = useState<AggregatedModelState | "all">("all")
    const [searchQuery, setSearchQuery] = useState("")
    const [copiedModelUid, setCopiedModelUid] = useState<string | null>(null)

    const act = async (uid: string, actionName: string, fn: () => Promise<unknown>) => {
        setActing(uid)
        const promise = fn()
        toast.promise(promise, {
            loading: t('models.actionLoading', { action: actionName, uid }),
            success: () => {
                refetch()
                return t('models.actionSuccess', { action: actionName })
            },
            error: (err) => err instanceof Error ? err.message : t('models.actionFailedFor', { action: actionName }),
        })
        try {
            await promise
        } finally {
            setActing(null)
        }
    }

    const copyModelName = async (uid: string, modelName: string) => {
        try {
            await navigator.clipboard.writeText(modelName)
            setCopiedModelUid(uid)
            toast.success(t('models.copySuccess'))
            setTimeout(() => setCopiedModelUid(null), 2000)
        } catch {
            toast.error(t('models.copyFail'))
        }
    }

    const filtered = models.filter((m) => {
        const matchesState = filter === "all" || m.state === filter
        const matchesSearch = m.model_uid.toLowerCase().includes(searchQuery.toLowerCase()) ||
                            m.model_name.toLowerCase().includes(searchQuery.toLowerCase())
        return matchesState && matchesSearch
    })

    return (
        <div className="space-y-8 animate-in fade-in duration-500">
            {/* Header */}
            <div className="flex flex-col md:flex-row md:items-end justify-between gap-4">
                <div>
                    <h2 className="text-3xl font-bold tracking-tight font-mono uppercase text-foreground">{t('models.title')}</h2>
                    <p className="text-muted-foreground mt-2">{t('models.subtitle')}</p>
                </div>
                <Button className="bg-primary text-primary-foreground rim-light h-11 px-6 font-bold uppercase tracking-widest text-xs">
                    <Plus className="mr-2 h-4 w-4" />
                    {t('models.loadModel')}
                </Button>
            </div>

            {/* Toolbar */}
            <div className="flex flex-col md:flex-row gap-4 items-center justify-between bg-card/40 backdrop-blur-xl border border-border p-4 rounded-xl">
                <div className="flex items-center gap-2 flex-wrap">
                    <div className="flex items-center gap-2 bg-black/20 px-3 py-1.5 rounded-lg border border-border/50">
                        <Filter className="h-3.5 w-3.5 text-muted-foreground" />
                        {(["all", "running", "stopped", "downloading", "failed"] as const).map((s) => (
                            <button
                                key={s}
                                onClick={() => setFilter(s)}
                                className={cn(
                                    "px-2.5 py-1 rounded-md text-[10px] font-bold uppercase tracking-wider transition-all",
                                    filter === s
                                        ? "bg-primary text-primary-foreground shadow-sm"
                                        : "text-muted-foreground hover:text-foreground"
                                )}
                            >
                                {s === "all" ? t('common.all') : t(`state.${s}`)}
                            </button>
                        ))}
                    </div>
                </div>

                <div className="relative w-full md:w-64 group">
                    <Search className="absolute left-3 top-1/2 -translate-y-1/2 h-4 w-4 text-muted-foreground group-focus-within:text-primary transition-colors" />
                    <input
                        type="text"
                        placeholder={t('models.searchPlaceholder')}
                        value={searchQuery}
                        onChange={(e) => setSearchQuery(e.target.value)}
                        className="w-full bg-black/20 border border-border/50 rounded-lg pl-10 pr-4 py-2 text-xs font-mono focus:outline-none focus:border-primary/50 transition-all"
                    />
                </div>
            </div>

            {/* Models Grid/Table */}
            <div className="bg-card/40 backdrop-blur-xl border border-border rounded-xl overflow-hidden">
                <Table>
                    <TableHeader className="bg-black/20">
                        <TableRow className="border-border/50 hover:bg-transparent">
                            <TableHead className="text-[10px] uppercase font-bold text-muted-foreground px-6 py-4">{t('models.identity')}</TableHead>
                            <TableHead className="text-[10px] uppercase font-bold text-muted-foreground">{t('common.status')}</TableHead>
                            <TableHead className="text-[10px] uppercase font-bold text-muted-foreground">{t('models.provisioning')}</TableHead>
                            <TableHead className="text-[10px] uppercase font-bold text-muted-foreground">{t('models.engine')}</TableHead>
                            <TableHead className="text-[10px] uppercase font-bold text-muted-foreground text-right pr-6">{t('models.management')}</TableHead>
                        </TableRow>
                    </TableHeader>
                    <TableBody>
                        {initialLoading ? (
                            <TableRow>
                                <TableCell colSpan={5} className="h-64 text-center">
                                    <div className="flex flex-col items-center gap-3 opacity-50">
                                        <Loader2 className="h-8 w-8 animate-spin text-primary" />
                                        <p className="text-[10px] font-mono uppercase tracking-widest">{t('models.loading')}</p>
                                    </div>
                                </TableCell>
                            </TableRow>
                        ) : filtered.length === 0 ? (
                            <TableRow>
                                <TableCell colSpan={5} className="h-64 text-center">
                                    <div className="flex flex-col items-center gap-3 opacity-30">
                                        <Box className="h-12 w-12" />
                                        <p className="text-[10px] font-mono uppercase tracking-widest">{t('models.empty')}</p>
                                    </div>
                                </TableCell>
                            </TableRow>
                        ) : (
                            filtered.map((model) => {
                                const config = STATE_CONFIG[model.state] || STATE_CONFIG.stopped
                                const isActing = acting === model.model_uid
                                return (
                                    <TableRow key={model.model_uid} className="border-border/40 hover:bg-white/5 transition-colors group">
                                        <TableCell className="px-6 py-5">
                                            <div className="flex flex-col gap-1.5">
                                                <div className="flex items-center gap-2">
                                                    <span className="font-mono text-sm font-bold group-hover:text-primary transition-colors">{model.model_uid}</span>
                                                    {copiedModelUid === model.model_uid ? (
                                                        <Check className="h-3 w-3 text-success" />
                                                    ) : (
                                                        <Copy
                                                            className="h-3 w-3 text-muted-foreground opacity-0 group-hover:opacity-100 cursor-pointer hover:text-foreground transition-all"
                                                            onClick={() => copyModelName(model.model_uid, model.model_name)}
                                                        />
                                                    )}
                                                </div>
                                                <span className="text-[10px] text-muted-foreground/60 font-mono truncate max-w-[300px]">{model.model_name}</span>
                                            </div>
                                        </TableCell>
                                        <TableCell>
                                            <div className="flex flex-col gap-2">
                                                <div className="flex items-center gap-2">
                                                    <div className={cn("w-1.5 h-1.5 rounded-full", config.animate ? "animate-pulse" : "", config.color.split(' ')[0])} />
                                                    <span className={cn("text-[10px] font-bold uppercase tracking-wider px-2 py-0.5 rounded border", config.color)}>
                                                        {t(config.key)}
                                                    </span>
                                                </div>
                                                {model.state === "downloading" && (
                                                    <Progress value={45} className="h-1 w-24 bg-white/5" />
                                                )}
                                            </div>
                                        </TableCell>
                                        <TableCell>
                                            <div className="flex items-baseline gap-1 font-mono">
                                                <span className="text-sm font-bold text-foreground">{model.replicas.ready}</span>
                                                <span className="text-[10px] text-muted-foreground">/ {model.replicas.desired}</span>
                                                {model.replicas.unhealthy > 0 && (
                                                     <Badge variant="destructive" className="ml-2 text-[9px] h-4">{model.replicas.unhealthy} {t('models.unhealthy')}</Badge>
                                                )}
                                            </div>
                                        </TableCell>
                                        <TableCell>
                                            <Badge variant="outline" className="text-[10px] font-mono border-border/50 text-muted-foreground uppercase">{model.engine_type || "vLLM"}</Badge>
                                        </TableCell>
                                        <TableCell className="text-right pr-6">
                                            <div className="flex items-center justify-end gap-2">
                                                {(model.state === "stopped" || model.state === "failed") && (
                                                    <Button
                                                        variant="ghost" size="sm"
                                                        className="h-8 w-8 p-0 hover:bg-success/20 hover:text-success"
                                                        onClick={() => act(model.model_uid, "START", () => v2.startModel(model.model_uid, {}, token || ''))}
                                                        disabled={isActing}
                                                    >
                                                        <Play className="h-4 w-4" />
                                                    </Button>
                                                )}
                                                {model.state === "running" && (
                                                    <Button
                                                        variant="ghost" size="sm"
                                                        className="h-8 w-8 p-0 hover:bg-destructive/20 hover:text-destructive"
                                                        onClick={() => act(model.model_uid, "STOP", () => v2.stopModel(model.model_uid, token || ''))}
                                                        disabled={isActing}
                                                    >
                                                        <Square className="h-4 w-4" />
                                                    </Button>
                                                )}
                                                <Button
                                                    variant="ghost" size="sm"
                                                    className="h-8 w-8 p-0 hover:bg-white/10"
                                                >
                                                    <ExternalLink className="h-4 w-4" />
                                                </Button>
                                                <Button
                                                    variant="ghost" size="sm"
                                                    className="h-8 w-8 p-0 hover:bg-destructive/20 hover:text-destructive"
                                                    disabled={isActing}
                                                >
                                                    <Trash2 className="h-4 w-4" />
                                                </Button>
                                            </div>
                                        </TableCell>
                                    </TableRow>
                                )
                            })
                        )}
                    </TableBody>
                </Table>
            </div>
        </div>
    )
}
