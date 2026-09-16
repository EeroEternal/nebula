import { LogOut, Settings, UserCircle } from "lucide-react"
import { useNavigate } from "react-router-dom"
import { Avatar, AvatarFallback } from "@/components/ui/avatar"
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuGroup,
  DropdownMenuItem,
  DropdownMenuLabel,
  DropdownMenuSeparator,
  DropdownMenuTrigger,
} from "@/components/ui/dropdown-menu"
import { t, useI18n } from "@/lib/i18n"
import { useAuthStore } from "@/store/useAuthStore"

export function UserMenu() {
  useI18n()
  const navigate = useNavigate()
  const user = useAuthStore((s) => s.user)
  const identity = user?.display_name || user?.username || t("common.account")

  function handleLogout() {
    useAuthStore.getState().logout()
    navigate("/login")
  }

  return (
    <DropdownMenu>
      <DropdownMenuTrigger asChild>
        <Avatar
          className="h-8 w-8 cursor-pointer text-muted-foreground ring-offset-background transition-all hover:text-foreground hover:ring-2 hover:ring-primary/20"
          aria-label={t("common.userMenu")}
        >
          <AvatarFallback className="rounded-lg bg-transparent">
            <UserCircle className="h-4 w-4" />
          </AvatarFallback>
        </Avatar>
      </DropdownMenuTrigger>
      <DropdownMenuContent className="w-56 max-w-[calc(100vw-1rem)]" align="end" sideOffset={10}>
        <DropdownMenuLabel className="p-0">
          <div className="flex items-center gap-2 px-3 py-2 text-left text-sm">
            <Avatar className="h-8 w-8 text-muted-foreground">
              <AvatarFallback className="rounded-lg bg-transparent">
                <UserCircle className="h-4 w-4" />
              </AvatarFallback>
            </Avatar>
            <div className="grid min-w-0 flex-1 text-left text-sm leading-tight">
              <span className="truncate font-medium">{identity}</span>
            </div>
          </div>
        </DropdownMenuLabel>
        <DropdownMenuSeparator />
        <DropdownMenuGroup>
          <DropdownMenuItem className="justify-start" onClick={() => navigate("/system/settings")}>
            <Settings className="mr-2 h-4 w-4" />
            <span>{t("common.settings")}</span>
          </DropdownMenuItem>
        </DropdownMenuGroup>
        <DropdownMenuSeparator />
        <DropdownMenuItem
          className="justify-start text-destructive focus:text-destructive"
          onClick={handleLogout}
        >
          <LogOut className="mr-2 h-4 w-4" />
          <span>{t("common.logout")}</span>
        </DropdownMenuItem>
      </DropdownMenuContent>
    </DropdownMenu>
  )
}
