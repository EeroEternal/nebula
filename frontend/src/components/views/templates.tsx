import { useState } from "react"
import { Layers, Rocket, RefreshCw, Cpu, Layout, Info, Search, Loader2 } from "lucide-react"
import { Badge } from "@/components/ui/badge"
import { Button } from "@/components/ui/button"
import { Input } from "@/components/ui/input"
import { Label } from "@/components/ui/label"
import { Table, TableBody, TableCell, TableHead, TableHeader, TableRow } from "@/components/ui/table"
import { Dialog, DialogContent, DialogHeader, DialogTitle, DialogFooter } from "@/components/ui/dialog"
import { v2 } from "@/lib/api"
import type { ModelTemplate } from "@/lib/types"
import { useI18n } from "@/lib/useI18n"
import { useTemplates } from "@/hooks/useTemplates"
import { useAuthStore } from "@/store/useAuthStore"
import { cn } from "@/lib/utils"
import { toast } from "sonner"
import type { SVGProps } from "react"

const EMPTY_DEPLOY_FORM = {
  model_uid: "",
  replicas: "",
  node_id: "",
  gpu_indices: "",
}

export function TemplatesView() {
  const { t } = useI18n()
  const { token } = useAuthStore()
  const { data: templates = [], isLoading, refetch } = useTemplates()
  
  const [deployDialogOpen, setDeployDialogOpen] = useState(false)
  const [selectedTemplate, setSelectedTemplate] = useState<ModelTemplate | null>(null)
  const [deployForm, setDeployForm] = useState(EMPTY_DEPLOY_FORM)
  const [searchQuery, setSearchQuery] = useState("")

  const openDeploy = (tpl: ModelTemplate) => {
    setSelectedTemplate(tpl)
    setDeployForm({ ...EMPTY_DEPLOY_FORM })
    setDeployDialogOpen(true)
  }

  const handleDeploy = async () => {
    if (!selectedTemplate) return
    const body: Record<string, unknown> = {}
    if (deployForm.model_uid) body.model_uid = deployForm.model_uid
    if (deployForm.replicas) body.replicas = parseInt(deployForm.replicas, 10)
    if (deployForm.node_id) body.node_id = deployForm.node_id
    if (deployForm.gpu_indices) {
      body.gpu_indices = deployForm.gpu_indices
        .split(",")
        .map((s) => parseInt(s.trim(), 10))
        .filter((n) => !isNaN(n))
    }
    
    const promise = v2.deployTemplate(selectedTemplate.template_id, body, token || '')
    toast.promise(promise, {
       loading: t('templates.deploying', { name: selectedTemplate.name }),
      success: () => {
        setDeployDialogOpen(false)
         return t('templates.deploymentStarted')
      },
      error: t('templates.deployFailed')
    })
  }

  const filtered = templates.filter(tpl => 
    tpl.name.toLowerCase().includes(searchQuery.toLowerCase()) || 
    tpl.model_name.toLowerCase().includes(searchQuery.toLowerCase()) ||
    tpl.category?.toLowerCase().includes(searchQuery.toLowerCase())
  )

  return (
    <div className="space-y-8 animate-in fade-in duration-500">
      <div className="flex flex-col md:flex-row md:items-end justify-between gap-4">
        <div>
          <h2 className="text-3xl font-bold tracking-tight font-mono uppercase text-foreground">{t('templates.title')}</h2>
          <p className="text-muted-foreground mt-2 flex items-center gap-2">
            <Layers className="h-4 w-4 text-primary" />
            {t('templates.subtitle')}
          </p>
        </div>
        <div className="flex gap-3">
          <div className="relative w-64 group">
            <Search className="absolute left-3 top-1/2 -translate-y-1/2 h-4 w-4 text-muted-foreground group-focus-within:text-primary transition-colors" />
            <input
              type="text"
               placeholder={t('templates.filterPlaceholder')}
              value={searchQuery}
              onChange={(e) => setSearchQuery(e.target.value)}
              className="w-full bg-black/20 border border-border/50 rounded-lg pl-10 pr-4 py-2 text-xs font-mono focus:outline-none focus:border-primary/50 transition-all"
            />
          </div>
          <Button
            variant="outline"
            size="sm"
            onClick={() => refetch()}
            className="h-10 px-4 bg-white/5 border-border/50 font-mono text-[10px] uppercase tracking-widest"
          >
            <RefreshCw className={cn("h-3.5 w-3.5 mr-2", isLoading ? "animate-spin" : "")} />
            {t('common.refresh')}
          </Button>
        </div>
      </div>

      <div className="bg-card/40 backdrop-blur-xl border border-border rounded-xl overflow-hidden">
        <div className="px-6 py-4 border-b border-border/50 flex items-center justify-between bg-white/5">
          <h3 className="text-xs font-bold font-mono uppercase tracking-widest text-muted-foreground">
            {t('templates.available')}
          </h3>
          <Badge variant="outline" className="font-mono text-[10px] border-primary/20 text-primary uppercase">
            {templates.length} {t('common.total')}
          </Badge>
        </div>

        <Table>
          <TableHeader className="bg-black/20">
            <TableRow className="border-border/50 hover:bg-transparent">
               <TableHead className="text-[10px] uppercase font-bold text-muted-foreground px-6 py-4">{t('templates.protocolIdentity')}</TableHead>
               <TableHead className="text-[10px] uppercase font-bold text-muted-foreground">{t('templates.baseModel')}</TableHead>
               <TableHead className="text-[10px] uppercase font-bold text-muted-foreground">{t('templates.infrastructure')}</TableHead>
               <TableHead className="text-[10px] uppercase font-bold text-muted-foreground">{t('templates.category')}</TableHead>
               <TableHead className="text-[10px] uppercase font-bold text-muted-foreground">{t('templates.source')}</TableHead>
               <TableHead className="text-right text-[10px] uppercase font-bold text-muted-foreground pr-6">{t('templates.operations')}</TableHead>
            </TableRow>
          </TableHeader>
          <TableBody>
            {isLoading && templates.length === 0 ? (
                <TableRow>
                    <TableCell colSpan={6} className="h-64 text-center">
                        <div className="flex flex-col items-center gap-3 opacity-50">
                            <Loader2 className="h-8 w-8 animate-spin text-primary" />
                            <p className="text-[10px] font-mono uppercase tracking-widest">{t('templates.loading')}</p>
                        </div>
                    </TableCell>
                </TableRow>
            ) : filtered.length === 0 ? (
              <TableRow>
                <TableCell colSpan={6} className="h-64 text-center">
                  <div className="flex flex-col items-center justify-center opacity-30 gap-3">
                    <Layers className="h-12 w-12" />
                    <p className="text-[10px] font-mono uppercase tracking-widest">
                      {t('templates.noData')}
                    </p>
                  </div>
                </TableCell>
              </TableRow>
            ) : (
              filtered.map((tpl) => (
                <TableRow key={tpl.template_id} className="border-border/40 hover:bg-white/5 transition-colors group">
                  <TableCell className="px-6 py-5">
                    <div className="flex flex-col gap-1.5">
                      <span className="font-mono text-sm font-bold group-hover:text-primary transition-colors">{tpl.name}</span>
                      {tpl.description && (
                        <span className="text-[10px] text-muted-foreground/60 font-mono italic truncate max-w-[200px]">
                          {tpl.description}
                        </span>
                      )}
                    </div>
                  </TableCell>
                  <TableCell>
                    <div className="flex flex-col gap-1.5">
                        <span className="font-mono text-[11px] bg-black/20 px-2 py-1 rounded border border-border/30 max-w-[240px] truncate">
                        {tpl.model_name}
                        </span>
                    </div>
                  </TableCell>
                  <TableCell>
                    <div className="flex flex-col gap-1.5">
                        <Badge variant="outline" className="font-mono text-[10px] border-border/50 text-muted-foreground uppercase w-fit">
                            {tpl.engine_type || "AUTO"}
                        </Badge>
                        <span className="text-[9px] font-mono text-muted-foreground uppercase tracking-widest">
                             {t('templates.defaultScale')}: {tpl.default_replicas}
                        </span>
                    </div>
                  </TableCell>
                  <TableCell>
                    {tpl.category ? (
                      <Badge variant="secondary" className="font-mono text-[9px] uppercase px-2 py-0.5 bg-white/5 text-muted-foreground border-border/30">
                        {tpl.category}
                      </Badge>
                    ) : (
                      <span className="text-[10px] font-mono text-muted-foreground/30">—</span>
                    )}
                  </TableCell>
                  <TableCell>
                    <Badge
                      className={cn("text-[9px] font-bold uppercase px-2 py-0.5", 
                          tpl.source === 'system' ? "bg-primary/10 text-primary border-primary/20" : "bg-white/5 text-muted-foreground border-border/30")}
                      variant="outline"
                    >
                      {tpl.source}
                    </Badge>
                  </TableCell>
                  <TableCell className="text-right pr-6">
                    <Button
                      variant="ghost"
                      size="sm"
                      onClick={() => openDeploy(tpl)}
                      className="h-9 px-4 hover:bg-primary/20 hover:text-primary font-bold text-[10px] uppercase tracking-widest border border-transparent hover:border-primary/30 transition-all"
                    >
                      <Rocket className="h-3.5 w-3.5 mr-2" />
                      {t('templates.deploy')}
                    </Button>
                  </TableCell>
                </TableRow>
              ))
            )}
          </TableBody>
        </Table>
      </div>

      <Dialog open={deployDialogOpen} onOpenChange={setDeployDialogOpen}>
        <DialogContent className="sm:max-w-[500px] bg-card/95 backdrop-blur-2xl border-border rim-light">
          <DialogHeader>
            <DialogTitle className="font-mono uppercase tracking-tight text-2xl flex items-center gap-3">
              <Rocket className="h-6 w-6 text-primary animate-signal" />
               {t('templates.deployTitle', { name: selectedTemplate?.name || '' })}
            </DialogTitle>
          </DialogHeader>

          <div className="space-y-6 py-4">
            <div className="space-y-2">
              <Label htmlFor="deploy-uid" className="text-[10px] uppercase font-bold tracking-widest text-muted-foreground">
                 {t('templates.instanceIdentity')}
                 <span className="text-muted-foreground/50 font-normal ml-2">({t('templates.optionalOverrideLabel')})</span>
              </Label>
              <Input
                id="deploy-uid"
                className="bg-white/5 border-border/50 font-mono"
                 placeholder={t('templates.leaveEmptyAuto')}
                value={deployForm.model_uid}
                onChange={(e) => setDeployForm({ ...deployForm, model_uid: e.target.value })}
              />
            </div>

            <div className="grid grid-cols-2 gap-4">
              <div className="space-y-2">
                 <Label htmlFor="deploy-replicas" className="text-[10px] uppercase font-bold tracking-widest text-muted-foreground">{t('templates.targetScale')}</Label>
                <div className="relative">
                    <Layout className="absolute left-3 top-1/2 -translate-y-1/2 h-4 w-4 text-muted-foreground" />
                    <Input
                        id="deploy-replicas"
                        type="number"
                        className="pl-10 bg-white/5 border-border/50 font-mono"
                        placeholder={String(selectedTemplate?.default_replicas ?? 1)}
                        value={deployForm.replicas}
                        onChange={(e) => setDeployForm({ ...deployForm, replicas: e.target.value })}
                    />
                </div>
              </div>

              <div className="space-y-2">
                 <Label htmlFor="deploy-node" className="text-[10px] uppercase font-bold tracking-widest text-muted-foreground">{t('templates.targetNode')}</Label>
                <div className="relative">
                    <Server className="absolute left-3 top-1/2 -translate-y-1/2 h-4 w-4 text-muted-foreground" />
                    <Input
                        id="deploy-node"
                        className="pl-10 bg-white/5 border-border/50 font-mono"
                         placeholder={t('templates.autoScheduling')}
                        value={deployForm.node_id}
                        onChange={(e) => setDeployForm({ ...deployForm, node_id: e.target.value })}
                    />
                </div>
              </div>
            </div>

            <div className="space-y-2">
              <Label htmlFor="deploy-gpus" className="text-[10px] uppercase font-bold tracking-widest text-muted-foreground">
                 {t('templates.gpuAffinity')}
                 <span className="text-muted-foreground/50 font-normal ml-2">({t('templates.commaSeparatedIndices')})</span>
              </Label>
              <div className="relative">
                  <Cpu className="absolute left-3 top-1/2 -translate-y-1/2 h-4 w-4 text-muted-foreground" />
                  <Input
                    id="deploy-gpus"
                    className="pl-10 bg-white/5 border-border/50 font-mono"
                     placeholder={t('templates.gpuExample')}
                    value={deployForm.gpu_indices}
                    onChange={(e) => setDeployForm({ ...deployForm, gpu_indices: e.target.value })}
                  />
              </div>
            </div>
            
            <div className="p-4 rounded-lg bg-primary/5 border border-primary/10 flex gap-3">
                <Info className="h-4 w-4 text-primary shrink-0 mt-0.5" />
                <p className="text-[10px] text-muted-foreground uppercase leading-relaxed tracking-wider">
                     {t('templates.deployNotice')}
                </p>
            </div>
          </div>

          <DialogFooter className="bg-black/20 -mx-6 -mb-6 p-6 mt-4">
            <Button
              variant="ghost"
              onClick={() => setDeployDialogOpen(false)}
              className="font-mono text-[10px] uppercase tracking-widest text-muted-foreground hover:text-foreground"
            >
              {t('common.cancel')}
            </Button>
            <Button
              onClick={handleDeploy}
              className="bg-primary text-primary-foreground rim-light h-10 px-8 font-bold uppercase tracking-widest text-xs ml-auto"
            >
              <Rocket className="h-4 w-4 mr-2" />
              {t('templates.deploy')}
            </Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>
    </div>
  )
}

function Server(props: SVGProps<SVGSVGElement>) {
  return (
    <svg
      {...props}
      xmlns="http://www.w3.org/2000/svg"
      width="24"
      height="24"
      viewBox="0 0 24 24"
      fill="none"
      stroke="currentColor"
      strokeWidth="2"
      strokeLinecap="round"
      strokeLinejoin="round"
    >
      <rect width="20" height="8" x="2" y="2" rx="2" ry="2" />
      <rect width="20" height="8" x="2" y="14" rx="2" ry="2" />
      <line x1="6" x2="6.01" y1="6" y2="6" />
      <line x1="6" x2="6.01" y1="18" y2="18" />
    </svg>
  )
}
