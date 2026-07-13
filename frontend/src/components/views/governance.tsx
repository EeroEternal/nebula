import { useEffect, useState } from "react"
import { Shield, Activity, RefreshCw, Loader2, Database, Gauge, FlaskConical } from "lucide-react"
import { Badge } from "@/components/ui/badge"
import { Button } from "@/components/ui/button"
import { Input } from "@/components/ui/input"
import {
    Table, TableBody, TableCell, TableHead, TableHeader, TableRow,
} from "@/components/ui/table"
import { v2 } from "@/lib/api"
import type {
    BenchmarkRun,
    CanaryRelease,
    CompatibilityRule,
    DiagnosticEvent,
    HardwareInventory,
    ModelSlo,
    RecommendResponse,
    SloEvaluation,
    Tenant,
    TenantCostSummary,
    CostPriceConfig,
} from "@/lib/types"
import { useAuthStore } from "@/store/useAuthStore"
import { useI18n } from "@/lib/i18n"
import { toast } from "sonner"
import { cn } from "@/lib/utils"

export function GovernanceView() {
    const { t } = useI18n()
    const { token } = useAuthStore()
    const [rules, setRules] = useState<CompatibilityRule[]>([])
    const [inventory, setInventory] = useState<HardwareInventory | null>(null)
    const [slos, setSlos] = useState<ModelSlo[]>([])
    const [events, setEvents] = useState<DiagnosticEvent[]>([])
    const [runs, setRuns] = useState<BenchmarkRun[]>([])
    const [canaries, setCanaries] = useState<CanaryRelease[]>([])
    const [tenants, setTenants] = useState<Tenant[]>([])
    const [pricing, setPricing] = useState<CostPriceConfig[]>([])
    const [costByTenant, setCostByTenant] = useState<Record<string, TenantCostSummary>>({})
    const [newTenantId, setNewTenantId] = useState("")
    const [newTenantRps, setNewTenantRps] = useState("60")
    const [evalUid, setEvalUid] = useState("")
    const [evaluation, setEvaluation] = useState<SloEvaluation | null>(null)
    const [recModel, setRecModel] = useState("")
    const [recWorkload, setRecWorkload] = useState("short-chat-v1")
    const [recommend, setRecommend] = useState<RecommendResponse | null>(null)
    const [canaryModel, setCanaryModel] = useState("")
    const [canaryCandidate, setCanaryCandidate] = useState("")
    const [canaryStable, setCanaryStable] = useState("")
    const [loading, setLoading] = useState(true)

    const refresh = async () => {
        setLoading(true)
        try {
            const [r, inv, s, e, br, c, tn, pr] = await Promise.all([
                v2.listCompatRules(token || ""),
                v2.hardwareInventory(token || ""),
                v2.listSlos(token || ""),
                v2.listDiagnostics(undefined, token || ""),
                v2.listBenchmarkRuns(token || ""),
                v2.listCanaries(token || ""),
                v2.listTenants(token || ""),
                v2.listPricing(token || ""),
            ])
            setRules(r)
            setInventory(inv)
            setSlos(s)
            setEvents(e)
            setRuns(br)
            setCanaries(c)
            setTenants(tn)
            setPricing(pr)
            const costs: Record<string, TenantCostSummary> = {}
            await Promise.all(
                tn.map(async (t) => {
                    try {
                        costs[t.tenant_id] = await v2.tenantCost(t.tenant_id, token || "")
                    } catch {
                        /* no usage yet */
                    }
                }),
            )
            setCostByTenant(costs)
        } catch (err) {
            toast.error(err instanceof Error ? err.message : "load failed")
        } finally {
            setLoading(false)
        }
    }

    useEffect(() => { void refresh() }, [token])

    const seed = async () => {
        try {
            const r = await v2.seedCompatRules(token || "")
            setRules(r)
            toast.success(t("governance.seeded"))
        } catch (err) {
            toast.error(err instanceof Error ? err.message : "seed failed")
        }
    }

    const ensureSlo = async (modelUid: string) => {
        try {
            const slo = await v2.upsertSlo(modelUid, {
                availability_target: 0.99,
                ttft_p95_ms: 2000,
                latency_p95_ms: 30000,
                window: "15m",
            }, token || "")
            setSlos((prev) => {
                const rest = prev.filter((x) => x.model_uid !== modelUid)
                return [...rest, slo].sort((a, b) => a.model_uid.localeCompare(b.model_uid))
            })
            toast.success(t("governance.sloSaved"))
        } catch (err) {
            toast.error(err instanceof Error ? err.message : "slo save failed")
        }
    }

    const runEval = async () => {
        if (!evalUid.trim()) return
        try {
            await ensureSlo(evalUid.trim())
            const ev = await v2.evaluateSlo(evalUid.trim(), token || "")
            setEvaluation(ev)
        } catch (err) {
            toast.error(err instanceof Error ? err.message : "evaluate failed")
        }
    }

    const runRecommend = async () => {
        if (!recModel.trim()) return
        try {
            const resp = await v2.recommendEngines({
                model_name: recModel.trim(),
                workload_id: recWorkload.trim() || undefined,
            }, token || "")
            setRecommend(resp)
            if (resp.status === "insufficient_data") {
                toast.message(resp.message || t("governance.recInsufficient"))
            }
        } catch (err) {
            toast.error(err instanceof Error ? err.message : "recommend failed")
        }
    }

    const createCanary = async () => {
        if (!canaryModel.trim() || !canaryCandidate.trim()) return
        try {
            await v2.createCanary({
                model_uid: canaryModel.trim(),
                candidate_image_id: canaryCandidate.trim(),
                stable_image_id: canaryStable.trim() || undefined,
                traffic_weight_percent: 10,
            }, token || "")
            toast.success(t("governance.canaryCreated"))
            setCanaries(await v2.listCanaries(token || ""))
        } catch (err) {
            toast.error(err instanceof Error ? err.message : "canary create failed")
        }
    }

    const createTenant = async () => {
        if (!newTenantId.trim()) return
        try {
            const rps = Number(newTenantRps) || undefined
            await v2.upsertTenant({
                tenant_id: newTenantId.trim(),
                display_name: newTenantId.trim(),
                enabled: true,
                quotas: { rps_per_minute: rps, max_concurrency: 8 },
            }, token || "")
            toast.success(t("governance.tenantSaved"))
            setNewTenantId("")
            await refresh()
        } catch (err) {
            toast.error(err instanceof Error ? err.message : "tenant save failed")
        }
    }

    const seedPricing = async () => {
        try {
            await v2.upsertPricing({
                price_id: "vllm-cuda-default",
                engine_type: "vllm",
                platform: "nvidia-cuda",
                price_per_1k_input: 0.1,
                price_per_1k_output: 0.2,
                currency: "USD",
            }, token || "")
            toast.success(t("governance.pricingSaved"))
            setPricing(await v2.listPricing(token || ""))
        } catch (err) {
            toast.error(err instanceof Error ? err.message : "pricing save failed")
        }
    }

    return (
        <div className="space-y-8 animate-in fade-in duration-500">
            <div className="flex justify-between items-end gap-4">
                <div>
                    <h2 className="text-3xl font-bold tracking-tight font-mono uppercase text-foreground">
                        {t("governance.title")}
                    </h2>
                    <p className="text-muted-foreground mt-2">{t("governance.subtitle")}</p>
                </div>
                <Button variant="outline" size="sm" onClick={() => void refresh()}>
                    {loading ? <Loader2 className="h-3.5 w-3.5 animate-spin" /> : <RefreshCw className="h-3.5 w-3.5" />}
                </Button>
            </div>

            <section className="bg-card/40 border border-border rounded-xl overflow-hidden">
                <div className="px-6 py-4 border-b border-border/50 flex items-center justify-between bg-white/5">
                    <h3 className="text-xs font-bold font-mono uppercase tracking-widest flex items-center gap-2">
                        <Shield className="h-3.5 w-3.5" /> {t("governance.compat")}
                    </h3>
                    <Button size="sm" variant="secondary" onClick={() => void seed()}>{t("governance.seed")}</Button>
                </div>
                <Table>
                    <TableHeader>
                        <TableRow className="hover:bg-transparent">
                            <TableHead className="text-[10px] uppercase">ID</TableHead>
                            <TableHead className="text-[10px] uppercase">Engine</TableHead>
                            <TableHead className="text-[10px] uppercase">Platforms</TableHead>
                            <TableHead className="text-[10px] uppercase">Verdict</TableHead>
                            <TableHead className="text-[10px] uppercase">Driver/CUDA</TableHead>
                        </TableRow>
                    </TableHeader>
                    <TableBody>
                        {rules.length === 0 ? (
                            <TableRow>
                                <TableCell colSpan={5} className="text-center text-muted-foreground text-xs py-10">
                                    {t("governance.compatEmpty")}
                                </TableCell>
                            </TableRow>
                        ) : rules.map((r) => (
                            <TableRow key={r.id}>
                                <TableCell className="font-mono text-xs">{r.id}</TableCell>
                                <TableCell className="font-mono text-xs">
                                    {r.engine_type}
                                    {r.engine_version_min ? ` ≥${r.engine_version_min}` : ""}
                                </TableCell>
                                <TableCell className="font-mono text-xs">{r.platforms.join(", ") || "*"}</TableCell>
                                <TableCell>
                                    <Badge className={cn(
                                        "text-[9px] uppercase",
                                        r.verdict === "allow"
                                            ? "bg-success/10 text-success border-success/20"
                                            : "bg-destructive/10 text-destructive border-destructive/20",
                                    )}>{r.verdict}</Badge>
                                </TableCell>
                                <TableCell className="font-mono text-[10px] text-muted-foreground">
                                    {r.min_driver_version || "—"} / {r.min_cuda_version || "—"}
                                </TableCell>
                            </TableRow>
                        ))}
                    </TableBody>
                </Table>
            </section>

            <section className="bg-card/40 border border-border rounded-xl p-6 space-y-4">
                <h3 className="text-xs font-bold font-mono uppercase tracking-widest flex items-center gap-2">
                    <Database className="h-3.5 w-3.5" /> {t("governance.inventory")}
                </h3>
                {!inventory || inventory.nodes.length === 0 ? (
                    <p className="text-sm text-muted-foreground">{t("governance.inventoryEmpty")}</p>
                ) : (
                    <div className="grid gap-3 md:grid-cols-2">
                        {inventory.nodes.map((n) => (
                            <div key={n.node_id} className="border border-border/40 rounded-lg p-4 space-y-2">
                                <div className="flex justify-between items-center">
                                    <span className="font-mono font-bold">{n.node_id}</span>
                                    <Badge variant="outline" className="text-[9px] font-mono uppercase">
                                        {n.platform || "nvidia-cuda"}
                                    </Badge>
                                </div>
                                {n.gpus.map((g) => (
                                    <p key={g.index} className="text-[11px] font-mono text-muted-foreground">
                                        GPU{g.index} {g.name || ""} · drv {g.driver_version || "n/a"} · cuda {g.cuda_version || "n/a"}
                                        {g.occupied_by ? ` · busy:${g.occupied_by}` : " · free"}
                                    </p>
                                ))}
                            </div>
                        ))}
                    </div>
                )}
            </section>

            <section className="bg-card/40 border border-border rounded-xl p-6 space-y-4">
                <h3 className="text-xs font-bold font-mono uppercase tracking-widest flex items-center gap-2">
                    <Activity className="h-3.5 w-3.5" /> {t("governance.slo")}
                </h3>
                <div className="flex gap-2 max-w-md">
                    <Input
                        className="font-mono"
                        placeholder="model_uid"
                        value={evalUid}
                        onChange={(e) => setEvalUid(e.target.value)}
                    />
                    <Button onClick={() => void runEval()}>{t("governance.evaluate")}</Button>
                </div>
                {slos.length > 0 && (
                    <p className="text-[11px] font-mono text-muted-foreground">
                        configured: {slos.map((s) => s.model_uid).join(", ")}
                    </p>
                )}
                {evaluation && (
                    <div className="border border-border/40 rounded-lg p-4 space-y-2">
                        <div className="flex gap-2 items-center">
                            <Badge className="text-[9px] uppercase font-mono">{evaluation.status}</Badge>
                            <span className="text-[10px] text-muted-foreground font-mono">
                                window={evaluation.window} · abort_excluded={String(evaluation.abort_excluded)}
                            </span>
                        </div>
                        {evaluation.breaches.map((b) => (
                            <p key={b} className="text-xs text-destructive font-mono">{b}</p>
                        ))}
                        {evaluation.suggestions.map((s) => (
                            <p key={s.message} className="text-xs font-mono text-muted-foreground">
                                [{s.target}/{s.kind}] {s.message}
                            </p>
                        ))}
                        {evaluation.status === "insufficient_data" && (
                            <p className="text-xs text-muted-foreground">{t("governance.insufficient")}</p>
                        )}
                    </div>
                )}
            </section>

            <section className="bg-card/40 border border-border rounded-xl overflow-hidden">
                <div className="px-6 py-4 border-b border-border/50 bg-white/5">
                    <h3 className="text-xs font-bold font-mono uppercase tracking-widest flex items-center gap-2">
                        <Gauge className="h-3.5 w-3.5" /> {t("governance.benchmark")}
                    </h3>
                </div>
                <Table>
                    <TableHeader>
                        <TableRow className="hover:bg-transparent">
                            <TableHead className="text-[10px] uppercase">Run</TableHead>
                            <TableHead className="text-[10px] uppercase">Model / Engine</TableHead>
                            <TableHead className="text-[10px] uppercase">Workload</TableHead>
                            <TableHead className="text-[10px] uppercase">TTFT p95</TableHead>
                            <TableHead className="text-[10px] uppercase">TPS</TableHead>
                            <TableHead className="text-[10px] uppercase">Status</TableHead>
                        </TableRow>
                    </TableHeader>
                    <TableBody>
                        {runs.length === 0 ? (
                            <TableRow>
                                <TableCell colSpan={6} className="text-center text-muted-foreground text-xs py-10">
                                    {t("governance.benchmarkEmpty")}
                                </TableCell>
                            </TableRow>
                        ) : runs.slice(0, 20).map((r) => (
                            <TableRow key={r.run_id}>
                                <TableCell className="font-mono text-[10px]">{r.run_id}</TableCell>
                                <TableCell className="font-mono text-xs">
                                    {r.profile_key.model_name} / {r.profile_key.engine_type}
                                </TableCell>
                                <TableCell className="font-mono text-xs">{r.profile_key.workload_id}</TableCell>
                                <TableCell className="font-mono text-xs">
                                    {r.ttft_p95_ms != null ? `${r.ttft_p95_ms.toFixed(0)}ms` : "—"}
                                </TableCell>
                                <TableCell className="font-mono text-xs">
                                    {r.throughput_tps != null ? r.throughput_tps.toFixed(1) : "—"}
                                </TableCell>
                                <TableCell>
                                    <Badge variant="outline" className="text-[9px] uppercase">{r.status}</Badge>
                                </TableCell>
                            </TableRow>
                        ))}
                    </TableBody>
                </Table>
            </section>

            <section className="bg-card/40 border border-border rounded-xl p-6 space-y-4">
                <h3 className="text-xs font-bold font-mono uppercase tracking-widest flex items-center gap-2">
                    <FlaskConical className="h-3.5 w-3.5" /> {t("governance.recommend")}
                </h3>
                <div className="flex flex-wrap gap-2 max-w-2xl">
                    <Input
                        className="font-mono max-w-[200px]"
                        placeholder="model_name"
                        value={recModel}
                        onChange={(e) => setRecModel(e.target.value)}
                    />
                    <Input
                        className="font-mono max-w-[180px]"
                        placeholder="workload_id"
                        value={recWorkload}
                        onChange={(e) => setRecWorkload(e.target.value)}
                    />
                    <Button onClick={() => void runRecommend()}>{t("governance.recommendRun")}</Button>
                </div>
                {recommend && (
                    <div className="border border-border/40 rounded-lg p-4 space-y-2">
                        <div className="flex gap-2 items-center">
                            <Badge className="text-[9px] uppercase font-mono">{recommend.status}</Badge>
                            {recommend.message && (
                                <span className="text-xs text-muted-foreground">{recommend.message}</span>
                            )}
                        </div>
                        {recommend.candidates.length === 0 ? (
                            <p className="text-xs text-muted-foreground">{t("governance.recInsufficient")}</p>
                        ) : recommend.candidates.map((c) => (
                            <p key={`${c.engine_type}-${c.image_id || ""}-${c.rationale}`} className="text-xs font-mono text-muted-foreground">
                                {c.engine_type}
                                {c.engine_version ? `@${c.engine_version}` : ""}
                                {" · "}
                                conf={c.confidence}
                                {c.ttft_p95_ms != null ? ` · ttft_p95=${c.ttft_p95_ms.toFixed(0)}ms` : ""}
                                {c.throughput_tps != null ? ` · tps=${c.throughput_tps.toFixed(1)}` : ""}
                                {" — "}
                                {c.rationale}
                            </p>
                        ))}
                    </div>
                )}
            </section>

            <section className="bg-card/40 border border-border rounded-xl p-6 space-y-4">
                <h3 className="text-xs font-bold font-mono uppercase tracking-widest">
                    {t("governance.canary")}
                </h3>
                <div className="flex flex-wrap gap-2 max-w-3xl">
                    <Input
                        className="font-mono max-w-[160px]"
                        placeholder="model_uid"
                        value={canaryModel}
                        onChange={(e) => setCanaryModel(e.target.value)}
                    />
                    <Input
                        className="font-mono max-w-[180px]"
                        placeholder="candidate_image_id"
                        value={canaryCandidate}
                        onChange={(e) => setCanaryCandidate(e.target.value)}
                    />
                    <Input
                        className="font-mono max-w-[180px]"
                        placeholder="stable_image_id"
                        value={canaryStable}
                        onChange={(e) => setCanaryStable(e.target.value)}
                    />
                    <Button onClick={() => void createCanary()}>{t("governance.canaryCreate")}</Button>
                </div>
                {canaries.length === 0 ? (
                    <p className="text-sm text-muted-foreground">{t("governance.canaryEmpty")}</p>
                ) : (
                    <div className="space-y-3">
                        {canaries.map((c) => (
                            <div key={c.canary_id} className="border border-border/40 rounded-lg p-4 space-y-2">
                                <div className="flex flex-wrap gap-2 items-center justify-between">
                                    <div className="space-y-1">
                                        <p className="font-mono text-xs font-bold">{c.canary_id}</p>
                                        <p className="font-mono text-[11px] text-muted-foreground">
                                            {c.model_uid} · {c.candidate_image_id} · weight={c.traffic_weight_percent}%
                                        </p>
                                    </div>
                                    <Badge className="text-[9px] uppercase font-mono">{c.state}</Badge>
                                </div>
                                {c.rollback_reason && (
                                    <p className="text-xs text-destructive font-mono">{c.rollback_reason}</p>
                                )}
                                <div className="flex flex-wrap gap-2">
                                    <Button
                                        size="sm"
                                        variant="secondary"
                                        onClick={async () => {
                                            try {
                                                await v2.evaluateCanary(c.canary_id, false, token || "")
                                                setCanaries(await v2.listCanaries(token || ""))
                                            } catch (err) {
                                                toast.error(err instanceof Error ? err.message : "evaluate failed")
                                            }
                                        }}
                                    >
                                        {t("governance.canaryOk")}
                                    </Button>
                                    <Button
                                        size="sm"
                                        variant="secondary"
                                        onClick={async () => {
                                            try {
                                                await v2.evaluateCanary(c.canary_id, true, token || "")
                                                setCanaries(await v2.listCanaries(token || ""))
                                                toast.message(t("governance.canaryRolled"))
                                            } catch (err) {
                                                toast.error(err instanceof Error ? err.message : "evaluate failed")
                                            }
                                        }}
                                    >
                                        {t("governance.canaryBreach")}
                                    </Button>
                                    <Button
                                        size="sm"
                                        onClick={async () => {
                                            try {
                                                await v2.promoteCanary(c.canary_id, token || "")
                                                setCanaries(await v2.listCanaries(token || ""))
                                                toast.success(t("governance.canaryPromoted"))
                                            } catch (err) {
                                                toast.error(err instanceof Error ? err.message : "promote failed")
                                            }
                                        }}
                                    >
                                        {t("governance.canaryPromote")}
                                    </Button>
                                    <Button
                                        size="sm"
                                        variant="outline"
                                        onClick={async () => {
                                            try {
                                                await v2.rollbackCanary(c.canary_id, "manual rollback", token || "")
                                                setCanaries(await v2.listCanaries(token || ""))
                                                toast.message(t("governance.canaryRolled"))
                                            } catch (err) {
                                                toast.error(err instanceof Error ? err.message : "rollback failed")
                                            }
                                        }}
                                    >
                                        {t("governance.canaryRollback")}
                                    </Button>
                                </div>
                            </div>
                        ))}
                    </div>
                )}
            </section>

            <section className="bg-card/40 border border-border rounded-xl p-6 space-y-4">
                <div className="flex flex-wrap items-center justify-between gap-2">
                    <h3 className="text-xs font-bold font-mono uppercase tracking-widest">
                        {t("governance.tenants")}
                    </h3>
                    <Button size="sm" variant="secondary" onClick={() => void seedPricing()}>
                        {t("governance.seedPricing")}
                    </Button>
                </div>
                <p className="text-[11px] text-muted-foreground font-mono">
                    {t("governance.tenantsHint")}
                </p>
                <div className="flex flex-wrap gap-2 max-w-xl">
                    <Input
                        className="font-mono max-w-[160px]"
                        placeholder="tenant_id"
                        value={newTenantId}
                        onChange={(e) => setNewTenantId(e.target.value)}
                    />
                    <Input
                        className="font-mono max-w-[100px]"
                        placeholder="rps/min"
                        value={newTenantRps}
                        onChange={(e) => setNewTenantRps(e.target.value)}
                    />
                    <Button onClick={() => void createTenant()}>{t("governance.tenantSave")}</Button>
                </div>
                {pricing.length > 0 && (
                    <p className="text-[11px] font-mono text-muted-foreground">
                        pricing: {pricing.map((p) => `${p.price_id}=${p.price_per_1k_input}/${p.price_per_1k_output} ${p.currency}`).join(" · ")}
                    </p>
                )}
                {tenants.length === 0 ? (
                    <p className="text-sm text-muted-foreground">{t("governance.tenantsEmpty")}</p>
                ) : (
                    <div className="space-y-3">
                        {tenants.map((tn) => {
                            const cost = costByTenant[tn.tenant_id]
                            return (
                                <div key={tn.tenant_id} className="border border-border/40 rounded-lg p-4 space-y-2">
                                    <div className="flex flex-wrap gap-2 items-center justify-between">
                                        <div>
                                            <p className="font-mono text-xs font-bold">{tn.tenant_id}</p>
                                            <p className="font-mono text-[11px] text-muted-foreground">
                                                rps={tn.quotas.rps_per_minute ?? "∞"} · conc={tn.quotas.max_concurrency ?? "∞"}
                                                {tn.quotas.allowed_models ? ` · models=${tn.quotas.allowed_models.join("|")}` : ""}
                                            </p>
                                        </div>
                                        <Badge className={cn(
                                            "text-[9px] uppercase font-mono",
                                            tn.enabled
                                                ? "bg-success/10 text-success border-success/20"
                                                : "bg-destructive/10 text-destructive border-destructive/20",
                                        )}>
                                            {tn.enabled ? "enabled" : "disabled"}
                                        </Badge>
                                    </div>
                                    {cost ? (
                                        <p className="text-[11px] font-mono text-muted-foreground">
                                            req={cost.requests} · tok_in={cost.input_tokens} · tok_out={cost.output_tokens}
                                            {" · "}denied={cost.denied_total}
                                            {cost.deny_breakdown.rps ? ` (rps=${cost.deny_breakdown.rps})` : ""}
                                            {cost.deny_breakdown.model ? ` (model=${cost.deny_breakdown.model})` : ""}
                                            {cost.cost_estimate != null
                                                ? ` · cost≈${cost.cost_estimate.toFixed(4)} ${cost.currency || ""}`
                                                : ""}
                                        </p>
                                    ) : (
                                        <p className="text-[11px] text-muted-foreground">{t("governance.noUsage")}</p>
                                    )}
                                    <Button
                                        size="sm"
                                        variant="outline"
                                        onClick={async () => {
                                            try {
                                                await v2.deleteTenant(tn.tenant_id, token || "")
                                                await refresh()
                                            } catch (err) {
                                                toast.error(err instanceof Error ? err.message : "delete failed")
                                            }
                                        }}
                                    >
                                        {t("common.delete")}
                                    </Button>
                                </div>
                            )
                        })}
                    </div>
                )}
            </section>

            <section className="bg-card/40 border border-border rounded-xl overflow-hidden">
                <div className="px-6 py-4 border-b border-border/50 bg-white/5">
                    <h3 className="text-xs font-bold font-mono uppercase tracking-widest">{t("governance.timeline")}</h3>
                </div>
                <Table>
                    <TableHeader>
                        <TableRow className="hover:bg-transparent">
                            <TableHead className="text-[10px] uppercase">Time</TableHead>
                            <TableHead className="text-[10px] uppercase">Kind</TableHead>
                            <TableHead className="text-[10px] uppercase">Summary</TableHead>
                            <TableHead className="text-[10px] uppercase">Model</TableHead>
                        </TableRow>
                    </TableHeader>
                    <TableBody>
                        {events.length === 0 ? (
                            <TableRow>
                                <TableCell colSpan={4} className="text-center text-muted-foreground text-xs py-10">
                                    {t("governance.timelineEmpty")}
                                </TableCell>
                            </TableRow>
                        ) : events.slice(0, 40).map((ev, i) => (
                            <TableRow key={`${ev.ts_ms}-${ev.kind}-${i}`}>
                                <TableCell className="font-mono text-[10px]">
                                    {ev.ts_ms ? new Date(ev.ts_ms).toLocaleString() : "—"}
                                </TableCell>
                                <TableCell><Badge variant="outline" className="text-[9px]">{ev.kind}</Badge></TableCell>
                                <TableCell className="text-xs font-mono">{ev.summary}</TableCell>
                                <TableCell className="font-mono text-xs">{ev.model_uid || "—"}</TableCell>
                            </TableRow>
                        ))}
                    </TableBody>
                </Table>
            </section>
        </div>
    )
}
