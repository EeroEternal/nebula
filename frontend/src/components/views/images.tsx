import { useState } from "react"
import { Plus, Trash2, Container, RefreshCw, CheckCircle2, XCircle, Loader2, Clock, ChevronRight, ChevronDown, Edit2 } from "lucide-react"
import { Badge } from "@/components/ui/badge"
import { Button } from "@/components/ui/button"
import { Input } from "@/components/ui/input"
import { Label } from "@/components/ui/label"
import { Table, TableBody, TableCell, TableHead, TableHeader, TableRow } from "@/components/ui/table"
import { Dialog, DialogContent, DialogHeader, DialogTitle, DialogFooter } from "@/components/ui/dialog"
import { apiPut, apiDelete } from "@/lib/api"
import type { EngineImage } from "@/lib/types"
import { useI18n } from "@/lib/useI18n"
import { useImages } from "@/hooks/useImages"
import { useAuthStore } from "@/store/useAuthStore"
import { cn } from "@/lib/utils"
import { toast } from "sonner"

const StatusIcon = ({ status }: { status: string }) => {
  switch (status) {
    case "ready":
      return <CheckCircle2 className="h-3.5 w-3.5 text-success" />
    case "pulling":
      return <Loader2 className="h-3.5 w-3.5 text-primary animate-spin" />
    case "failed":
      return <XCircle className="h-3.5 w-3.5 text-destructive" />
    default:
      return <Clock className="h-3.5 w-3.5 text-muted-foreground" />
  }
}

const EMPTY_FORM: Omit<EngineImage, "created_at_ms" | "updated_at_ms"> = {
  id: "",
  engine_type: "vllm",
  image: "",
  platforms: [],
  version_policy: "pin",
  pre_pull: true,
  description: "",
}

export function ImagesView() {
  const { t } = useI18n()
  const { token } = useAuthStore()
  const { data, isLoading, refetch } = useImages()
  const images = data?.images || []
  const statuses = data?.statuses || []

  const [dialogOpen, setDialogOpen] = useState(false)
  const [editingId, setEditingId] = useState<string | null>(null)
  const [form, setForm] = useState(EMPTY_FORM)
  const [platformInput, setPlatformInput] = useState("")
  const [expandedImage, setExpandedImage] = useState<string | null>(null)

  const openCreate = () => {
    setEditingId(null)
    setForm({ ...EMPTY_FORM })
    setPlatformInput("")
    setDialogOpen(true)
  }

  const openEdit = (img: EngineImage) => {
    setEditingId(img.id)
    setForm({
      id: img.id,
      engine_type: img.engine_type,
      image: img.image,
      platforms: img.platforms,
      version_policy: img.version_policy,
      pre_pull: img.pre_pull,
      description: img.description || "",
    })
    setPlatformInput(img.platforms.join(", "))
    setDialogOpen(true)
  }

  const handleSave = async () => {
    if (!form.id || !form.image) return
    const payload = {
      ...form,
      platforms: platformInput
        .split(",")
        .map((s) => s.trim())
        .filter(Boolean),
      created_at_ms: 0,
      updated_at_ms: 0,
    }
    const promise = apiPut(`/images/${form.id}`, payload, token || '')
    toast.promise(promise, {
      loading: t('images.saving'),
      success: () => {
        setDialogOpen(false)
        refetch()
        return t('images.saved')
      },
      error: t('images.saveFailed')
    })
  }

  const handleDelete = async (id: string) => {
    const promise = apiDelete(`/images/${id}`, token || '')
    toast.promise(promise, {
      loading: t('images.deleting'),
      success: () => {
        refetch()
        return t('images.deleted')
      },
      error: t('images.deleteFailed')
    })
  }

  const getImageNodeStatuses = (imageId: string) =>
    statuses.filter((s) => s.image_id === imageId)

  return (
    <div className="space-y-8 animate-in fade-in duration-500">
      <div className="flex flex-col md:flex-row md:items-end justify-between gap-4">
        <div>
          <h2 className="text-3xl font-bold tracking-tight font-mono uppercase text-foreground">{t('images.title')}</h2>
          <p className="text-muted-foreground mt-2 flex items-center gap-2">
            <Container className="h-4 w-4 text-primary" />
            {t('images.subtitle')}
          </p>
        </div>
        <div className="flex gap-3">
          <Button
            variant="outline"
            size="sm"
            onClick={() => refetch()}
            className="h-11 px-4 bg-white/5 border-border/50 font-mono text-[10px] uppercase tracking-widest"
          >
            <RefreshCw className={cn("h-3.5 w-3.5 mr-2", isLoading ? "animate-spin" : "")} />
            {t('common.refresh')}
          </Button>
          <Button
            onClick={openCreate}
            className="bg-primary text-primary-foreground rim-light h-11 px-6 font-bold uppercase tracking-widest text-xs"
          >
            <Plus className="mr-2 h-4 w-4" />
            {t('images.register')}
          </Button>
        </div>
      </div>

      <div className="bg-card/40 backdrop-blur-xl border border-border rounded-xl overflow-hidden">
        <div className="px-6 py-4 border-b border-border/50 flex items-center justify-between bg-white/5">
          <div className="flex items-center gap-2">
            <h3 className="text-xs font-bold font-mono uppercase tracking-widest text-muted-foreground">
              {t('images.registered')}
            </h3>
          </div>
          <Badge variant="outline" className="font-mono text-[10px] border-primary/20 text-primary uppercase">
            {images.length} {t('common.total')}
          </Badge>
        </div>

        <Table>
          <TableHeader className="bg-black/20">
            <TableRow className="border-border/50 hover:bg-transparent">
               <TableHead className="text-[10px] uppercase font-bold text-muted-foreground px-6 py-4">{t('images.tableImageIdentity')}</TableHead>
               <TableHead className="text-[10px] uppercase font-bold text-muted-foreground">{t('images.engineType')}</TableHead>
               <TableHead className="text-[10px] uppercase font-bold text-muted-foreground">{t('images.dockerReference')}</TableHead>
               <TableHead className="text-[10px] uppercase font-bold text-muted-foreground">{t('images.nodeDistribution')}</TableHead>
               <TableHead className="text-right text-[10px] uppercase font-bold text-muted-foreground pr-6">{t('common.actions')}</TableHead>
            </TableRow>
          </TableHeader>
          <TableBody>
            {isLoading && images.length === 0 ? (
                <TableRow>
                    <TableCell colSpan={5} className="h-64 text-center">
                        <div className="flex flex-col items-center gap-3 opacity-50">
                            <Loader2 className="h-8 w-8 animate-spin text-primary" />
                            <p className="text-[10px] font-mono uppercase tracking-widest">{t('images.loading')}</p>
                        </div>
                    </TableCell>
                </TableRow>
            ) : images.length === 0 ? (
              <TableRow>
                <TableCell colSpan={5} className="h-64 text-center">
                  <div className="flex flex-col items-center justify-center opacity-30 gap-3">
                    <Container className="h-12 w-12" />
                    <p className="text-[10px] font-mono uppercase tracking-widest">
                      {t('images.noData')}
                    </p>
                  </div>
                </TableCell>
              </TableRow>
            ) : (
              images.map((img) => {
                const ns = getImageNodeStatuses(img.id)
                const readyCount = ns.filter((s) => s.status === "ready").length
                const totalCount = ns.length
                const isExpanded = expandedImage === img.id

                return (
                  <>
                  <TableRow key={img.id} className={cn("border-border/40 hover:bg-white/5 transition-colors group", isExpanded ? "bg-white/5" : "")}>
                    <TableCell className="px-6 py-5">
                      <div className="flex flex-col gap-1">
                        <span className="font-mono text-sm font-bold group-hover:text-primary transition-colors cursor-pointer flex items-center gap-2"
                              onClick={() => setExpandedImage(isExpanded ? null : img.id)}>
                          {isExpanded ? <ChevronDown className="h-3 w-3" /> : <ChevronRight className="h-3 w-3" />}
                          {img.id}
                        </span>
                        {img.description && (
                          <span className="text-[10px] text-muted-foreground/60 font-mono italic px-5">
                            {img.description}
                          </span>
                        )}
                      </div>
                    </TableCell>
                    <TableCell>
                      <Badge variant="outline" className="font-mono text-[10px] border-border/50 text-muted-foreground uppercase">
                        {img.engine_type}
                      </Badge>
                    </TableCell>
                    <TableCell>
                      <div className="flex flex-col gap-1.5">
                        <span className="font-mono text-[11px] bg-black/20 px-2 py-1 rounded border border-border/30 max-w-[320px] truncate select-all">
                          {img.image}
                        </span>
                        <div className="flex items-center gap-2">
                           <Badge variant={img.version_policy === "rolling" ? "default" : "secondary"} className="text-[9px] h-4 uppercase px-1.5">
                             {img.version_policy}
                           </Badge>
                            {img.pre_pull && <span className="text-[9px] font-mono text-primary/70 uppercase tracking-tighter">● {t('images.autoPull')}</span>}
                        </div>
                      </div>
                    </TableCell>
                    <TableCell>
                      <button
                        onClick={() => setExpandedImage(isExpanded ? null : img.id)}
                        className="flex items-center gap-2 hover:opacity-80 transition-opacity"
                      >
                        <div className="flex -space-x-1.5">
                             {ns.slice(0, 3).map((s, i) => (
                                 <div key={i} className={cn("w-5 h-5 rounded-full border-2 border-background flex items-center justify-center text-[8px] font-bold", 
                                     s.status === 'ready' ? "bg-success text-success-foreground" : "bg-warning text-warning-foreground")}>
                                     {s.node_id[0].toUpperCase()}
                                 </div>
                             ))}
                             {totalCount > 3 && (
                                 <div className="w-5 h-5 rounded-full border-2 border-background bg-muted flex items-center justify-center text-[8px] font-bold">
                                     +{totalCount - 3}
                                 </div>
                             )}
                        </div>
                        <span className="text-[10px] font-mono font-bold">
                           <span className="text-success">{readyCount}</span>
                            <span className="text-muted-foreground">/{totalCount} {t('images.ok')}</span>
                        </span>
                      </button>
                    </TableCell>
                    <TableCell className="text-right pr-6">
                      <div className="flex justify-end gap-2">
                        <Button
                          variant="ghost"
                          size="sm"
                          onClick={() => openEdit(img)}
                          className="h-8 w-8 p-0 hover:bg-white/10"
                        >
                          <Edit2 className="h-4 w-4" />
                        </Button>
                        <Button
                          variant="ghost"
                          size="sm"
                          onClick={() => handleDelete(img.id)}
                          className="h-8 w-8 p-0 hover:bg-destructive/20 hover:text-destructive"
                        >
                          <Trash2 className="h-4 w-4" />
                        </Button>
                      </div>
                    </TableCell>
                  </TableRow>
                  {isExpanded && (
                      <TableRow className="bg-black/10 border-none">
                          <TableCell colSpan={5} className="px-12 py-4">
                              <div className="grid grid-cols-1 sm:grid-cols-2 lg:grid-cols-3 gap-3">
                                  {ns.length > 0 ? ns.map((s) => (
                                    <div key={s.node_id} className="flex items-center justify-between p-3 rounded-lg border border-border/30 bg-card/20 backdrop-blur-sm">
                                        <div className="flex items-center gap-3">
                                            <StatusIcon status={s.status} />
                                            <span className="text-[11px] font-mono font-bold uppercase tracking-widest">{s.node_id}</span>
                                        </div>
                                        <span className={cn("text-[10px] font-bold uppercase tracking-tighter", 
                                            s.status === 'ready' ? "text-success" : s.status === 'failed' ? "text-destructive" : "text-primary")}>
                                            {s.status}
                                        </span>
                                    </div>
                                  )) : (
                                    <div className="col-span-full py-4 text-center">
                                         <p className="text-[10px] font-mono text-muted-foreground uppercase">{t('images.noDistribution')}</p>
                                    </div>
                                  )}
                              </div>
                          </TableCell>
                      </TableRow>
                  )}
                  </>
                )
              })
            )}
          </TableBody>
        </Table>
      </div>

      <Dialog open={dialogOpen} onOpenChange={setDialogOpen}>
        <DialogContent className="sm:max-w-[500px] bg-card/95 backdrop-blur-2xl border-border rim-light">
          <DialogHeader>
            <DialogTitle className="font-mono uppercase tracking-tight text-2xl">
               {editingId ? t('images.updateTitle') : t('images.registerTitle')}
            </DialogTitle>
          </DialogHeader>

          <div className="space-y-6 py-4">
            <div className="grid grid-cols-2 gap-4">
                <div className="space-y-2">
                    <Label htmlFor="img-id" className="text-[10px] uppercase font-bold tracking-widest text-muted-foreground">{t('images.imageId')}</Label>
                    <Input
                        id="img-id"
                        className="bg-white/5 border-border/50 font-mono"
                         placeholder={t('images.imageIdPlaceholder')}
                        value={form.id}
                        onChange={(e) => setForm({ ...form, id: e.target.value })}
                        disabled={!!editingId}
                    />
                </div>
                <div className="space-y-2">
                    <Label htmlFor="img-engine" className="text-[10px] uppercase font-bold tracking-widest text-muted-foreground">{t('images.engineType')}</Label>
                    <select
                        id="img-engine"
                        value={form.engine_type}
                        onChange={(e) => setForm({ ...form, engine_type: e.target.value })}
                        className="flex h-10 w-full rounded-md border border-border/50 bg-white/5 px-3 py-1 text-sm font-mono focus:outline-none focus:ring-1 focus:ring-primary/30"
                    >
                        <option value="vllm">vLLM</option>
                        <option value="sglang">SGLang</option>
                        <option value="xllm">xLLM</option>
                    </select>
                </div>
            </div>

            <div className="space-y-2">
                <Label htmlFor="img-ref" className="text-[10px] uppercase font-bold tracking-widest text-muted-foreground">{t('images.dockerImage')}</Label>
                <div className="relative">
                    <Container className="absolute left-3 top-1/2 -translate-y-1/2 h-4 w-4 text-muted-foreground" />
                    <Input
                        id="img-ref"
                        className="pl-10 bg-white/5 border-border/50 font-mono text-xs"
                         placeholder={t('images.dockerPlaceholder')}
                        value={form.image}
                        onChange={(e) => setForm({ ...form, image: e.target.value })}
                    />
                </div>
            </div>

            <div className="grid grid-cols-2 gap-4">
                <div className="space-y-2">
                    <Label htmlFor="img-policy" className="text-[10px] uppercase font-bold tracking-widest text-muted-foreground">{t('images.versionPolicy')}</Label>
                    <select
                        id="img-policy"
                        value={form.version_policy}
                        onChange={(e) => setForm({ ...form, version_policy: e.target.value as "pin" | "rolling" })}
                        className="flex h-10 w-full rounded-md border border-border/50 bg-white/5 px-3 py-1 text-sm font-mono focus:outline-none focus:ring-1 focus:ring-primary/30"
                    >
                         <option value="pin">{t('images.pinStable')}</option>
                         <option value="rolling">{t('images.rollingLatest')}</option>
                    </select>
                </div>
                <div className="space-y-2">
                     <Label htmlFor="img-platforms" className="text-[10px] uppercase font-bold tracking-widest text-muted-foreground">{t('images.platformsLabel')}</Label>
                    <Input
                        id="img-platforms"
                        className="bg-white/5 border-border/50 font-mono text-xs"
                         placeholder={t('images.platformPlaceholder')}
                        value={platformInput}
                        onChange={(e) => setPlatformInput(e.target.value)}
                    />
                </div>
            </div>

            <div className="space-y-2">
                 <Label htmlFor="img-desc" className="text-[10px] uppercase font-bold tracking-widest text-muted-foreground">{t('images.description')}</Label>
                <Input
                    id="img-desc"
                    className="bg-white/5 border-border/50"
                     placeholder={t('images.descriptionPlaceholder')}
                    value={form.description || ""}
                    onChange={(e) => setForm({ ...form, description: e.target.value })}
                />
            </div>

            <div className="flex items-center gap-3 p-3 rounded-lg border border-border/30 bg-white/5">
                <input
                    type="checkbox"
                    id="img-prepull"
                    checked={form.pre_pull}
                    onChange={(e) => setForm({ ...form, pre_pull: e.target.checked })}
                    className="h-4 w-4 rounded border-border bg-black/20 text-primary focus:ring-primary/30"
                />
                <div className="space-y-0.5">
                     <Label htmlFor="img-prepull" className="text-xs font-bold uppercase tracking-tight">{t('images.prePull')}</Label>
                     <p className="text-[10px] text-muted-foreground uppercase tracking-widest">{t('images.prePullHint')}</p>
                </div>
            </div>
          </div>

          <DialogFooter className="bg-black/20 -mx-6 -mb-6 p-6 mt-4">
            <Button
              variant="ghost"
              onClick={() => setDialogOpen(false)}
              className="font-mono text-[10px] uppercase tracking-widest text-muted-foreground hover:text-foreground"
            >
              {t('common.cancel')}
            </Button>
            <Button
              onClick={handleSave}
              disabled={!form.id || !form.image}
              className="bg-primary text-primary-foreground rim-light h-10 px-6 font-bold uppercase tracking-widest text-xs ml-auto"
            >
               {editingId ? t('images.update') : t('images.commit')}
            </Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>
    </div>
  )
}
