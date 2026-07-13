import { useCallback, useEffect, useState } from "react"
import {
    Layers, Loader2, RefreshCw, Trash2, Plus, Activity, Link2, EyeOff,
} from "lucide-react"
import { Badge } from "@/components/ui/badge"
import { Button } from "@/components/ui/button"
import { Input } from "@/components/ui/input"
import { Label } from "@/components/ui/label"
import {
    Table, TableBody, TableCell, TableHead, TableHeader, TableRow,
} from "@/components/ui/table"
import {
    Dialog, DialogContent, DialogHeader, DialogTitle, DialogFooter,
} from "@/components/ui/dialog"
import { cn } from "@/lib/utils"
import { v2 } from "@/lib/api"
import type { CellIngress, CellObservation, RegisterCellPayload } from "@/lib/types"
import { useI18n } from "@/lib/i18n"
import { useCells } from "@/hooks/useCells"
import { useAuthStore } from "@/store/useAuthStore"
import { toast } from "sonner"

function fmtPct(v?: number | null) {
    if (v == null || Number.isNaN(v)) return "n/a"
    return `${(v * 100).toFixed(1)}%`
}

function fmtNum(v?: number | null) {
    if (v == null || Number.isNaN(v)) return "n/a"
    return String(v)
}

function statusClass(status: string) {
    if (status === "ready") return "bg-success/10 text-success border-success/20"
    if (status === "unhealthy") return "bg-destructive/10 text-destructive border-destructive/20"
    return "bg-muted text-muted-foreground border-border"
}

export function CellsView() {
    const { t } = useI18n()
    const { token } = useAuthStore()
    const { data: cells = [], isLoading, refetch } = useCells()
    const [selected, setSelected] = useState<CellIngress | null>(null)
    const [obs, setObs] = useState<CellObservation | null>(null)
    const [obsLoading, setObsLoading] = useState(false)
    const [registerOpen, setRegisterOpen] = useState(false)
    const [form, setForm] = useState({
        model_uid: "",
        base_url: "",
        native_stack: "sglang-model-gateway",
        engine_type: "sglang",
    })

    const loadObs = useCallback(async (cell: CellIngress) => {
        setObsLoading(true)
        try {
            const o = await v2.observeCell(cell.model_uid, cell.cell_id, token || "")
            setObs(o)
            setSelected(o.cell)
        } catch (err) {
            toast.error(err instanceof Error ? err.message : t("cells.observeFailed"))
        } finally {
            setObsLoading(false)
        }
    }, [token, t])

    useEffect(() => {
        if (!selected) return
        const id = setInterval(() => { loadObs(selected) }, 10_000)
        return () => clearInterval(id)
    }, [selected, loadObs])

    const onSelect = (cell: CellIngress) => {
        setSelected(cell)
        setObs(null)
        void loadObs(cell)
    }

    const deregister = async (cell: CellIngress) => {
        const promise = v2.deregisterCell(cell.model_uid, cell.cell_id, token || "")
        toast.promise(promise, {
            loading: t("cells.deregistering"),
            success: () => {
                if (selected?.cell_id === cell.cell_id) {
                    setSelected(null)
                    setObs(null)
                }
                refetch()
                return t("cells.deregistered")
            },
            error: (err) => err instanceof Error ? err.message : t("cells.deregisterFailed"),
        })
        await promise.catch(() => undefined)
    }

    const register = async () => {
        const body: RegisterCellPayload = {
            model_uid: form.model_uid.trim(),
            base_url: form.base_url.trim(),
            topology: {
                kind: "native_gateway",
                native_stack: form.native_stack.trim() || undefined,
            },
            engine_type: form.engine_type.trim() || undefined,
        }
        const promise = v2.registerCell(body, token || "")
        toast.promise(promise, {
            loading: t("cells.registering"),
            success: () => {
                setRegisterOpen(false)
                refetch()
                return t("cells.registered")
            },
            error: (err) => err instanceof Error ? err.message : t("cells.registerFailed"),
        })
        await promise.catch(() => undefined)
    }

    return (
        <div className="space-y-8 animate-in fade-in duration-500">
            <div className="flex justify-between items-end gap-4">
                <div>
                    <h2 className="text-3xl font-bold tracking-tight font-mono uppercase text-foreground">
                        {t("cells.title")}
                    </h2>
                    <p className="text-muted-foreground mt-2">{t("cells.subtitle")}</p>
                </div>
                <Button onClick={() => setRegisterOpen(true)} className="font-mono uppercase text-xs">
                    <Plus className="h-4 w-4 mr-2" />
                    {t("cells.register")}
                </Button>
            </div>

            <div className="grid grid-cols-1 xl:grid-cols-5 gap-6">
                <div className="xl:col-span-3 bg-card/40 backdrop-blur-xl border border-border rounded-xl overflow-hidden">
                    <div className="px-6 py-4 border-b border-border/50 flex items-center justify-between bg-white/5">
                        <h3 className="text-xs font-bold font-mono uppercase tracking-widest text-muted-foreground">
                            {t("cells.list")}
                        </h3>
                        <Button variant="ghost" size="sm" onClick={() => refetch()}>
                            <RefreshCw className="h-3.5 w-3.5" />
                        </Button>
                    </div>
                    <Table>
                        <TableHeader className="bg-black/20">
                            <TableRow className="border-border/50 hover:bg-transparent">
                                <TableHead className="text-[10px] uppercase font-bold text-muted-foreground px-6">
                                    {t("cells.model")}
                                </TableHead>
                                <TableHead className="text-[10px] uppercase font-bold text-muted-foreground">
                                    {t("cells.topology")}
                                </TableHead>
                                <TableHead className="text-[10px] uppercase font-bold text-muted-foreground">
                                    {t("cells.health")}
                                </TableHead>
                                <TableHead className="text-right text-[10px] uppercase font-bold text-muted-foreground pr-6">
                                    {t("common.actions")}
                                </TableHead>
                            </TableRow>
                        </TableHeader>
                        <TableBody>
                            {isLoading ? (
                                <TableRow>
                                    <TableCell colSpan={4} className="h-40 text-center text-muted-foreground">
                                        <Loader2 className="h-4 w-4 animate-spin inline mr-2" />
                                        {t("cells.loading")}
                                    </TableCell>
                                </TableRow>
                            ) : cells.length === 0 ? (
                                <TableRow>
                                    <TableCell colSpan={4} className="h-40 text-center text-[10px] font-mono uppercase tracking-widest text-muted-foreground opacity-50">
                                        {t("cells.empty")}
                                    </TableCell>
                                </TableRow>
                            ) : (
                                cells.map((cell) => (
                                    <TableRow
                                        key={`${cell.model_uid}/${cell.cell_id}`}
                                        className={cn(
                                            "border-border/40 hover:bg-white/5 cursor-pointer transition-colors",
                                            selected?.cell_id === cell.cell_id && "bg-white/5",
                                        )}
                                        onClick={() => onSelect(cell)}
                                    >
                                        <TableCell className="px-6 py-4">
                                            <div className="flex flex-col gap-1">
                                                <span className="font-mono text-sm font-bold uppercase">{cell.model_uid}</span>
                                                <span className="text-[9px] font-mono text-muted-foreground tracking-widest">
                                                    {cell.cell_id}
                                                </span>
                                            </div>
                                        </TableCell>
                                        <TableCell>
                                            <div className="flex flex-col gap-1">
                                                <Badge variant="outline" className="w-fit text-[9px] font-mono uppercase">
                                                    {cell.topology.kind}
                                                </Badge>
                                                <span className="text-[10px] text-muted-foreground font-mono">
                                                    {cell.topology.native_stack || cell.engine_type || "—"}
                                                </span>
                                            </div>
                                        </TableCell>
                                        <TableCell>
                                            <Badge className={cn("text-[9px] font-mono uppercase border", statusClass(cell.status))}>
                                                {cell.status}
                                            </Badge>
                                        </TableCell>
                                        <TableCell className="text-right pr-6">
                                            <Button
                                                variant="ghost"
                                                size="sm"
                                                className="text-destructive"
                                                onClick={(e) => {
                                                    e.stopPropagation()
                                                    void deregister(cell)
                                                }}
                                            >
                                                <Trash2 className="h-3.5 w-3.5" />
                                            </Button>
                                        </TableCell>
                                    </TableRow>
                                ))
                            )}
                        </TableBody>
                    </Table>
                </div>

                <div className="xl:col-span-2 space-y-4">
                    {!selected ? (
                        <div className="bg-card/40 border border-border rounded-xl p-8 text-center text-muted-foreground text-sm">
                            <Layers className="h-8 w-8 mx-auto mb-3 opacity-40" />
                            {t("cells.selectHint")}
                        </div>
                    ) : (
                        <>
                            <div className="bg-card/40 border border-border rounded-xl p-6 space-y-4">
                                <div className="flex items-start justify-between gap-3">
                                    <div>
                                        <p className="text-[10px] font-mono uppercase tracking-widest text-muted-foreground">
                                            {t("cells.ingress")}
                                        </p>
                                        <h3 className="font-mono font-bold text-lg mt-1">{selected.model_uid}</h3>
                                        <p className="text-[11px] font-mono text-muted-foreground mt-1 break-all">
                                            {selected.base_url}
                                        </p>
                                    </div>
                                    <Button
                                        variant="outline"
                                        size="sm"
                                        disabled={obsLoading}
                                        onClick={() => loadObs(selected)}
                                    >
                                        {obsLoading ? <Loader2 className="h-3.5 w-3.5 animate-spin" /> : <RefreshCw className="h-3.5 w-3.5" />}
                                    </Button>
                                </div>
                                <div className="flex flex-wrap gap-2">
                                    <Badge className={cn("text-[9px] font-mono uppercase border", statusClass(selected.status))}>
                                        {selected.status}
                                    </Badge>
                                    <Badge variant="outline" className="text-[9px] font-mono uppercase gap-1">
                                        <EyeOff className="h-3 w-3" />
                                        {t("cells.topologyHidden")}
                                    </Badge>
                                </div>
                                <div className="grid grid-cols-2 gap-3 text-[11px] font-mono">
                                    <div>
                                        <p className="text-muted-foreground uppercase tracking-widest text-[9px] mb-1">
                                            {t("cells.engine")}
                                        </p>
                                        <p>{selected.engine_type || "n/a"}{selected.engine_version ? ` @ ${selected.engine_version}` : ""}</p>
                                    </div>
                                    <div>
                                        <p className="text-muted-foreground uppercase tracking-widest text-[9px] mb-1">
                                            {t("cells.stack")}
                                        </p>
                                        <p>{selected.topology.native_stack || selected.topology.kind}</p>
                                    </div>
                                </div>
                                <div className="flex items-center gap-2 text-[10px] text-muted-foreground font-mono">
                                    <Link2 className="h-3 w-3" />
                                    {t("cells.dataSourceNote")}
                                </div>
                            </div>

                            <div className="bg-card/40 border border-border rounded-xl p-6 space-y-4">
                                <div className="flex items-center gap-2">
                                    <Activity className="h-4 w-4 text-primary" />
                                    <h4 className="text-xs font-bold font-mono uppercase tracking-widest">
                                        {t("cells.ingressMetrics")}
                                    </h4>
                                </div>
                                {!obs ? (
                                    <p className="text-sm text-muted-foreground">{obsLoading ? t("cells.observing") : t("cells.noObs")}</p>
                                ) : (
                                    <div className="grid grid-cols-3 gap-3">
                                        <div className="bg-black/20 rounded-lg p-3">
                                            <p className="text-[9px] uppercase tracking-widest text-muted-foreground mb-1">
                                                {t("cells.pending")}
                                            </p>
                                            <p className="font-mono text-lg font-bold">{fmtNum(obs.stats.pending_requests)}</p>
                                        </div>
                                        <div className="bg-black/20 rounded-lg p-3">
                                            <p className="text-[9px] uppercase tracking-widest text-muted-foreground mb-1">
                                                {t("cells.kv")}
                                            </p>
                                            <p className="font-mono text-lg font-bold">{fmtPct(obs.stats.kv_cache_usage)}</p>
                                        </div>
                                        <div className="bg-black/20 rounded-lg p-3">
                                            <p className="text-[9px] uppercase tracking-widest text-muted-foreground mb-1">
                                                {t("cells.prefix")}
                                            </p>
                                            <p className="font-mono text-lg font-bold">{fmtPct(obs.stats.prefix_cache_hit_rate)}</p>
                                        </div>
                                    </div>
                                )}
                                {obs && (
                                    <p className="text-[10px] font-mono text-muted-foreground">
                                        scrape={obs.stats.scrape_status} · source={obs.stats.data_source} ·
                                        health={obs.health_ok ? "ok" : "fail"}
                                    </p>
                                )}
                            </div>
                        </>
                    )}
                </div>
            </div>

            <Dialog open={registerOpen} onOpenChange={setRegisterOpen}>
                <DialogContent className="sm:max-w-md">
                    <DialogHeader>
                        <DialogTitle className="font-mono uppercase text-sm tracking-widest">
                            {t("cells.register")}
                        </DialogTitle>
                    </DialogHeader>
                    <div className="space-y-4 py-2">
                        <div className="space-y-2">
                            <Label className="text-[10px] font-mono uppercase">{t("cells.model")}</Label>
                            <Input
                                value={form.model_uid}
                                onChange={(e) => setForm((f) => ({ ...f, model_uid: e.target.value }))}
                                placeholder="my-model"
                                className="font-mono"
                            />
                        </div>
                        <div className="space-y-2">
                            <Label className="text-[10px] font-mono uppercase">Base URL</Label>
                            <Input
                                value={form.base_url}
                                onChange={(e) => setForm((f) => ({ ...f, base_url: e.target.value }))}
                                placeholder="http://127.0.0.1:30000"
                                className="font-mono"
                            />
                        </div>
                        <div className="space-y-2">
                            <Label className="text-[10px] font-mono uppercase">{t("cells.stack")}</Label>
                            <Input
                                value={form.native_stack}
                                onChange={(e) => setForm((f) => ({ ...f, native_stack: e.target.value }))}
                                className="font-mono"
                            />
                        </div>
                        <div className="space-y-2">
                            <Label className="text-[10px] font-mono uppercase">{t("cells.engine")}</Label>
                            <Input
                                value={form.engine_type}
                                onChange={(e) => setForm((f) => ({ ...f, engine_type: e.target.value }))}
                                className="font-mono"
                            />
                        </div>
                        <p className="text-[11px] text-muted-foreground">{t("cells.registerHint")}</p>
                    </div>
                    <DialogFooter>
                        <Button variant="outline" onClick={() => setRegisterOpen(false)}>{t("common.cancel")}</Button>
                        <Button
                            onClick={() => void register()}
                            disabled={!form.model_uid.trim() || !form.base_url.trim()}
                        >
                            {t("cells.register")}
                        </Button>
                    </DialogFooter>
                </DialogContent>
            </Dialog>
        </div>
    )
}
