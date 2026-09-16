import type { ReactNode } from "react"
import { Label } from "@/components/ui/label"
import { Switch } from "@/components/ui/switch"

export function SettingsToggleRow({
  label,
  description,
  checked,
  onCheckedChange,
  badge,
}: {
  label: string
  description: string
  checked: boolean
  onCheckedChange: (checked: boolean) => void
  badge?: ReactNode
}) {
  return (
    <div className="flex items-start justify-between gap-3 rounded-lg border border-border px-3 py-2.5">
      <div className="min-w-0 space-y-1">
        <div className="flex flex-wrap items-center gap-2">
          <Label className="text-sm font-medium text-foreground">{label}</Label>
          {badge}
        </div>
        <p className="text-xs text-muted-foreground leading-relaxed">{description}</p>
      </div>
      <Switch checked={checked} onCheckedChange={onCheckedChange} />
    </div>
  )
}
