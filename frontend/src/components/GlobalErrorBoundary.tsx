import { ErrorBoundary } from 'react-error-boundary'
import type { FallbackProps } from 'react-error-boundary'
import { AlertTriangle, RefreshCw } from 'lucide-react'
import { useI18n } from '@/lib/useI18n'

function ErrorFallback({ error, resetErrorBoundary }: FallbackProps) {
  const { t } = useI18n()
  const message = error instanceof Error ? error.message : String(error)
  return (
    <div className="flex h-[60vh] w-full flex-col items-center justify-center text-center px-4 animate-in fade-in duration-500">
      <div className="w-16 h-16 rounded-2xl bg-destructive/10 flex items-center justify-center mb-6 rim-light border border-destructive/20">
        <AlertTriangle className="h-8 w-8 text-destructive" />
      </div>
      <h2 className="text-xl font-bold font-mono uppercase tracking-tight text-foreground mb-2">{t('error.moduleException')}</h2>
      <p className="text-sm text-muted-foreground max-w-md mb-6">
        {t('error.moduleDescription')}
      </p>
      <div className="bg-black/20 border border-border/50 rounded-xl p-4 mb-8 w-full max-w-xl text-left overflow-auto">
          <p className="text-xs font-mono text-destructive break-all">{message}</p>
      </div>
      <button
        onClick={resetErrorBoundary}
        className="flex items-center gap-2 px-6 py-2.5 rounded-xl bg-primary text-primary-foreground font-bold text-xs uppercase tracking-widest rim-light hover:bg-primary/90 transition-colors"
      >
        <RefreshCw className="h-3.5 w-3.5" />
        {t('error.reinitialize')}
      </button>
    </div>
  )
}

export function GlobalErrorBoundary({ children }: { children: React.ReactNode }) {
  return (
    <ErrorBoundary FallbackComponent={ErrorFallback}>
      {children}
    </ErrorBoundary>
  )
}
