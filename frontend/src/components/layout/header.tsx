import { SidebarTrigger } from "@/components/ui/sidebar"
import { Separator } from "@/components/ui/separator"
import { LanguageSwitcher } from "@/components/layout/language-switcher"
import { UserMenu } from "@/components/layout/user-menu"

export function SiteHeader() {
  return (
    <header className="relative z-50 flex h-14 shrink-0 items-center justify-between border-b bg-background/95 px-4 backdrop-blur supports-[backdrop-filter]:bg-background/60">
      <div className="flex items-center">
        <SidebarTrigger className="-ml-1" />
        <Separator orientation="vertical" className="mx-3 hidden h-4 sm:block" />
      </div>
      <div className="flex items-center gap-2">
        <LanguageSwitcher />
        <UserMenu />
      </div>
    </header>
  )
}
