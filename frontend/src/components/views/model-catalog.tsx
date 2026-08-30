import { useMemo, useState } from 'react'
import { Download, ExternalLink, Search, Globe, Box, Filter, ArrowUpRight, Loader2, Info } from 'lucide-react'
import { Badge } from '@/components/ui/badge'
import { Button } from '@/components/ui/button'
import { Dialog, DialogContent, DialogFooter, DialogHeader, DialogTitle } from '@/components/ui/dialog'
import { Input } from '@/components/ui/input'
import { Label } from '@/components/ui/label'
import { Table, TableBody, TableCell, TableHead, TableHeader, TableRow } from '@/components/ui/table'
import { apiGetWithParams, v2 } from '@/lib/api'
import type { ModelView } from '@/lib/types'
import { useI18n } from '@/lib/useI18n'
import { useModels } from '@/hooks/useModels'
import { useAuthStore } from '@/store/useAuthStore'
import { cn } from '@/lib/utils'
import { toast } from 'sonner'
import { useQuery } from '@tanstack/react-query'

interface ModelSearchResult {
  id: string
  name: string
  author?: string | null
  downloads: number
  likes: number
  tags: string[]
  pipeline_tag?: string | null
  source: 'huggingface' | 'modelscope'
}

type SourceOption = 'hugging_face' | 'model_scope'

export function ModelCatalogView() {
  const { t } = useI18n()
  const { token } = useAuthStore()
  const { data: models = [], refetch: refetchModels } = useModels()
  
  const [source, setSource] = useState<SourceOption>('hugging_face')
  const [taskFilter, setTaskFilter] = useState<string>('all')
  const [searchQuery, setSearchQuery] = useState('')
  const [downloadDialogOpen, setDownloadDialogOpen] = useState(false)
  const [selectedResult, setSelectedResult] = useState<ModelSearchResult | null>(null)
  const [downloadModelUid, setDownloadModelUid] = useState('')
  const [downloadPath, setDownloadPath] = useState('')
  const [activeDownloadUid, setActiveDownloadUid] = useState<string | null>(null)

  const compactNumber = useMemo(
    () => new Intl.NumberFormat('en', { notation: 'compact', maximumFractionDigits: 1 }),
    []
  )

  const { data: searchResults = [], isLoading: searching } = useQuery({
    queryKey: ['model-search', source, searchQuery],
    queryFn: () => apiGetWithParams<ModelSearchResult[]>(
        '/models/search',
        {
          q: searchQuery.trim(),
          source: source === 'hugging_face' ? 'huggingface' : 'modelscope',
          limit: '20',
        },
        token || ''
      ),
    enabled: !!token && (searchQuery.length > 2 || searchQuery.length === 0),
    staleTime: 60000,
  })

  const downloadedByName = useMemo(() => {
    const index = new Map<string, ModelView[]>()
    for (const model of models) {
      const list = index.get(model.model_name) || []
      list.push(model)
      index.set(model.model_name, list)
    }
    return index
  }, [models])

  const taskLabel = (pipelineTag?: string | null) => {
    if (!pipelineTag) return 'General'
    return pipelineTag
      .split('-')
      .map((part) => part.charAt(0).toUpperCase() + part.slice(1))
      .join(' ')
  }

  const taskOptions = useMemo(() => {
    const set = new Set<string>()
    for (const result of searchResults) {
      set.add(taskLabel(result.pipeline_tag))
    }
    return ['all', ...Array.from(set).sort()]
  }, [searchResults])

  const visibleResults = useMemo(() => {
    if (taskFilter === 'all') return searchResults
    return searchResults.filter((result) => taskLabel(result.pipeline_tag) === taskFilter)
  }, [searchResults, taskFilter])

  const handleDownloadConfirm = async () => {
    if (!selectedResult) return
    const promise = v2.createModel(
      {
        model_name: selectedResult.id,
        model_uid: downloadModelUid.trim() || undefined,
        model_path: downloadPath.trim() || undefined,
        model_source: selectedResult.source === 'huggingface' ? 'hugging_face' : 'model_scope',
        auto_start: true,
        replicas: 1,
      },
      token || ''
    )
    
    toast.promise(promise, {
      loading: t('catalog.initiatingImport'),
      success: (data) => {
        setActiveDownloadUid(data.model_uid)
        setDownloadDialogOpen(false)
        refetchModels()
        return t('catalog.importStarted')
      },
      error: t('catalog.importFailed')
    })
  }

  return (
    <div className="space-y-8 animate-in fade-in duration-500">
      <div className="flex flex-col md:flex-row md:items-end justify-between gap-4">
        <div>
          <h2 className="text-3xl font-bold tracking-tight font-mono uppercase text-foreground">{t('catalog.title')}</h2>
          <p className="text-muted-foreground mt-2 flex items-center gap-2">
            <Globe className="h-4 w-4 text-primary" />
            {t('catalog.subtitle')}
          </p>
        </div>
      </div>

      {activeDownloadUid && (
        <div className="bg-primary/5 border border-primary/20 rounded-xl p-5 flex items-center justify-between gap-4 rim-light animate-pulse">
          <div className="flex items-center gap-4">
             <div className="h-10 w-10 rounded-lg bg-primary/20 flex items-center justify-center">
                <Download className="h-5 w-5 text-primary" />
             </div>
             <div>
                <p className="text-xs font-bold font-mono uppercase tracking-widest text-foreground">{t('catalog.downloadTask')}</p>
            <p className="text-[10px] font-mono text-muted-foreground uppercase mt-1">{activeDownloadUid} ● PULLING FROM {source.replace('_', ' ')}</p>
             </div>
          </div>
          <Button
            size="sm"
            variant="ghost"
            className="text-[10px] font-bold uppercase tracking-widest hover:bg-white/10"
            onClick={() => setActiveDownloadUid(null)}
          >
            {t('catalog.dismiss')}
          </Button>
        </div>
      )}

      <div className="flex flex-col md:flex-row gap-4 items-center justify-between bg-card/40 backdrop-blur-xl border border-border p-4 rounded-xl">
        <div className="flex items-center gap-2 flex-wrap">
            <div className="flex items-center gap-2 bg-black/20 px-3 py-1.5 rounded-lg border border-border/50">
                <Globe className="h-3.5 w-3.5 text-muted-foreground" />
                {(['hugging_face', 'model_scope'] as const).map((s) => (
                    <button
                        key={s}
                        onClick={() => setSource(s)}
                        className={cn(
                            "px-2.5 py-1 rounded-md text-[10px] font-bold uppercase tracking-wider transition-all whitespace-nowrap",
                            source === s
                                ? "bg-primary text-primary-foreground shadow-sm"
                                : "text-muted-foreground hover:text-foreground"
                        )}
                    >
                        {s.replace('_', ' ')}
                    </button>
                ))}
            </div>

            <div className="flex items-center gap-2 bg-black/20 px-3 py-1.5 rounded-lg border border-border/50">
                <Filter className="h-3.5 w-3.5 text-muted-foreground" />
                <select
                    value={taskFilter}
                    onChange={(e) => setTaskFilter(e.target.value)}
                    className="bg-transparent border-none text-[10px] font-bold uppercase tracking-wider focus:outline-none cursor-pointer text-muted-foreground hover:text-foreground"
                >
                    <option value="all" className="bg-card text-foreground">{t('catalog.allTasks')}</option>
                    {taskOptions.filter(o => o !== 'all').map(o => (
                        <option key={o} value={o} className="bg-card text-foreground">{o}</option>
                    ))}
                </select>
            </div>
        </div>

        <div className="relative w-full md:w-80 group">
          <Search className="absolute left-3 top-1/2 -translate-y-1/2 h-4 w-4 text-muted-foreground group-focus-within:text-primary transition-colors" />
          <input
            type="text"
           placeholder={t('catalog.searchRegistry')}
            value={searchQuery}
            onChange={(e) => setSearchQuery(e.target.value)}
            className="w-full bg-black/20 border border-border/50 rounded-lg pl-10 pr-4 py-2 text-xs font-mono focus:outline-none focus:border-primary/50 transition-all"
          />
        </div>
      </div>

      <div className="bg-card/40 backdrop-blur-xl border border-border rounded-xl overflow-hidden">
        <div className="px-6 py-4 border-b border-border/50 flex items-center justify-between bg-white/5">
            <h3 className="text-xs font-bold font-mono uppercase tracking-widest text-muted-foreground">{t('catalog.browse')}</h3>
            <Badge variant="outline" className="font-mono text-[10px] border-primary/20 text-primary uppercase">
                {visibleResults.length} {t('catalog.results')}
            </Badge>
        </div>

        <Table>
          <TableHeader className="bg-black/20">
            <TableRow className="border-border/50 hover:bg-transparent">
              <TableHead className="text-[10px] uppercase font-bold text-muted-foreground px-6 py-4">{t('catalog.provider')} ID</TableHead>
              <TableHead className="text-[10px] uppercase font-bold text-muted-foreground">{t('modelDetail.capabilities')}</TableHead>
              <TableHead className="text-[10px] uppercase font-bold text-muted-foreground">{t('catalog.downloads')}</TableHead>
              <TableHead className="text-right text-[10px] uppercase font-bold text-muted-foreground pr-6">{t('catalog.likes')}</TableHead>
            </TableRow>
          </TableHeader>
          <TableBody>
            {searching ? (
                <TableRow>
                    <TableCell colSpan={4} className="h-64 text-center">
                        <div className="flex flex-col items-center gap-3 opacity-50">
                            <Loader2 className="h-8 w-8 animate-spin text-primary" />
                            <p className="text-[10px] font-mono uppercase tracking-widest">{t('catalog.searching')}</p>
                        </div>
                    </TableCell>
                </TableRow>
            ) : visibleResults.length === 0 ? (
              <TableRow>
                <TableCell colSpan={4} className="h-64 text-center">
                  <div className="flex flex-col items-center justify-center opacity-30 gap-3">
                    <Box className="h-12 w-12" />
                    <p className="text-[10px] font-mono uppercase tracking-widest">
                      {t('catalog.empty')}
                    </p>
                  </div>
                </TableCell>
              </TableRow>
            ) : (
              visibleResults.map((result) => {
                const downloaded = downloadedByName.get(result.id) || []
                const isImported = downloaded.length > 0

                return (
                  <TableRow key={`${result.source}-${result.id}`} className="border-border/40 hover:bg-white/5 transition-colors group">
                    <TableCell className="px-6 py-5">
                      <div className="flex flex-col gap-1.5">
                        <div className="flex items-center gap-3">
                          <span className="font-mono text-sm font-bold group-hover:text-primary transition-colors cursor-pointer">{result.id}</span>
                          {isImported && (
                            <Badge variant="secondary" className="text-[9px] font-bold uppercase h-4 bg-primary/20 text-primary border-none">
                              {t('catalog.imported')}
                            </Badge>
                          )}
                        </div>
                        {result.author && (
                          <span className="text-[10px] text-muted-foreground/60 font-mono uppercase tracking-widest">PROV: {result.author}</span>
                        )}
                      </div>
                    </TableCell>
                    <TableCell>
                      <Badge variant="outline" className="font-mono text-[9px] uppercase border-border/50 text-muted-foreground">
                        {taskLabel(result.pipeline_tag)}
                      </Badge>
                    </TableCell>
                    <TableCell>
                      <div className="flex flex-col gap-1 font-mono">
                         <div className="flex items-center gap-2 text-[10px] text-foreground font-bold">
                            <Download className="h-3 w-3 text-muted-foreground" />
                            {compactNumber.format(result.downloads)}
                         </div>
                         <div className="flex items-center gap-2 text-[9px] text-muted-foreground uppercase tracking-tighter">
                           {t('catalog.trending')}
                         </div>
                      </div>
                    </TableCell>
                    <TableCell className="text-right pr-6">
                      <div className="inline-flex items-center gap-2">
                        <Button
                          size="icon"
                          variant="ghost"
                          className="h-9 w-9 hover:bg-white/10"
                          onClick={() => window.open(
                              result.source === 'huggingface' ? `https://huggingface.co/${result.id}` : `https://modelscope.cn/models/${result.id}`,
                              '_blank'
                          )}
                        >
                          <ExternalLink className="h-4 w-4" />
                        </Button>
                        <Button
                            size="sm"
                            className={cn(
                                "h-9 px-4 font-bold text-[10px] uppercase tracking-widest transition-all",
                                isImported ? "bg-white/5 text-muted-foreground border-border/50" : "bg-primary text-primary-foreground rim-light"
                            )}
                            disabled={isImported}
                            onClick={() => {
                                setSelectedResult(result)
                                setDownloadDialogOpen(true)
                            }}
                        >
                            {isImported ? t('catalog.synced') : t('catalog.import')}
                            {!isImported && <ArrowUpRight className="ml-2 h-3 w-3" />}
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

      <Dialog open={downloadDialogOpen} onOpenChange={setDownloadDialogOpen}>
        <DialogContent className="sm:max-w-[500px] bg-card/95 backdrop-blur-2xl border-border rim-light">
          <DialogHeader>
            <DialogTitle className="font-mono uppercase tracking-tight text-2xl flex items-center gap-3">
              <Download className="h-6 w-6 text-primary animate-signal" />
               {t('catalog.importAsset')}
            </DialogTitle>
          </DialogHeader>

          <div className="space-y-6 py-4">
            <div className="space-y-1 p-3 rounded-lg bg-white/5 border border-border/30">
               <p className="text-[10px] uppercase font-bold tracking-widest text-muted-foreground">{t('catalog.targetIdentifier')}</p>
              <p className="text-sm font-mono font-bold text-foreground break-all">
                {selectedResult?.id || '—'}
              </p>
            </div>

            <div className="space-y-2">
               <Label htmlFor="download-model-uid" className="text-[10px] uppercase font-bold tracking-widest text-muted-foreground">{t('catalog.localIdentity')}</Label>
              <Input
                id="download-model-uid"
                className="bg-white/5 border-border/50 font-mono"
                 placeholder={t('catalog.localIdentityPlaceholder')}
                value={downloadModelUid}
                onChange={(e) => setDownloadModelUid(e.target.value)}
              />
            </div>

            <div className="space-y-2">
               <Label htmlFor="download-model-path" className="text-[10px] uppercase font-bold tracking-widest text-muted-foreground">{t('catalog.storagePath')}</Label>
              <div className="relative">
                  <Box className="absolute left-3 top-1/2 -translate-y-1/2 h-4 w-4 text-muted-foreground" />
                  <Input
                    id="download-model-path"
                    className="pl-10 bg-white/5 border-border/50 font-mono text-xs"
                     placeholder={t('catalog.storagePathPlaceholder')}
                    value={downloadPath}
                    onChange={(e) => setDownloadPath(e.target.value)}
                  />
              </div>
              <p className="text-[9px] text-muted-foreground uppercase tracking-widest mt-1">
                  {t('catalog.leaveEmpty')}
              </p>
            </div>
            
            <div className="p-4 rounded-lg bg-primary/5 border border-primary/10 flex gap-3">
                <Info className="h-4 w-4 text-primary shrink-0 mt-0.5" />
                <p className="text-[10px] text-muted-foreground uppercase leading-relaxed tracking-wider">
                     {t('catalog.importNotice')}
                </p>
            </div>
          </div>

          <DialogFooter className="bg-black/20 -mx-6 -mb-6 p-6 mt-4">
            <Button
              variant="ghost"
              onClick={() => setDownloadDialogOpen(false)}
              className="font-mono text-[10px] uppercase tracking-widest text-muted-foreground hover:text-foreground"
            >
              {t('common.cancel')}
            </Button>
            <Button 
                onClick={handleDownloadConfirm} 
                className="bg-primary text-primary-foreground rim-light h-10 px-8 font-bold uppercase tracking-widest text-xs ml-auto"
            >
              <Download className="mr-2 h-4 w-4" />
               {t('catalog.startImport')}
            </Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>
    </div>
  )
}
