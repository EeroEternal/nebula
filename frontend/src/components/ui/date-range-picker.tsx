import { useMemo, useState } from "react"
import { Calendar as CalendarIcon } from "lucide-react"
import type { DateRange } from "react-day-picker"
import { enUS, zhCN } from "react-day-picker/locale"

import { Button } from "@/components/ui/button"
import { Calendar } from "@/components/ui/calendar"
import { Popover, PopoverContent, PopoverTrigger } from "@/components/ui/popover"
import { t, useI18n } from "@/lib/i18n"
import { cn } from "@/lib/utils"

export interface DateRangePickerPreset {
  id: string
  label: string
  getRange: () => { from: Date; to: Date }
}

interface DateRangePickerProps {
  startDate: string
  endDate: string
  onChange: (next: { startDate: string; endDate: string; presetId?: string }) => void
  onClear?: () => void
  presets?: DateRangePickerPreset[]
  activePresetId?: string | null
  /** When true, closed trigger shows the neutral placeholder instead of preset/date text. */
  forcePlaceholder?: boolean
  className?: string
  triggerClassName?: string
}

function formatLocalDateInput(date: Date) {
  const year = date.getFullYear()
  const month = String(date.getMonth() + 1).padStart(2, "0")
  const day = String(date.getDate()).padStart(2, "0")
  return `${year}-${month}-${day}`
}

function parseLocalDate(dateStr: string): Date | undefined {
  if (!dateStr) return undefined
  const [year, month, day] = dateStr.split("-").map(Number)
  if (!year || !month || !day) return undefined
  return new Date(year, month - 1, day)
}

function startOfDay(date: Date) {
  return new Date(date.getFullYear(), date.getMonth(), date.getDate())
}

function isSameDay(a: Date, b: Date) {
  return (
    a.getFullYear() === b.getFullYear() &&
    a.getMonth() === b.getMonth() &&
    a.getDate() === b.getDate()
  )
}

function formatDisplayDate(date: Date, language: string) {
  return new Intl.DateTimeFormat(language === "zh" ? "zh-CN" : "en-US", {
    year: "numeric",
    month: "2-digit",
    day: "2-digit",
  }).format(date)
}

function formatRangeLabel(from: Date, to: Date | undefined, language: string) {
  if (!to || isSameDay(from, to)) {
    return formatDisplayDate(from, language)
  }
  return `${formatDisplayDate(from, language)} – ${formatDisplayDate(to, language)}`
}

function endOfToday() {
  const date = new Date()
  date.setHours(23, 59, 59, 999)
  return date
}

function commitRange(
  from: Date,
  to: Date,
  onChange: DateRangePickerProps["onChange"],
  setDraft: (next: DateRange | undefined) => void,
) {
  const start = from <= to ? from : to
  const end = from <= to ? to : from
  setDraft({ from: start, to: end })
  onChange({
    startDate: formatLocalDateInput(start),
    endDate: formatLocalDateInput(end),
    presetId: undefined,
  })
}

export function DateRangePicker({
  startDate,
  endDate,
  onChange,
  onClear,
  presets = [],
  activePresetId = null,
  forcePlaceholder = false,
  className,
  triggerClassName,
}: DateRangePickerProps) {
  const { language } = useI18n()
  const [open, setOpen] = useState(false)
  const [draft, setDraft] = useState<DateRange | undefined>()
  const [month, setMonth] = useState<Date>(() => parseLocalDate(startDate) ?? new Date())
  // After selecting a single day, the next click can extend it into a range.
  const [canExtend, setCanExtend] = useState(false)

  const handleOpenChange = (next: boolean) => {
    if (next) {
      const from = parseLocalDate(startDate)
      const to = parseLocalDate(endDate)
      if (from) {
        setDraft({ from, to: to ?? from })
        setMonth(from)
        setCanExtend(Boolean(to && isSameDay(from, to)))
      } else {
        setDraft(undefined)
        setMonth(new Date())
        setCanExtend(false)
      }
    }
    setOpen(next)
  }

  const showPlaceholder = forcePlaceholder || !startDate

  const triggerLabel = useMemo(() => {
    if (showPlaceholder) return t("datePicker.selectRange")
    const from = parseLocalDate(startDate)
    const to = parseLocalDate(endDate)
    if (from) return formatRangeLabel(from, to, language)
    return t("datePicker.selectRange")
  }, [endDate, language, showPlaceholder, startDate])

  const statusText = draft?.from
    ? draft.to && !isSameDay(draft.from, draft.to)
      ? formatRangeLabel(draft.from, draft.to, language)
      : canExtend
        ? t("datePicker.singleDayOrPickEnd")
        : formatRangeLabel(draft.from, draft.to ?? draft.from, language)
    : t("datePicker.selectStart")

  return (
    <div className={cn("shrink-0", className)}>
      <Popover open={open} onOpenChange={handleOpenChange}>
        <PopoverTrigger asChild>
          <Button
            type="button"
            variant="outline"
            className={cn(
              "h-8 min-w-[224px] justify-start rounded-lg px-3 text-left text-sm font-normal",
              showPlaceholder && "text-muted-foreground",
              triggerClassName,
            )}
          >
            <CalendarIcon className="mr-2 h-4 w-4 shrink-0" />
            <span className="truncate">{triggerLabel}</span>
          </Button>
        </PopoverTrigger>
        <PopoverContent
          className="max-h-[384px] min-w-[280px] w-auto overflow-y-auto p-0"
          align="start"
          sideOffset={8}
        >
          <Calendar
            mode="range"
            numberOfMonths={2}
            locale={language === "zh" ? zhCN : enUS}
            selected={draft}
            month={month}
            onMonthChange={setMonth}
            onSelect={(range, selectedDay) => {
              if (!selectedDay) {
                if (!range?.from) {
                  if (draft?.from && draft.to && isSameDay(draft.from, draft.to)) {
                    return
                  }
                  setDraft(undefined)
                  setCanExtend(false)
                  onChange({ startDate: "", endDate: "", presetId: undefined })
                }
                return
              }

              const clicked = startOfDay(selectedDay)
              const hasSingleDay =
                Boolean(draft?.from && draft.to && isSameDay(draft.from, draft.to))

              if (hasSingleDay && canExtend) {
                if (isSameDay(clicked, draft!.from!)) {
                  commitRange(clicked, clicked, onChange, setDraft)
                  setCanExtend(false)
                  return
                }
                commitRange(draft!.from!, clicked, onChange, setDraft)
                setCanExtend(false)
                return
              }

              commitRange(clicked, clicked, onChange, setDraft)
              setCanExtend(true)
            }}
            disabled={{ after: endOfToday() }}
          />
          <div className="space-y-2 border-t border-border px-3 py-2">
            {presets.length > 0 ? (
              <div className="flex flex-wrap items-center gap-2">
                {presets.map((preset) => (
                  <Button
                    key={preset.id}
                    type="button"
                    variant="outline"
                    size="sm"
                    className={cn("h-8", activePresetId === preset.id && "border-primary text-primary")}
                    onClick={() => {
                      const range = preset.getRange()
                      setDraft(range)
                      setCanExtend(isSameDay(range.from, range.to))
                      onChange({
                        startDate: formatLocalDateInput(range.from),
                        endDate: formatLocalDateInput(range.to),
                        presetId: preset.id,
                      })
                      setOpen(false)
                    }}
                  >
                    {preset.label}
                  </Button>
                ))}
              </div>
            ) : null}
            <div className="flex h-8 items-center justify-between gap-3">
              <span className="text-xs text-muted-foreground">{statusText}</span>
              {onClear ? (
                <button
                  type="button"
                  className="text-xs text-muted-foreground underline-offset-2 hover:text-foreground hover:underline"
                  onClick={(event) => {
                    event.preventDefault()
                    event.stopPropagation()
                    setDraft(undefined)
                    setCanExtend(false)
                    onChange({ startDate: "", endDate: "", presetId: undefined })
                    onClear()
                  }}
                >
                  {t("datePicker.clear")}
                </button>
              ) : null}
            </div>
          </div>
        </PopoverContent>
      </Popover>
    </div>
  )
}
