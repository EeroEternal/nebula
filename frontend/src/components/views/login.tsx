import { useState } from 'react'
import { useNavigate } from 'react-router-dom'
import { Eye, EyeOff, KeyRound, UserRound, Activity } from 'lucide-react'
import { Button } from '@/components/ui/button'
import { Input } from '@/components/ui/input'
import { Label } from '@/components/ui/label'
import { authApi } from '@/lib/api'
import { useI18n } from '@/lib/useI18n'
import { useAuthStore } from '@/store/useAuthStore'
import { toast } from 'sonner'

import { Badge } from '@/components/ui/badge'
import type { SVGProps } from 'react'

export function LoginView() {
  const { t } = useI18n()
  const navigate = useNavigate()
  const { setAuth } = useAuthStore()
  
  const [username, setUsername] = useState('admin')
  const [password, setPassword] = useState('admin123')
  const [showPassword, setShowPassword] = useState(false)
  const [loading, setLoading] = useState(false)

  const submit = async () => {
    setLoading(true)
    try {
      const result = await authApi.login(username.trim(), password)
      setAuth(result.token, result.user)
      localStorage.setItem('nebula_token', result.token) // Legacy sync
       toast.success(t('login.success'))
      navigate('/')
    } catch (err) {
      toast.error(err instanceof Error ? err.message : t('login.failed'))
    } finally {
      setLoading(false)
    }
  }

  return (
    <div className="min-h-screen w-full bg-background flex overflow-hidden">
      {/* Left Panel: Hero */}
      <div className="hidden lg:flex flex-col flex-1 bg-card/40 border-r border-border p-12 relative overflow-hidden">
        {/* Background Mesh Pattern */}
        <div className="absolute inset-0 opacity-10 pointer-events-none" 
             style={{ backgroundImage: 'radial-gradient(circle at 2px 2px, oklch(70% 0.18 190) 1px, transparent 0)', backgroundSize: '40px 40px' }} />
        
        <div className="flex items-center gap-4 relative z-10">
          <div className="w-10 h-10 rounded-xl bg-primary flex items-center justify-center rim-light">
            <Activity className="h-6 w-6 text-primary-foreground" />
          </div>
          <p className="text-2xl font-bold tracking-tight font-mono uppercase">Nebula</p>
        </div>

        <div className="mt-auto mb-auto max-w-lg relative z-10">
          <Badge variant="outline" className="mb-6 border-primary/20 text-primary font-mono uppercase tracking-widest px-3 py-1">
             {t('login.universalPlane')}
          </Badge>
          <h1 className="text-6xl font-bold leading-tight tracking-tighter text-foreground font-mono uppercase">
            {t('login.heroTitle')}
          </h1>
          <p className="mt-8 text-lg text-muted-foreground leading-relaxed max-w-md">
            {t('login.heroDesc')}
          </p>
        </div>

        <div className="flex items-center justify-between relative z-10">
          <p className="text-[10px] font-mono text-muted-foreground uppercase tracking-widest">© 2026 Nebula Infrastructure Group</p>
          <div className="flex gap-4">
            <div className="w-2 h-2 rounded-full bg-success animate-pulse" />
             <p className="text-[10px] font-mono text-success uppercase tracking-widest">{t('login.systemsOnline')}</p>
          </div>
        </div>
      </div>

      {/* Right Panel: Form */}
      <div className="flex-1 flex items-center justify-center p-8">
        <div className="w-full max-w-sm space-y-10">
          <div className="space-y-2">
            <h2 className="text-4xl font-bold tracking-tight text-foreground uppercase font-mono">{t('login.signIn')}</h2>
            <p className="text-sm text-muted-foreground">{t('login.subtitle')}</p>
          </div>

          <form
            className="space-y-6"
            onSubmit={(e) => {
              e.preventDefault()
              void submit()
            }}
          >
            <div className="space-y-2">
              <Label htmlFor="login-username" className="text-[10px] uppercase font-bold tracking-widest text-muted-foreground">{t('login.email')}</Label>
              <div className="relative group">
                <UserRound className="h-4 w-4 text-muted-foreground absolute left-3 top-1/2 -translate-y-1/2 group-focus-within:text-primary transition-colors" />
                <Input
                  id="login-username"
                  className="pl-10 h-12 bg-white/5 border-border/50 rounded-lg font-mono text-sm focus:ring-1 focus:ring-primary/30"
                   placeholder={t('login.identityPlaceholder')}
                  value={username}
                  onChange={(e) => setUsername(e.target.value)}
                  name="username"
                  autoComplete="username"
                />
              </div>
            </div>

            <div className="space-y-2">
              <div className="flex items-center justify-between">
                <Label htmlFor="login-password" className="text-[10px] uppercase font-bold tracking-widest text-muted-foreground">{t('login.password')}</Label>
                <button type="button" className="text-[10px] uppercase font-bold text-muted-foreground hover:text-primary transition-colors">{t('login.forgot')}</button>
              </div>
              <div className="relative group">
                <KeyRound className="h-4 w-4 text-muted-foreground absolute left-3 top-1/2 -translate-y-1/2 group-focus-within:text-primary transition-colors" />
                <Input
                  id="login-password"
                  type={showPassword ? 'text' : 'password'}
                  className="pl-10 pr-10 h-12 bg-white/5 border-border/50 rounded-lg font-mono text-sm focus:ring-1 focus:ring-primary/30"
                  value={password}
                  onChange={(e) => setPassword(e.target.value)}
                  name="password"
                  autoComplete="current-password"
                />
                <button
                  type="button"
                  onClick={() => setShowPassword((v) => !v)}
                  className="absolute right-3 top-1/2 -translate-y-1/2 text-muted-foreground hover:text-foreground"
                >
                  {showPassword ? <EyeOff className="h-4 w-4" /> : <Eye className="h-4 w-4" />}
                </button>
              </div>
            </div>

            <Button 
              type="submit"
              className="w-full bg-primary text-primary-foreground rim-light h-12 font-bold uppercase tracking-widest text-xs" 
              disabled={loading}
            >
              {loading ? (
                <div className="flex items-center gap-2">
                  <Loader2 className="h-4 w-4 animate-spin" />
                  {t('login.signingIn')}
                </div>
              ) : t('login.signIn')}
            </Button>
          </form>

          <div className="pt-6 border-t border-border/50 text-center space-y-4">
            <p className="text-[10px] text-muted-foreground uppercase tracking-widest">
              {t('login.noAccount')} <span className="text-foreground font-bold cursor-pointer hover:text-primary">{t('login.contactAdmin')}</span>
            </p>
            <p className="text-[9px] font-mono text-muted-foreground opacity-50 uppercase">{t('login.demo')}</p>
          </div>
        </div>
      </div>
    </div>
  )
}

function Loader2(props: SVGProps<SVGSVGElement>) {
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
      <path d="M21 12a9 9 0 1 1-6.219-8.56" />
    </svg>
  )
}
