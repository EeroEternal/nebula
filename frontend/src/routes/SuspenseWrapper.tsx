import type { ReactNode } from 'react'
import { Suspense } from 'react'
import { useI18n } from '@/lib/useI18n'

export function SuspenseWrapper({ children }: { children: ReactNode }) {
  const { t } = useI18n()
  return (
    <Suspense fallback={
      <div className="flex h-[50vh] w-full items-center justify-center">
        <div className="flex flex-col items-center gap-4">
          <div className="h-8 w-8 animate-spin rounded-full border-4 border-primary border-t-transparent rim-light"></div>
          <p className="font-mono text-[10px] text-muted-foreground uppercase tracking-widest">{t('common.loading')}</p>
        </div>
      </div>
    }>
      {children}
    </Suspense>
  )
}