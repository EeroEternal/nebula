import { useEffect, useState } from "react"
import { Button } from "@/components/ui/button"
import {
    Dialog,
    DialogContent,
    DialogFooter,
    DialogHeader,
    DialogTitle,
} from "@/components/ui/dialog"
import { Input } from "@/components/ui/input"
import { v2 } from "@/lib/api"
import type { ReplicaPlacementSpec } from "@/lib/types"
import { useI18n } from "@/lib/useI18n"

type Role = "prefill" | "decode"

export interface DeployDialogProps {
    modelUid: string
    replicasDefault?: number
    token?: string
    open: boolean
    onOpenChange: (open: boolean) => void
    onStarted?: () => void
}

/**
 * Start-deployment dialog. pd_split (pool.md §pd_split) needs a role per
 * replica plus the shared-KV store address; the API rejects incomplete
 * combinations, so the dialog mirrors those constraints up front.
 */
export function DeployDialog({
    modelUid,
    replicasDefault = 2,
    token,
    open,
    onOpenChange,
    onStarted,
}: DeployDialogProps) {
    const { t } = useI18n()
    const [replicas, setReplicas] = useState(replicasDefault)
    const [topology, setTopology] = useState<"replicated" | "pd_split">("replicated")
    const [topologyTouched, setTopologyTouched] = useState(false)
    const [roles, setRoles] = useState<Role[]>(Array.from({ length: replicasDefault }, () => "mixed" as unknown as Role))
    const [baseReplicaSpecs, setBaseReplicaSpecs] = useState<ReplicaPlacementSpec[]>([])
    const [remoteUrl, setRemoteUrl] = useState("lm://127.0.0.1:6800")
    const [kvSerde, setKvSerde] = useState("naive")
    const [hydrating, setHydrating] = useState(false)
    const [busy, setBusy] = useState(false)
    const [error, setError] = useState<string | null>(null)

    useEffect(() => {
        if (!open || !modelUid) return
        let cancelled = false
        setError(null)
        setTopologyTouched(false)
        setHydrating(true)
        v2.getModel(modelUid, token)
            .then((detail) => {
                if (cancelled) return
                const dep = detail.deployment
                const repCount = Math.max(1, Math.min(16, dep?.replicas || replicasDefault || 2))
                setReplicas(repCount)
                setBaseReplicaSpecs(dep?.replica_specs || [])
                if (dep?.topology === "pd_split") {
                    setTopology("pd_split")
                    setRemoteUrl(dep.kv_sharing?.remote_url || "lm://127.0.0.1:6800")
                    setKvSerde(dep.kv_sharing?.serde || "naive")
                    const hydratedRoles: Role[] = Array.from({ length: repCount }, (_, idx) => {
                        const r = dep.replica_specs?.[idx]?.role
                        return r === "prefill" || r === "decode"
                            ? r
                            : ("mixed" as unknown as Role)
                    })
                    setRoles(hydratedRoles)
                } else {
                    setTopology("replicated")
                    setRemoteUrl("lm://127.0.0.1:6800")
                    setKvSerde("naive")
                    setRoles(Array.from({ length: repCount }, () => "mixed" as unknown as Role))
                }
            })
            .catch(() => {
                // Keep defaults if model detail lookup fails
            })
            .finally(() => {
                if (!cancelled) setHydrating(false)
            })
        return () => {
            cancelled = true
        }
    }, [open, modelUid, replicasDefault, token])

    const resize = (n: number) => {
        const next = Math.max(1, Math.min(16, n || 1))
        setReplicas(next)
        setRoles((prev) => {
            const out = prev.slice(0, next)
            while (out.length < next) out.push("mixed" as unknown as Role)
            return out
        })
    }

    const switchTopology = (tp: "replicated" | "pd_split") => {
        setTopology(tp)
        setTopologyTouched(true)
        setError(null)
        if (tp === "replicated") {
            setRoles(Array.from({ length: replicas }, () => "mixed" as unknown as Role))
            setRemoteUrl("lm://127.0.0.1:6800")
            setKvSerde("naive")
        }
    }

    const hasPrefill = roles.slice(0, replicas).includes("prefill")
    const hasDecode = roles.slice(0, replicas).includes("decode")
    const pdSplitReady = hasPrefill && hasDecode

    const submit = async () => {
        setBusy(true)
        setError(null)
        try {
            const body: Record<string, unknown> = { replicas }
            if (topology === "pd_split") {
                body.topology = "pd_split"
                body.kv_sharing = { remote_url: remoteUrl, serde: kvSerde || "naive" }
                body.replica_specs = roles.slice(0, replicas).map((role, idx) => ({
                    ...(baseReplicaSpecs[idx] || {}),
                    role,
                }))
            } else if (topologyTouched) {
                body.topology = "replicated"
            }
            await v2.startModel(modelUid, body, token)
            onStarted?.()
            onOpenChange(false)
        } catch (e) {
            setError(e instanceof Error ? e.message : String(e))
        } finally {
            setBusy(false)
        }
    }

    return (
        <Dialog open={open} onOpenChange={onOpenChange}>
            <DialogContent className="max-h-[85vh] overflow-y-auto">
                <DialogHeader>
                    <DialogTitle>{t('deploy.title')}</DialogTitle>
                </DialogHeader>
                <div className="space-y-4">
                    <div className="space-y-1">
                        <label className="text-sm font-medium">{t('deploy.replicas')}</label>
                        <Input
                            type="number"
                            min={1}
                            max={16}
                            value={replicas}
                            onChange={(e) => resize(Number(e.target.value))}
                        />
                    </div>
                    <div className="space-y-1">
                        <label className="text-sm font-medium">{t('deploy.topology')}</label>
                        <div className="flex gap-2">
                            {(["replicated", "pd_split"] as const).map((tp) => (
                                <Button
                                    key={tp}
                                    type="button"
                                    variant={topology === tp ? "default" : "outline"}
                                    size="sm"
                                    onClick={() => switchTopology(tp)}
                                >
                                    {tp === "replicated" ? t('deploy.replicated') : t('deploy.pdSplit')}
                                </Button>
                            ))}
                        </div>
                    </div>

                    {topology === "pd_split" ? (
                        <>
                            <div className="space-y-1">
                                <label className="text-sm font-medium">{t('deploy.roles')}</label>
                                <div className="space-y-2">
                                    {Array.from({ length: replicas }).map((_, i) => (
                                        <div key={i} className="flex items-center gap-3">
                                            <span className="w-24 text-meta-sm text-muted-foreground">
                                                {t('deploy.replica')} {i}
                                            </span>
                                            <div className="flex gap-2">
                                                {(["prefill", "decode"] as const).map((r) => (
                                                    <Button
                                                        key={r}
                                                        type="button"
                                                        size="sm"
                                                        variant={roles[i] === r ? "default" : "outline"}
                                                        onClick={() =>
                                                            setRoles((prev) => {
                                                                const out = [...prev]
                                                                out[i] = r
                                                                return out
                                                            })
                                                        }
                                                    >
                                                        {r === "prefill" ? t('deploy.prefill') : t('deploy.decode')}
                                                    </Button>
                                                ))}
                                            </div>
                                        </div>
                                    ))}
                                </div>
                            </div>
                            <div className="space-y-1">
                                <label className="text-sm font-medium">{t('deploy.sharedKv')}</label>
                                <Input
                                    value={remoteUrl}
                                    onChange={(e) => setRemoteUrl(e.target.value)}
                                    placeholder="lm://host:6800"
                                />
                            </div>
                            {!pdSplitReady && (
                                <p className="text-meta-sm text-destructive">{t('deploy.needBothRoles')}</p>
                            )}
                            <p className="text-meta-sm text-muted-foreground">{t('deploy.imageGateHint')}</p>
                        </>
                    ) : null}

                    {error && <p className="text-meta-sm text-destructive">{error}</p>}
                </div>
                <DialogFooter>
                    <Button variant="outline" onClick={() => onOpenChange(false)} disabled={busy}>
                        {t('common.cancel')}
                    </Button>
                    <Button
                        onClick={submit}
                        disabled={busy || hydrating || (topology === "pd_split" && !pdSplitReady)}
                    >
                        {busy ? t('deploy.starting') : t('deploy.start')}
                    </Button>
                </DialogFooter>
            </DialogContent>
        </Dialog>
    )
}
