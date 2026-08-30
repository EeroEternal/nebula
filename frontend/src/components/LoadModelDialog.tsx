import { useEffect } from "react"
import { Search, ArrowLeft, ArrowRight, Cpu, Server, Check, Globe, Settings2, Rocket, X, Info } from "lucide-react"
import {
    Dialog,
    DialogContent,
    DialogTitle,
} from "@/components/ui/dialog"
import { Button } from "@/components/ui/button"
import { Input } from "@/components/ui/input"
import { Progress } from "@/components/ui/progress"
import { cn } from "@/lib/utils"
import { v2 } from "@/lib/api"
import { useLoadModelStore } from "@/store/useLoadModelStore"
import { useClusterOverview } from "@/hooks/useClusterOverview"
import { useImages } from "@/hooks/useImages"
import { Label } from "@/components/ui/label"
import { toast } from "sonner"
import type { ClusterStatus, EngineImage, ModelLoadRequest } from "@/lib/types"
import type { LucideIcon } from "lucide-react"
import type { ReactNode } from "react"
import { useI18n } from "@/lib/useI18n"

type Step = 'source' | 'search' | 'hardware' | 'engine' | 'review'

export function LoadModelDialog() {
    const { t } = useI18n()
    const { open, setOpen, step, setStep, form, updateForm, selectedNode, selectedGpuIndices, setHardware, reset } = useLoadModelStore()
    const { data: overview, refetch: refetchOverview } = useClusterOverview()
    const { data: imageData } = useImages()
    
    useEffect(() => {
        if (!open) reset()
    }, [open, reset])

    const handleClose = () => setOpen(false)

    const handleSubmit = async () => {
        if (!selectedNode || selectedGpuIndices.length === 0 || !form.model_name) return
        
        const finalForm: ModelLoadRequest = {
            ...form,
            node_id: selectedNode,
            gpu_indices: selectedGpuIndices,
            config: {
                ...form.config,
                tensor_parallel_size: selectedGpuIndices.length > 1 ? selectedGpuIndices.length : undefined
            }
        }

        const promise = v2.createModel({ ...finalForm }, '') // Token placeholder
        toast.promise(promise, {
            loading: t('loadDialog.deploying'),
            success: () => {
                setOpen(false)
                refetchOverview()
                return t('loadDialog.deploymentStarted')
            },
            error: t('loadDialog.deployFailed')
        })
    }

    return (
        <Dialog open={open} onOpenChange={setOpen}>
            <DialogContent className="max-w-4xl h-[85vh] p-0 overflow-hidden bg-card/95 backdrop-blur-2xl border-border rim-light flex flex-col sm:rounded-2xl">
                {/* Custom Header */}
                <div className="px-8 py-6 border-b border-border/50 flex items-center justify-between bg-white/5">
                    <div className="flex items-center gap-4">
                        <div className="w-10 h-10 rounded-xl bg-primary flex items-center justify-center rim-light">
                            <Rocket className="h-6 w-6 text-primary-foreground" />
                        </div>
                        <div>
                            <DialogTitle className="text-xl font-bold font-mono uppercase tracking-tight">{t('loadDialog.provisionModel')}</DialogTitle>
                            <p className="text-[10px] font-mono text-muted-foreground uppercase tracking-widest mt-0.5">{t('loadDialog.wizardStep', { step: ['source', 'search', 'hardware', 'engine', 'review'].indexOf(step) + 1 })}</p>
                        </div>
                    </div>
                    <Button variant="ghost" size="icon" onClick={handleClose} className="rounded-full hover:bg-white/10 h-8 w-8">
                        <X className="h-4 w-4" />
                    </Button>
                </div>

                <div className="flex-1 overflow-hidden flex">
                    {/* Stepper Sidebar */}
                    <div className="w-64 border-r border-border/50 bg-black/20 p-6 hidden md:flex flex-col gap-8">
                         <StepItem icon={Globe} label={t('loadDialog.stepSource')} active={step === 'source'} completed={['search', 'hardware', 'engine', 'review'].includes(step)} />
                         <StepItem icon={Search} label={t('loadDialog.stepIdentity')} active={step === 'search'} completed={['hardware', 'engine', 'review'].includes(step)} />
                         <StepItem icon={Cpu} label={t('loadDialog.stepHardware')} active={step === 'hardware'} completed={['engine', 'review'].includes(step)} />
                         <StepItem icon={Settings2} label={t('loadDialog.stepEngine')} active={step === 'engine'} completed={['review'].includes(step)} />
                         <StepItem icon={Check} label={t('loadDialog.stepReview')} active={step === 'review'} completed={false} />
                    </div>

                    {/* Content Area */}
                    <div className="flex-1 flex flex-col overflow-hidden bg-background/50">
                        <div className="flex-1 overflow-y-auto p-8">
                            {step === 'source' && <StepSource />}
                            {step === 'search' && <StepSearch />}
                            {step === 'hardware' && <StepHardware overview={overview} selectedNode={selectedNode} selectedGpuIndices={selectedGpuIndices} onSelect={setHardware} />}
                            {step === 'engine' && <StepEngine form={form} updateForm={updateForm} images={imageData?.images || []} />}
                            {step === 'review' && <StepReview form={form} node={selectedNode} gpus={selectedGpuIndices} />}
                        </div>

                        {/* Footer Controls */}
                        <div className="px-8 py-6 border-t border-border/50 bg-black/20 flex justify-between items-center">
                            <Button 
                                variant="ghost" 
                                onClick={() => {
                                    const steps: Step[] = ['source', 'search', 'hardware', 'engine', 'review']
                                    const idx = steps.indexOf(step)
                                    if (idx > 0) setStep(steps[idx - 1])
                                }}
                                disabled={step === 'source'}
                                className="font-mono text-[10px] uppercase tracking-widest text-muted-foreground hover:text-foreground"
                            >
                                 <ArrowLeft className="h-3 w-3 mr-2" /> {t('loadDialog.previous')}
                            </Button>
                            
                            {step === 'review' ? (
                                <Button 
                                    onClick={handleSubmit}
                                    className="bg-primary text-primary-foreground rim-light h-10 px-8 font-bold uppercase tracking-widest text-xs"
                                >
                                     {t('loadDialog.execute')} <Rocket className="ml-2 h-4 w-4" />
                                </Button>
                            ) : (
                                <Button 
                                    onClick={() => {
                                        const steps: Step[] = ['source', 'search', 'hardware', 'engine', 'review']
                                        const idx = steps.indexOf(step)
                                        if (idx < steps.length - 1) setStep(steps[idx + 1])
                                    }}
                                    className="bg-primary text-primary-foreground rim-light h-10 px-8 font-bold uppercase tracking-widest text-xs"
                                >
                                     {t('loadDialog.proceed')} <ArrowRight className="ml-2 h-4 w-4" />
                                </Button>
                            )}
                        </div>
                    </div>
                </div>
            </DialogContent>
        </Dialog>
    )
}

interface StepItemProps {
    icon: LucideIcon
    label: string
    active: boolean
    completed: boolean
}

function StepItem({ icon: Icon, label, active, completed }: StepItemProps) {
    return (
        <div className={cn("flex items-center gap-3 transition-colors", active ? "text-primary" : completed ? "text-success" : "text-muted-foreground")}>
            <div className={cn("w-8 h-8 rounded-lg flex items-center justify-center border transition-all", 
                active ? "bg-primary/10 border-primary rim-light" : completed ? "bg-success/10 border-success/30" : "bg-white/5 border-border/50")}>
                {completed ? <Check className="h-4 w-4" /> : <Icon className="h-4 w-4" />}
            </div>
            <span className="text-[10px] font-bold uppercase tracking-widest">{label}</span>
        </div>
    )
}

function StepSource() {
    const { t } = useI18n()
    const { source, setSource, setStep } = useLoadModelStore()
    const options = [
        { id: 'huggingface', label: t('loadDialog.sourceHuggingFace'), desc: t('loadDialog.sourceHuggingFaceDesc'), icon: '🤗' },
        { id: 'modelscope', label: t('loadDialog.sourceModelScope'), desc: t('loadDialog.sourceModelScopeDesc'), icon: '📦' },
        { id: 'template', label: t('loadDialog.sourceTemplate'), desc: t('loadDialog.sourceTemplateDesc'), icon: '📋' },
        { id: 'manual', label: t('loadDialog.sourceManual'), desc: t('loadDialog.sourceManualDesc'), icon: '⌨️' },
    ] as const

    return (
        <div className="space-y-6">
            <div className="space-y-1">
                 <h3 className="text-sm font-bold uppercase tracking-widest text-foreground">{t('loadDialog.selectOrigin')}</h3>
                 <p className="text-[10px] text-muted-foreground uppercase tracking-widest">{t('loadDialog.originDesc')}</p>
            </div>
            <div className="grid grid-cols-1 sm:grid-cols-2 gap-4">
                {options.map((opt) => (
                    <button
                        key={opt.id}
                        onClick={() => { setSource(opt.id); setStep('search'); }}
                        className={cn("text-left p-5 rounded-xl border border-border transition-all hover:bg-white/5 hover:border-primary/50 group",
                            source === opt.id ? "bg-primary/5 border-primary/50 rim-light" : "bg-card/40")}
                    >
                        <div className="text-2xl mb-3">{opt.icon}</div>
                        <p className="text-xs font-bold uppercase tracking-widest text-foreground mb-1">{opt.label}</p>
                        <p className="text-[10px] text-muted-foreground uppercase tracking-tight leading-relaxed">{opt.desc}</p>
                    </button>
                ))}
            </div>
        </div>
    )
}

function StepSearch() {
    const { t } = useI18n()
    const { source, searchQuery, setSearchQuery, updateForm } = useLoadModelStore()
    return (
        <div className="space-y-6">
             <div className="space-y-1">
                 <h3 className="text-sm font-bold uppercase tracking-widest text-foreground">{t('loadDialog.specifyIdentity')}</h3>
                 <p className="text-[10px] text-muted-foreground uppercase tracking-widest">{t('loadDialog.identityDesc')}</p>
            </div>
            
            {source === 'manual' ? (
                <div className="space-y-4">
                    <div className="space-y-2">
                         <Label className="text-[10px] uppercase font-bold tracking-widest text-muted-foreground">{t('loadDialog.modelPathName')}</Label>
                        <Input 
                            className="bg-white/5 border-border/50 font-mono" 
                             placeholder={t('loadDialog.modelPathPlaceholder')} 
                            onChange={(e) => updateForm({ model_name: e.target.value, model_uid: e.target.value.split('/').pop()?.toLowerCase() })}
                        />
                    </div>
                </div>
            ) : (
                <div className="relative">
                    <Search className="absolute left-3 top-1/2 -translate-y-1/2 h-4 w-4 text-muted-foreground" />
                    <Input 
                        className="pl-10 h-12 bg-white/5 border-border/50 rounded-xl font-mono text-sm" 
                         placeholder={t('loadDialog.searchOn', { source: source === 'huggingface' ? 'Hugging Face' : source === 'modelscope' ? 'ModelScope' : source })}
                        value={searchQuery}
                        onChange={(e) => setSearchQuery(e.target.value)}
                    />
                    <div className="mt-8 p-12 text-center border-2 border-dashed border-border/50 rounded-2xl opacity-50">
                         <p className="text-[10px] font-mono uppercase tracking-widest">{t('loadDialog.registryActive')}</p>
                    </div>
                </div>
            )}
        </div>
    )
}

interface StepHardwareProps {
    overview?: ClusterStatus
    selectedNode: string | null
    selectedGpuIndices: number[]
    onSelect: (nodeId: string | null, gpuIndices: number[]) => void
}

function StepHardware({ overview, selectedNode, selectedGpuIndices, onSelect }: StepHardwareProps) {
    const { t } = useI18n()
    return (
        <div className="space-y-6">
             <div className="space-y-1">
                 <h3 className="text-sm font-bold uppercase tracking-widest text-foreground">{t('loadDialog.hardwareAllocation')}</h3>
                 <p className="text-[10px] text-muted-foreground uppercase tracking-widest">{t('loadDialog.hardwareDesc')}</p>
            </div>

            <div className="space-y-6 overflow-y-auto max-h-[40vh] pr-2">
                {overview?.nodes.map((node) => (
                    <div key={node.node_id} className="space-y-3">
                         <div className="flex items-center gap-2 px-1">
                            <Server className="h-3 w-3 text-muted-foreground" />
                            <span className="text-[10px] font-bold font-mono uppercase tracking-widest">{node.node_id}</span>
                         </div>
                         <div className="grid grid-cols-1 sm:grid-cols-2 gap-3">
                            {node.gpus.map((gpu) => {
                                const isSelected = selectedNode === node.node_id && selectedGpuIndices.includes(gpu.index)
                                const usage = Math.round((gpu.memory_used_mb / gpu.memory_total_mb) * 100)
                                return (
                                    <button
                                        key={gpu.index}
                                        onClick={() => {
                                            if (selectedNode !== node.node_id) onSelect(node.node_id, [gpu.index])
                                            else {
                                                const next = selectedGpuIndices.includes(gpu.index)
                                                    ? selectedGpuIndices.filter((i: number) => i !== gpu.index)
                                                    : [...selectedGpuIndices, gpu.index]
                                                onSelect(node.node_id, next)
                                            }
                                        }}
                                        className={cn("p-4 rounded-xl border text-left transition-all group",
                                            isSelected ? "bg-primary/5 border-primary rim-light" : "bg-card/40 border-border hover:border-primary/30")}
                                    >
                                        <div className="flex items-center justify-between mb-3">
                                            <div className="flex items-center gap-2">
                                                <Cpu className={cn("h-3.5 w-3.5", isSelected ? "text-primary" : "text-muted-foreground")} />
                                                <span className="text-xs font-bold font-mono uppercase tracking-widest">GPU {gpu.index}</span>
                                            </div>
                                            <span className={cn("text-[10px] font-mono font-bold", usage > 80 ? "text-destructive" : "text-primary")}>{usage}%</span>
                                        </div>
                                        <Progress value={usage} className="h-1 bg-white/5" indicatorClassName={isSelected ? "bg-primary" : "bg-primary/50"} />
                                        <div className="mt-3 flex items-center justify-between text-[9px] font-mono text-muted-foreground/60 uppercase tracking-widest">
                                             <span>{t('loadDialog.available')}</span>
                                            <span>{((gpu.memory_total_mb - gpu.memory_used_mb) / 1024).toFixed(1)}GB / {(gpu.memory_total_mb / 1024).toFixed(1)}GB</span>
                                        </div>
                                    </button>
                                )
                            })}
                         </div>
                    </div>
                ))}
            </div>
        </div>
    )
}

interface StepEngineProps {
    form: ModelLoadRequest
    updateForm: (updates: Partial<ModelLoadRequest>) => void
    images: EngineImage[]
}

function StepEngine({ form, updateForm, images }: StepEngineProps) {
    const { t } = useI18n()
    return (
        <div className="space-y-6">
            <div className="space-y-1">
                 <h3 className="text-sm font-bold uppercase tracking-widest text-foreground">{t('loadDialog.engineRuntime')}</h3>
                 <p className="text-[10px] text-muted-foreground uppercase tracking-widest">{t('loadDialog.engineDesc')}</p>
            </div>

            <div className="space-y-6">
                <div className="space-y-3">
                     <Label className="text-[10px] uppercase font-bold tracking-widest text-muted-foreground">{t('loadDialog.executionBackend')}</Label>
                    <div className="flex gap-2 p-1 bg-black/20 rounded-xl border border-border/50 w-fit">
                        {['vllm', 'sglang'].map(eng => (
                            <button
                                key={eng}
                                onClick={() => updateForm({ engine_type: eng })}
                                className={cn("px-4 py-1.5 rounded-lg text-[10px] font-bold uppercase tracking-widest transition-all",
                                    form.engine_type === eng ? "bg-primary text-primary-foreground shadow-sm" : "text-muted-foreground hover:text-foreground")}
                            >
                                {eng}
                            </button>
                        ))}
                    </div>
                </div>

                <div className="space-y-2">
                     <Label className="text-[10px] uppercase font-bold tracking-widest text-muted-foreground">{t('loadDialog.runtimeImage')}</Label>
                    <select
                        className="w-full h-11 bg-white/5 border border-border/50 rounded-xl px-4 font-mono text-xs focus:outline-none focus:ring-1 focus:ring-primary/30"
                        value={form.docker_image ?? ''}
                        onChange={(e) => updateForm({ docker_image: e.target.value || undefined })}
                    >
                         <option value="">{t('loadDialog.systemDefault')}</option>
                        {images.filter((img) => img.engine_type === form.engine_type).map((img) => (
                            <option key={img.id} value={img.image}>{img.id} ({img.image})</option>
                        ))}
                    </select>
                </div>

                <div className="grid grid-cols-2 gap-4">
                    <div className="space-y-2">
                         <Label className="text-[10px] uppercase font-bold tracking-widest text-muted-foreground">{t('loadDialog.targetReplicas')}</Label>
                        <Input 
                            type="number" 
                            className="bg-white/5 border-border/50 font-mono h-11" 
                            value={form.replicas} 
                            onChange={(e) => updateForm({ replicas: Number(e.target.value) })}
                        />
                    </div>
                    <div className="space-y-2">
                         <Label className="text-[10px] uppercase font-bold tracking-widest text-muted-foreground">{t('loadDialog.contextWindowTokens')}</Label>
                        <Input 
                            type="number" 
                            className="bg-white/5 border-border/50 font-mono h-11" 
                            placeholder="4096"
                            value={form.config?.max_model_len ?? ''}
                            onChange={(e) => updateForm({ config: { ...form.config, max_model_len: Number(e.target.value) || undefined } })}
                        />
                    </div>
                </div>
            </div>
        </div>
    )
}

interface StepReviewProps {
    form: ModelLoadRequest
    node: string | null
    gpus: number[]
}

function StepReview({ form, node, gpus }: StepReviewProps) {
    const { t } = useI18n()
    return (
        <div className="space-y-6">
            <div className="space-y-1">
                 <h3 className="text-sm font-bold uppercase tracking-widest text-foreground">{t('loadDialog.reviewManifest')}</h3>
                 <p className="text-[10px] text-muted-foreground uppercase tracking-widest">{t('loadDialog.reviewDesc')}</p>
            </div>

            <div className="bg-white/5 border border-border/30 rounded-2xl p-6 space-y-6 divide-y divide-border/20">
                 <ReviewItem label={t('loadDialog.modelProtocol')} value={form.model_name} subValue={`UID: ${form.model_uid}`} />
                 <ReviewItem label={t('loadDialog.computeTarget')} value={node} subValue={`GPU INDICES: ${gpus.join(', ')}`} pt />
                 <ReviewItem label={t('loadDialog.runtimeEngine')} value={form.engine_type?.toUpperCase()} subValue={form.docker_image || t('loadDialog.systemDefault')} pt />
                 <ReviewItem label={t('loadDialog.scaleCapacity')} value={`${form.replicas} ${t('loadDialog.replicas').toUpperCase()}`} subValue={`${t('loadDialog.limit').toUpperCase()}: ${form.config?.max_model_len || t('loadDialog.auto').toUpperCase()} ${t('loadDialog.tokens').toUpperCase()}`} pt />
            </div>

            <div className="p-4 rounded-xl bg-primary/5 border border-primary/10 flex gap-4">
                 <Info className="h-5 w-5 text-primary shrink-0 mt-0.5" />
                 <p className="text-[10px] text-muted-foreground uppercase leading-relaxed tracking-wider">
                     {t('loadDialog.reviewNotice')}
                 </p>
            </div>
        </div>
    )
}

interface ReviewItemProps {
    label: string
    value: ReactNode
    subValue: ReactNode
    pt?: boolean
}

function ReviewItem({ label, value, subValue, pt }: ReviewItemProps) {
    const { t } = useI18n()
    return (
        <div className={cn("flex justify-between items-start", pt && "pt-6")}>
            <span className="text-[10px] font-bold text-muted-foreground uppercase tracking-widest">{label}</span>
            <div className="text-right">
                <p className="text-sm font-mono font-bold text-foreground uppercase">{value || t('loadDialog.unspecified')}</p>
                <p className="text-[9px] font-mono text-muted-foreground/60 uppercase mt-1 tracking-tighter">{subValue}</p>
            </div>
        </div>
    )
}
