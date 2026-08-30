import React, { useCallback, useState } from 'react'
import { Outlet } from 'react-router-dom'
import Sidebar from '@/components/Sidebar'
import { Toaster } from 'sonner'
import { GlobalErrorBoundary } from '@/components/GlobalErrorBoundary'
import { Menu } from 'lucide-react'
import { Button } from '@/components/ui/button'
import { useI18n } from '@/lib/useI18n'
import { LanguageSwitcher } from '@/components/LanguageSwitcher'

export const DashboardLayout: React.FC = () => {
  const [mobileNavOpen, setMobileNavOpen] = useState(false)
  const { t } = useI18n()
  const closeMobileNav = useCallback(() => setMobileNavOpen(false), [])

  return (
    <div className="flex min-h-screen bg-background text-foreground">
      <Sidebar mobileOpen={mobileNavOpen} onClose={closeMobileNav} />
      
      <main className="flex-1 flex flex-col min-w-0 overflow-hidden">
        <div className="hidden items-center justify-end border-b border-border/60 bg-background/80 px-6 py-3 backdrop-blur-xl md:flex">
          <LanguageSwitcher />
        </div>
         <div className="flex items-center gap-3 border-b border-border/60 bg-background/80 px-4 py-3 backdrop-blur-xl md:hidden">
          <Button
            type="button"
            variant="ghost"
            size="icon"
            className="h-9 w-9 text-muted-foreground hover:text-foreground"
            aria-label={t('app.openNavigation')}
            onClick={() => setMobileNavOpen(true)}
          >
            <Menu className="h-5 w-5" />
          </Button>
           <span className="font-mono text-sm font-bold tracking-[0.2em]">NEBULA</span>
           <LanguageSwitcher className="ml-auto" />
        </div>

        <div className="flex-1 overflow-y-auto px-4 py-5 sm:px-6 sm:py-8">
          <div className="max-w-7xl mx-auto">
            <GlobalErrorBoundary>
              <Outlet />
            </GlobalErrorBoundary>
          </div>
        </div>
      </main>

      <Toaster theme="dark" position="top-right" closeButton />
    </div>
  )
}
