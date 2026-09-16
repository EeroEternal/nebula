import { useState } from "react"
import { Plus, Trash2, Box, Play, Square, Loader2, Copy, Check } from "lucide-react"
import { Badge } from "@/components/ui/badge"
import { Button } from "@/components/ui/button"
import { Progress } from "@/components/ui/progress"
import { Table, TableBody, TableCell, TableHead, TableHeader, TableRow } from "@/components/ui/table"
import { Card } from "@/components/ui/card"
import { Select } from "@/components/ui/select"
import { PageShell } from "@/components/layout/page-shell"
import { PageContainer } from "@/components/layout/page-container"
import { PageHeader } from "@/components/layout/page-header"
import { EntityListToolbar } from "@/components/entity-list/EntityListToolbar"
import { LoadModelDialog } from "@/components/LoadModelDialog"
import type { AggregatedModelState } from "@/lib/types"
import { v2 } from "@/lib/api"
import { useI18n } from "@/lib/useI18n"
import { useModels } from "@/hooks/useModels"
import { useAuthStore } from "@/store/useAuthStore"
import { useLoadModelStore } from "@/store/useLoadModelStore"
import { toast } from "sonner"

const STATE_VARIANT: Record<AggregatedModelState, "success" | "secondary" | "warning" | "destructive" | "outline"> = {
    running: "success",
    stopped: "secondary",
    downloading: "outline",
    starting: "warning",
    degraded: "destructive",
    failed: "destructive",
    stopping: "secondary",
}

export function ModelsView() {
    const { t } = useI18n()
    const { token } = useAuthStore()
    const { data: models = [], isLoading: initialLoading, refetch } = useModels()
    const [acting, setActing] = useState<string | null>(null)
    const [filter, setFilter] = useState<AggregatedModelState | "all">("all")
    const [searchQuery, setSearchQuery] = useState("")
    const [copiedModelUid, setCopiedModelUid] = useState<string | null>(null)
    const [selected, setSelected] = useState<string | null>(null)

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
        <PageShell className="overflow-y-auto">
            <PageContainer>
                <PageHeader
                    title={t('models.title')}
                    action={
                        <Button onClick={() => useLoadModelStore.getState().setOpen(true)}>
                            <Plus className="h-4 w-4" />
                            {t('models.loadModel')}
                        </Button>
                    }
                />
                <Card className="gap-0 p-4 sm:p-6">
                    <EntityListToolbar
                        searchValue={searchQuery}
                        onSearchChange={setSearchQuery}
                        searchPlaceholder={t('models.searchPlaceholder')}
                        filters={
                            <Select
                                value={filter}
                                onChange={(v) => setFilter(v as AggregatedModelState | "all")}
                                className="w-40"
                                options={[
                                    { value: "all", label: t('common.all') },
                                    { value: "running", label: t('state.running') },
                                    { value: "stopped", label: t('state.stopped') },
                                    { value: "downloading", label: t('state.downloading') },
                                    { value: "failed", label: t('state.failed') },
                                ]}
                            />
                        }
                        resultCount={t('common.total') + " " + filtered.length}
                    />
                    <Table className="table-fixed">
                        <TableHeader>
                            <TableRow>
                                <TableHead>{t('models.identity')}</TableHead>
                                <TableHead>{t('common.status')}</TableHead>
                                <TableHead>{t('models.provisioning')}</TableHead>
                                <TableHead>{t('models.engine')}</TableHead>
                                <TableHead className="text-right">{t('models.management')}</TableHead>
                            </TableRow>
                        </TableHeader>
                        <TableBody>
                            {initialLoading ? (
                                <TableRow>
                                    <TableCell colSpan={5} className="h-48 text-center text-muted-foreground">
                                        <Loader2 className="mx-auto mb-2 h-5 w-5 animate-spin" />
                                        {t('models.loading')}
                                    </TableCell>
                                </TableRow>
                            ) : filtered.length === 0 ? (
                                <TableRow>
                                    <TableCell colSpan={5} className="h-48 text-center text-muted-foreground">
                                        <Box className="mx-auto mb-2 h-8 w-8 opacity-40" />
                                        {t('models.empty')}
                                    </TableCell>
                                </TableRow>
                            ) : (
                                filtered.map((model) => {
                                    const isActing = acting === model.model_uid
                                    return (
                                        <TableRow
                                            key={model.model_uid}
                                            className={selected === model.model_uid ? "bg-primary/10 font-medium" : "hover:bg-muted/50"}
                                            onClick={() => setSelected(model.model_uid)}
                                        >
                                            <TableCell>
                                                <div className="flex min-w-0 items-center gap-2">
                                                    <span className="truncate">{model.model_uid}</span>
                                                    <button
                                                        type="button"
                                                        className="text-muted-foreground hover:text-foreground"
                                                        onClick={(e) => {
                                                            e.stopPropagation()
                                                            void copyModelName(model.model_uid, model.model_name)
                                                        }}
                                                        aria-label={t('models.copySuccess')}
                                                    >
                                                        {copiedModelUid === model.model_uid ? <Check className="h-3.5 w-3.5 text-success" /> : <Copy className="h-3.5 w-3.5" />}
                                                    </button>
                                                </div>
                                                <p className="truncate text-meta-sm text-muted-foreground">{model.model_name}</p>
                                            </TableCell>
                                            <TableCell>
                                                <Badge variant={STATE_VARIANT[model.state] ?? "secondary"}>{t(`state.${model.state}`)}</Badge>
                                                {model.state === "downloading" ? <Progress value={45} className="mt-2 h-1.5 w-24" /> : null}
                                            </TableCell>
                                            <TableCell className="tabular-nums">
                                                {model.replicas.ready} / {model.replicas.desired}
                                                {model.replicas.unhealthy > 0 ? (
                                                    <Badge variant="destructive" className="ml-2">{model.replicas.unhealthy} {t('models.unhealthy')}</Badge>
                                                ) : null}
                                            </TableCell>
                                            <TableCell><Badge variant="outline">{model.engine_type || "vLLM"}</Badge></TableCell>
                                            <TableCell className="text-right">
                                                <div className="flex justify-end gap-1" onClick={(e) => e.stopPropagation()}>
                                                    {(model.state === "stopped" || model.state === "failed") && (
                                                        <Button variant="ghost" size="icon" aria-label="start" onClick={() => act(model.model_uid, "START", () => v2.startModel(model.model_uid, {}, token || ''))} disabled={isActing}>
                                                            <Play className="h-4 w-4" />
                                                        </Button>
                                                    )}
                                                    {model.state === "running" && (
                                                        <Button variant="ghost" size="icon" aria-label="stop" onClick={() => act(model.model_uid, "STOP", () => v2.stopModel(model.model_uid, token || ''))} disabled={isActing}>
                                                            <Square className="h-4 w-4" />
                                                        </Button>
                                                    )}
                                                    <Button variant="ghost" size="icon" aria-label="delete" disabled={isActing}>
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
                </Card>
                <LoadModelDialog />
            </PageContainer>
        </PageShell>
    )
}
