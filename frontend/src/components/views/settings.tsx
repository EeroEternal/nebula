import { ShieldCheck, KeyRound, Monitor, Cpu, Globe, Database, Settings2, Power } from "lucide-react"
import { Button } from "@/components/ui/button"
import { Badge } from "@/components/ui/badge"
import { useI18n } from "@/lib/useI18n"
import { useAuthStore } from "@/store/useAuthStore"
import { toast } from "sonner"

export function SettingsView() {
    const { t, locale, setLocale } = useI18n()
    const { logout, user } = useAuthStore()

    const handleLogout = () => {
        logout()
        toast.success(t('settings.sessionTerminated'))
    }

    return (
        <div className="max-w-4xl space-y-10 animate-in fade-in duration-500">
            <div className="flex justify-between items-end">
                <div>
                    <h2 className="text-3xl font-bold tracking-tight font-mono uppercase text-foreground">{t('settings.title')}</h2>
                    <p className="text-muted-foreground mt-2 flex items-center gap-2">
                        <Settings2 className="h-4 w-4 text-primary" />
                        {t('settings.subtitle')}
                    </p>
                </div>
            </div>

            <div className="grid grid-cols-1 md:grid-cols-3 gap-8">
                <div className="md:col-span-2 space-y-6">
                    {/* Identity & Access */}
                    <div className="bg-card/40 backdrop-blur-xl border border-border rounded-2xl overflow-hidden rim-light">
                        <div className="px-6 py-5 border-b border-border/50 flex items-center justify-between bg-white/5">
                            <div className="flex items-center gap-3">
                                <ShieldCheck className="h-5 w-5 text-primary" />
                                 <h3 className="text-xs font-bold font-mono uppercase tracking-widest text-foreground">{t('settings.identityAccess')}</h3>
                             </div>
                             <Badge variant="outline" className="font-mono text-[9px] border-primary/20 text-primary uppercase">{t('settings.activeSession')}</Badge>
                        </div>
                        <div className="p-6 space-y-6">
                            <div className="flex items-center justify-between">
                                <div className="space-y-1">
                                     <p className="text-[10px] font-bold text-muted-foreground uppercase tracking-widest">{t('settings.authorizedUser')}</p>
                                    <p className="text-lg font-mono font-bold text-foreground">{user?.username || "SYSTEM_ROOT"}</p>
                                </div>
                                <div className="h-12 w-12 rounded-full bg-primary/10 border border-primary/20 flex items-center justify-center">
                                    <KeyRound className="h-6 w-6 text-primary" />
                                </div>
                            </div>

                            <div className="p-4 rounded-xl bg-black/20 border border-border/50 space-y-3">
                                <div className="flex items-center justify-between">
                                     <span className="text-[10px] font-bold text-muted-foreground uppercase">{t('settings.accessRole')}</span>
                                    <Badge className="bg-primary text-primary-foreground font-mono text-[10px] h-5 uppercase">{user?.role || "ADMIN"}</Badge>
                                </div>
                                <div className="flex items-center justify-between text-[10px]">
                                     <span className="font-bold text-muted-foreground uppercase">{t('settings.sessionPersistence')}</span>
                                     <span className="font-mono text-foreground uppercase">{t('settings.enabledLocalStorage')}</span>
                                </div>
                            </div>

                            <div className="pt-2 flex gap-3">
                                <Button className="flex-1 bg-white/5 border border-border/50 hover:bg-white/10 font-bold uppercase text-[10px] tracking-widest h-10">
                                     {t('settings.updateCredentials')}
                                </Button>
                                <Button 
                                    onClick={handleLogout}
                                    className="flex-1 bg-destructive/10 border border-destructive/30 text-destructive hover:bg-destructive/20 font-bold uppercase text-[10px] tracking-widest h-10"
                                >
                                     <Power className="h-3.5 w-3.5 mr-2" /> {t('settings.terminateSession')}
                                </Button>
                            </div>
                        </div>
                    </div>

                    {/* Regional & Protocol */}
                    <div className="bg-card/40 backdrop-blur-xl border border-border rounded-2xl overflow-hidden">
                        <div className="px-6 py-5 border-b border-border/50 flex items-center justify-between bg-white/5">
                            <div className="flex items-center gap-3">
                                <Globe className="h-5 w-5 text-muted-foreground" />
                                 <h3 className="text-xs font-bold font-mono uppercase tracking-widest text-foreground">{t('settings.localizationProtocol')}</h3>
                            </div>
                        </div>
                        <div className="p-6 grid grid-cols-2 gap-6">
                            <div className="space-y-2">
                                 <p className="text-[10px] font-bold text-muted-foreground uppercase tracking-widest">{t('settings.interfaceLanguage')}</p>
                                 <select value={locale} onChange={(e) => setLocale(e.target.value as 'en' | 'zh')} className="w-full h-10 bg-black/20 border border-border/50 rounded-lg px-3 text-xs font-mono focus:outline-none">
                                     <option value="en">{t('settings.englishUs')}</option>
                                     <option value="zh">{t('settings.simplifiedChinese')}</option>
                                </select>
                            </div>
                            <div className="space-y-2">
                                 <p className="text-[10px] font-bold text-muted-foreground uppercase tracking-widest">{t('settings.apiSchema')}</p>
                                <select className="w-full h-10 bg-black/20 border border-border/50 rounded-lg px-3 text-xs font-mono focus:outline-none">
                                     <option value="v2">{t('settings.coreSchema')}</option>
                                     <option value="grpc">{t('settings.meshSchema')}</option>
                                </select>
                            </div>
                        </div>
                    </div>
                </div>

                <div className="space-y-6">
                    {/* System Manifest */}
                    <div className="bg-card/40 backdrop-blur-xl border border-border rounded-2xl p-6 space-y-8">
                         <h4 className="text-[10px] font-bold text-muted-foreground uppercase tracking-[0.2em] text-center">{t('settings.manifest')}</h4>
                        
                        <div className="space-y-6">
                            <div className="flex items-center gap-4">
                                <div className="p-2.5 rounded-lg bg-white/5 border border-border/50">
                                    <Monitor className="h-4 w-4 text-muted-foreground" />
                                </div>
                                <div className="space-y-0.5">
                                     <p className="text-[9px] font-bold text-muted-foreground uppercase tracking-widest">{t('settings.protocolVersion')}</p>
                                    <p className="text-sm font-mono font-bold text-foreground">0.1.1-BETA.9</p>
                                </div>
                            </div>

                            <div className="flex items-center gap-4">
                                <div className="p-2.5 rounded-lg bg-white/5 border border-border/50">
                                    <Cpu className="h-4 w-4 text-primary" />
                                </div>
                                <div className="space-y-0.5">
                                     <p className="text-[9px] font-bold text-muted-foreground uppercase tracking-widest">{t('settings.apiInfrastructure')}</p>
                                    <p className="text-sm font-mono font-bold text-primary uppercase">Core-Cluster</p>
                                </div>
                            </div>

                            <div className="flex items-center gap-4">
                                <div className="p-2.5 rounded-lg bg-white/5 border border-border/50">
                                    <Database className="h-4 w-4 text-muted-foreground" />
                                </div>
                                <div className="space-y-0.5">
                                     <p className="text-[9px] font-bold text-muted-foreground uppercase tracking-widest">{t('settings.storageBackend')}</p>
                                    <p className="text-sm font-mono font-bold text-foreground uppercase">Redis + S3</p>
                                </div>
                            </div>
                        </div>

                        <div className="pt-4 border-t border-border/30">
                            <div className="p-4 rounded-xl bg-primary/5 border border-primary/10">
                                <p className="text-[9px] text-muted-foreground uppercase leading-relaxed tracking-wider text-center">
                                     {t('settings.haNotice')}
                                </p>
                            </div>
                        </div>
                    </div>
                </div>
            </div>
        </div>
    )
}
