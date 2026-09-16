import { Suspense } from "react"
import { Outlet } from "react-router-dom"
import { AppSidebar } from "./app-sidebar"
import { SiteHeader } from "./header"
import { SidebarInset, SidebarProvider } from "@/components/ui/sidebar"
import { PageLoading } from "@/components/ui/page-loading"
import { Toaster } from "@/components/ui/sonner"
import { GlobalErrorBoundary } from "@/components/GlobalErrorBoundary"

export function DashboardLayout() {
  return (
    <SidebarProvider>
      <AppSidebar />
      <SidebarInset className="h-svh overflow-hidden">
        <SiteHeader />
        <main className="flex min-h-0 flex-1 flex-col overflow-auto bg-muted/40 scrollbar-hide">
          <div className="flex min-h-0 flex-1 flex-col">
            <GlobalErrorBoundary>
              <Suspense fallback={<PageLoading />}>
                <Outlet />
              </Suspense>
            </GlobalErrorBoundary>
          </div>
        </main>
      </SidebarInset>
      <Toaster position="top-center" richColors />
    </SidebarProvider>
  )
}
