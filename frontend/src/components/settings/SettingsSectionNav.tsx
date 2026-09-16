import type { LucideIcon } from "lucide-react"
import { cn } from "@/lib/utils"

export interface SettingsSection {
  id: string
  label: string
  icon: LucideIcon
}

interface SettingsSectionNavProps {
  sections: SettingsSection[]
  activeSection: string
  onSectionChange: (sectionId: string) => void
}

export function SettingsSectionNav({
  sections,
  activeSection,
  onSectionChange,
}: SettingsSectionNavProps) {
  return (
    <nav className="flex items-center gap-1 overflow-x-auto scrollbar-hide p-1 bg-card rounded-lg border border-border w-full sm:w-fit">
      {sections.map((section) => {
        const Icon = section.icon
        const isActive = activeSection === section.id
        return (
          <button
            key={section.id}
            type="button"
            onClick={() => onSectionChange(section.id)}
            className={cn(
              "flex-1 sm:flex-none flex items-center justify-center gap-1.5 px-3 py-1.5 rounded-md text-sm font-medium transition-colors whitespace-nowrap",
              isActive
                ? "bg-primary text-primary-foreground"
                : "text-muted-foreground hover:bg-muted hover:text-foreground",
            )}
          >
            <Icon className={cn("h-3.5 w-3.5", isActive ? "" : "opacity-70")} />
            {section.label}
          </button>
        )
      })}
    </nav>
  )
}
