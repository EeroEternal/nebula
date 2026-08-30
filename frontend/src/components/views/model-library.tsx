import { useCallback, useState } from 'react'
import { FolderInput, Loader2, RefreshCw, Trash2, Box, Database, HardDrive, Info, ArrowUpRight, Search } from 'lucide-react'
import { Badge } from '@/components/ui/badge'
import { Button } from '@/components/ui/button'
import { Dialog, DialogContent, DialogFooter, DialogHeader, DialogTitle } from '@/components/ui/dialog'
import { Input } from '@/components/ui/input'
import { Label } from '@/components/ui/label'
import { Table, TableBody, TableCell, TableHead, TableHeader, TableRow } from '@/components/ui/table'
import { v2 } from '@/lib/api'
import type { ModelCacheEntry, ModelView } from '@/lib/types'
import { useI18n } from '@/lib/useI18n'
import { useModels } from '@/hooks/useModels'
import { useCacheSummary } from '@/hooks/useCacheSummary'
import { useAuthStore } from '@/store/useAuthStore'
import { cn } from '@/lib/utils'
import { toast } from 'sonner'

export function ModelLibraryView() {
  const { t } = useI18n()
  const { token } = useAuthStore()
  const { data: models = [], isLoading: modelsLoading, refetch: refetchModels } = useModels()
  const { data: cacheSummary, isLoading: cacheLoading, refetch: refetchCache } = useCacheSummary()
  
  const [searchQuery, setSearchQuery] = useState('')
  const [actingUid, setActingUid] = useState<string | null>(null)
  const [moveDialogOpen, setMoveDialogOpen] = useState(false)
  const [selectedModel, setSelectedModel] = useState<ModelView | null>(null)
  const [newPath, setNewPath] = useState('')

  const refresh = () => {
    refetchModels()
    refetchCache()
  }

  const handleDelete = async (uid: string) => {
    setActingUid(uid)
    const promise = v2.deleteModel(uid, token || '')
    toast.promise(promise, {
      loading: t('library.deletingModel', { uid }),
      success: () => {
        refresh()
        return t('library.modelDeleted')
      },
      error: t('library.deleteFailed')
    })
    try { await promise } finally { setActingUid(null) }
  }

  const getCacheStats = useCallback((modelUid: string, modelName: string) => {
    const matched = (cacheSummary?.caches || []).filter((item: ModelCacheEntry & { matched_model_uids?: string[] }) => {
      if ((item.matched_model_uids || []).includes(modelUid)) return true
      return item.model_name === modelName
    })
    return {
      nodes: new Set(matched.map((item) => item.node_id)).size,
      bytes: matched.reduce((sum, item) => sum + item.size_bytes, 0),
    }
  }, [cacheSummary])

  const handleMove = async () => {
    if (!selectedModel || !newPath.trim()) return
    setActingUid(selectedModel.model_uid)
    const promise = v2.updateModel(selectedModel.model_uid, { model_path: newPath.trim() }, token || '')
    toast.promise(promise, {
       loading: t('library.updatingPath'),
      success: () => {
        setMoveDialogOpen(false)
        refresh()
         return t('library.pathUpdated')
      },
      error: t('library.updateFailed')
    })
    try { await promise } finally { setActingUid(null) }
  }

  const filtered = models.filter(m => 
    m.model_uid.toLowerCase().includes(searchQuery.toLowerCase()) ||
    m.model_name.toLowerCase().includes(searchQuery.toLowerCase())
  )

  const formatSize = (bytes: number) => {
    if (bytes === 0) return '0 B'
    const k = 1024
    const sizes = ['B', 'KB', 'MB', 'GB', 'TB']
    const i = Math.floor(Math.log(bytes) / Math.log(k))
    return parseFloat((bytes / Math.pow(k, i)).toFixed(2)) + ' ' + sizes[i]
  }

  return (
    <div className="space-y-8 animate-in fade-in duration-500">
      <div className="flex flex-col md:flex-row md:items-end justify-between gap-4">
        <div>
          <h2 className="text-3xl font-bold tracking-tight font-mono uppercase text-foreground">{t('library.title')}</h2>
          <p className="text-muted-foreground mt-2 flex items-center gap-2">
            <Database className="h-4 w-4 text-primary" />
            {t('library.subtitle')}
          </p>
        </div>
        <div className="flex gap-3">
          <div className="relative w-64 group">
            <Search className="absolute left-3 top-1/2 -translate-y-1/2 h-4 w-4 text-muted-foreground group-focus-within:text-primary transition-colors" />
            <input
              type="text"
               placeholder={t('library.searchPlaceholder')}
              value={searchQuery}
              onChange={(e) => setSearchQuery(e.target.value)}
              className="w-full bg-black/20 border border-border/50 rounded-lg pl-10 pr-4 py-2 text-xs font-mono focus:outline-none focus:border-primary/50 transition-all"
            />
          </div>
          <Button
            variant="outline"
            size="sm"
            onClick={refresh}
            className="h-10 px-4 bg-white/5 border-border/50 font-mono text-[10px] uppercase tracking-widest"
          >
            <RefreshCw className={cn("h-3.5 w-3.5 mr-2", modelsLoading || cacheLoading ? "animate-spin" : "")} />
            {t('common.refresh')}
          </Button>
        </div>
      </div>

      <div className="bg-card/40 backdrop-blur-xl border border-border rounded-xl overflow-hidden">
        <div className="px-6 py-4 border-b border-border/50 flex items-center justify-between bg-white/5">
          <h3 className="text-xs font-bold font-mono uppercase tracking-widest text-muted-foreground">{t('library.assets')}</h3>
          <Badge variant="outline" className="font-mono text-[10px] border-primary/20 text-primary uppercase">
            {models.length} {t('library.models')}
          </Badge>
        </div>

        <Table>
          <TableHeader className="bg-black/20">
            <TableRow className="border-border/50 hover:bg-transparent">
               <TableHead className="text-[10px] uppercase font-bold text-muted-foreground px-6 py-4">{t('library.assetIdentity')}</TableHead>
               <TableHead className="text-[10px] uppercase font-bold text-muted-foreground">{t('library.storageStatus')}</TableHead>
               <TableHead className="text-[10px] uppercase font-bold text-muted-foreground">{t('library.serviceLevel')}</TableHead>
               <TableHead className="text-[10px] uppercase font-bold text-muted-foreground">{t('library.provisioning')}</TableHead>
               <TableHead className="text-right text-[10px] uppercase font-bold text-muted-foreground pr-6">{t('library.management')}</TableHead>
            </TableRow>
          </TableHeader>
          <TableBody>
            {(modelsLoading || cacheLoading) && models.length === 0 ? (
                <TableRow>
                    <TableCell colSpan={5} className="h-64 text-center">
                        <div className="flex flex-col items-center gap-3 opacity-50">
                            <Loader2 className="h-8 w-8 animate-spin text-primary" />
                            <p className="text-[10px] font-mono uppercase tracking-widest">{t('library.loading')}</p>
                        </div>
                    </TableCell>
                </TableRow>
            ) : filtered.length === 0 ? (
              <TableRow>
                <TableCell colSpan={5} className="h-64 text-center">
                  <div className="flex flex-col items-center justify-center opacity-30 gap-3">
                    <Database className="h-12 w-12" />
                    <p className="text-[10px] font-mono uppercase tracking-widest">{t('library.empty')}</p>
                  </div>
                </TableCell>
              </TableRow>
            ) : (
              filtered.map((model) => {
                const stats = getCacheStats(model.model_uid, model.model_name)
                const isActing = actingUid === model.model_uid
                return (
                  <TableRow key={model.model_uid} className="border-border/40 hover:bg-white/5 transition-colors group">
                    <TableCell className="px-6 py-5">
                      <div className="flex flex-col gap-1.5">
                        <span className="font-mono text-sm font-bold group-hover:text-primary transition-colors">{model.model_uid}</span>
                        <span className="text-[10px] text-muted-foreground/60 font-mono truncate max-w-[280px] uppercase tracking-tighter">{model.model_name}</span>
                      </div>
                    </TableCell>
                    <TableCell>
                      <div className="flex flex-col gap-1.5">
                        <div className="flex items-center gap-2">
                            <div className={cn("w-1.5 h-1.5 rounded-full", stats.bytes > 0 ? "bg-success" : "bg-warning")} />
                            <span className={cn("text-[10px] font-bold uppercase tracking-wider", stats.bytes > 0 ? "text-success" : "text-warning")}>
                                {stats.bytes > 0 ? t('catalog.downloaded') : t('library.pending')}
                            </span>
                        </div>
                        <span className="text-[9px] font-mono text-muted-foreground uppercase tracking-widest">
                             {formatSize(stats.bytes)} ● {t('library.nodesCount', { count: stats.nodes })}
                        </span>
                      </div>
                    </TableCell>
                    <TableCell>
                        <Badge variant="outline" className="font-mono text-[9px] border-border/50 text-muted-foreground uppercase">
                            {model.state}
                        </Badge>
                    </TableCell>
                    <TableCell>
                      <div className="flex items-baseline gap-1 font-mono">
                        <span className="text-sm font-bold text-foreground">{model.replicas.ready}</span>
                        <span className="text-[10px] text-muted-foreground">/ {model.replicas.desired}</span>
                      </div>
                    </TableCell>
                    <TableCell className="text-right pr-6">
                      <div className="flex justify-end gap-2">
                        <Button 
                            variant="ghost" size="sm" 
                            className="h-9 px-4 hover:bg-white/10 font-bold text-[10px] uppercase tracking-widest border border-transparent hover:border-border/50 transition-all"
                        >
                           {t('library.openService')}
                          <ArrowUpRight className="ml-2 h-3.5 w-3.5" />
                        </Button>
                        <Button
                          variant="ghost" size="sm"
                          className="h-9 w-9 p-0 hover:bg-white/10"
                          onClick={() => { setSelectedModel(model); setNewPath(''); setMoveDialogOpen(true); }}
                          disabled={isActing}
                        >
                          <FolderInput className="h-4 w-4" />
                        </Button>
                        <Button
                          variant="ghost" size="sm"
                          className="h-9 w-9 p-0 hover:bg-destructive/20 hover:text-destructive"
                          onClick={() => handleDelete(model.model_uid)}
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

      <Dialog open={moveDialogOpen} onOpenChange={setMoveDialogOpen}>
        <DialogContent className="sm:max-w-[500px] bg-card/95 backdrop-blur-2xl border-border rim-light">
          <DialogHeader>
            <DialogTitle className="font-mono uppercase tracking-tight text-2xl flex items-center gap-3">
              <HardDrive className="h-6 w-6 text-primary" />
               {t('library.relocateAsset')}
            </DialogTitle>
          </DialogHeader>

          <div className="space-y-6 py-4">
            <div className="space-y-1 p-3 rounded-lg bg-white/5 border border-border/30">
               <p className="text-[10px] uppercase font-bold tracking-widest text-muted-foreground">{t('library.targetAsset')}</p>
              <p className="text-sm font-mono font-bold text-foreground break-all">{selectedModel?.model_uid || '—'}</p>
            </div>

            <div className="space-y-2">
               <Label htmlFor="library-move-path" className="text-[10px] uppercase font-bold tracking-widest text-muted-foreground">{t('library.newStoragePath')}</Label>
              <div className="relative">
                  <Box className="absolute left-3 top-1/2 -translate-y-1/2 h-4 w-4 text-muted-foreground" />
                  <Input
                    id="library-move-path"
                    className="pl-10 bg-white/5 border-border/50 font-mono text-xs"
                     placeholder={t('library.pathPlaceholder')}
                    value={newPath}
                    onChange={(e) => setNewPath(e.target.value)}
                  />
              </div>
              <p className="text-[9px] text-muted-foreground uppercase tracking-widest mt-1">
                  {t('library.pathAccessibility')}
              </p>
            </div>
            
            <div className="p-4 rounded-lg bg-primary/5 border border-primary/10 flex gap-3">
                <Info className="h-4 w-4 text-primary shrink-0 mt-0.5" />
                <p className="text-[10px] text-muted-foreground uppercase leading-relaxed tracking-wider">
                     {t('library.relocateHint')}
                </p>
            </div>
          </div>

          <DialogFooter className="bg-black/20 -mx-6 -mb-6 p-6 mt-4">
            <Button variant="ghost" onClick={() => setMoveDialogOpen(false)} className="font-mono text-[10px] uppercase tracking-widest text-muted-foreground hover:text-foreground">
              {t('common.cancel')}
            </Button>
            <Button onClick={handleMove} disabled={!newPath.trim() || !selectedModel} className="bg-primary text-primary-foreground rim-light h-10 px-8 font-bold uppercase tracking-widest text-xs ml-auto">
               {t('library.commitRelocation')}
            </Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>
    </div>
  )
}
