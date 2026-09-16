import type { LucideIcon } from "lucide-react"
import {
  Activity,
  BookOpen,
  Box,
  Cpu,
  LayoutDashboard,
  Layers,
  Server,
  Settings,
  Shield,
  Zap,
} from "lucide-react"

export type NavItem = {
  nameKey: string
  href: string
  icon: LucideIcon
}

export type NavSection = {
  id: string
  titleKey: string
  collapsible?: boolean
  items: NavItem[]
}

/** Product nav. Replace items; do not invent a second sidebar. */
export const APP_TITLE = "Nebula"

export const NAV_SECTIONS: NavSection[] = [
  {
    id: "workbench",
    titleKey: "nav.workbench",
    collapsible: true,
    items: [
      { nameKey: "nav.dashboard", href: "/", icon: LayoutDashboard },
      { nameKey: "nav.models", href: "/models", icon: Box },
      { nameKey: "nav.inference", href: "/inference", icon: Activity },
      { nameKey: "nav.gateway", href: "/inference/gateway", icon: Shield },
      { nameKey: "nav.endpoints", href: "/endpoints", icon: Cpu },
      { nameKey: "nav.governance", href: "/governance", icon: Shield },
    ],
  },
  {
    id: "infrastructure",
    titleKey: "nav.infrastructure",
    items: [
      { nameKey: "nav.nodes", href: "/infrastructure/nodes", icon: Server },
      { nameKey: "nav.images", href: "/infrastructure/images", icon: Zap },
      { nameKey: "nav.templates", href: "/infrastructure/templates", icon: Layers },
    ],
  },
  {
    id: "resources",
    titleKey: "nav.resources",
    items: [
      { nameKey: "nav.catalog", href: "/resources/model-catalog", icon: BookOpen },
      { nameKey: "nav.library", href: "/resources/model-library", icon: Layers },
      { nameKey: "nav.audit", href: "/resources/audit", icon: Shield },
    ],
  },
  {
    id: "system",
    titleKey: "nav.system",
    items: [{ nameKey: "nav.settings", href: "/system/settings", icon: Settings }],
  },
]
