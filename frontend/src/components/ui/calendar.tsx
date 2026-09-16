import * as React from "react"
import { ChevronLeft, ChevronRight } from "lucide-react"
import { DayButton, DayPicker, getDefaultClassNames } from "react-day-picker"

import { Button } from "@/components/ui/button"
import { buttonVariants } from "@/components/ui/button-variants"
import { cn } from "@/lib/utils"

function Calendar({
  className,
  classNames,
  showOutsideDays = true,
  captionLayout = "label",
  buttonVariant = "ghost",
  formatters,
  components,
  ...props
}: React.ComponentProps<typeof DayPicker> & {
  buttonVariant?: React.ComponentProps<typeof Button>["variant"]
}) {
  const defaultClassNames = getDefaultClassNames()

  return (
    <DayPicker
      showOutsideDays={showOutsideDays}
      className={cn(
        "group/calendar bg-background p-3 [--cell-size:2.5rem]",
        className,
      )}
      captionLayout={captionLayout}
      formatters={{
        formatMonthDropdown: (date) => date.toLocaleString("default", { month: "short" }),
        ...formatters,
      }}
      classNames={{
        root: cn("w-fit", defaultClassNames.root),
        months: cn("relative flex flex-col gap-4 md:flex-row", defaultClassNames.months),
        month: cn("flex w-full flex-col gap-4", defaultClassNames.month),
        nav: cn(
          "absolute inset-x-0 top-0 flex h-8 w-full items-center justify-between gap-1",
          defaultClassNames.nav,
        ),
        button_previous: cn(
          buttonVariants({ variant: buttonVariant }),
          "h-8 w-8 select-none p-0 aria-disabled:opacity-50",
          defaultClassNames.button_previous,
        ),
        button_next: cn(
          buttonVariants({ variant: buttonVariant }),
          "h-8 w-8 select-none p-0 aria-disabled:opacity-50",
          defaultClassNames.button_next,
        ),
        month_caption: cn(
          "flex h-8 w-full items-center justify-center px-8",
          defaultClassNames.month_caption,
        ),
        caption_label: cn("select-none text-sm font-medium", defaultClassNames.caption_label),
        month_grid: cn("w-full border-collapse", defaultClassNames.month_grid),
        weekdays: cn("flex h-6", defaultClassNames.weekdays),
        weekday: cn(
          "flex h-6 w-[--cell-size] select-none items-center justify-center text-xs font-normal text-muted-foreground",
          defaultClassNames.weekday,
        ),
        week: cn("mt-0 flex h-8 w-full", defaultClassNames.week),
        day: cn(
          "group/day relative h-8 w-[--cell-size] select-none p-0 text-center",
          defaultClassNames.day,
        ),
        range_start: cn("rounded-sm bg-primary text-primary-foreground", defaultClassNames.range_start),
        range_middle: cn("rounded-sm bg-primary/10 text-primary", defaultClassNames.range_middle),
        range_end: cn("rounded-sm bg-primary text-primary-foreground", defaultClassNames.range_end),
        today: cn("text-foreground", defaultClassNames.today),
        outside: cn(
          "text-muted-foreground aria-selected:text-muted-foreground",
          defaultClassNames.outside,
        ),
        disabled: cn("text-muted-foreground opacity-50", defaultClassNames.disabled),
        hidden: cn("invisible", defaultClassNames.hidden),
        ...classNames,
      }}
      components={{
        Chevron: ({ className: chevronClassName, orientation, ...chevronProps }) => {
          if (orientation === "left") {
            return <ChevronLeft className={cn("size-4", chevronClassName)} {...chevronProps} />
          }
          return <ChevronRight className={cn("size-4", chevronClassName)} {...chevronProps} />
        },
        DayButton: CalendarDayButton,
        ...components,
      }}
      {...props}
    />
  )
}

function CalendarDayButton({
  className,
  day,
  modifiers,
  children,
  ...props
}: React.ComponentProps<typeof DayButton>) {
  const defaultClassNames = getDefaultClassNames()
  const ref = React.useRef<HTMLButtonElement>(null)

  React.useEffect(() => {
    if (modifiers.focused) ref.current?.focus()
  }, [modifiers.focused])

  return (
    <Button
      ref={ref}
      variant="ghost"
      size="icon"
      data-day={day.date.toLocaleDateString()}
      data-selected-single={
        modifiers.selected && !modifiers.range_start && !modifiers.range_end && !modifiers.range_middle
          ? "true"
          : undefined
      }
      data-range-start={modifiers.range_start ? "true" : undefined}
      data-range-end={modifiers.range_end ? "true" : undefined}
      data-range-middle={modifiers.range_middle ? "true" : undefined}
      className={cn(
        "relative flex h-8 w-full min-w-[--cell-size] flex-col items-center justify-center gap-0.5 rounded-sm font-normal leading-none",
        "hover:bg-muted hover:text-foreground",
        "data-[selected-single=true]:bg-primary data-[selected-single=true]:text-primary-foreground data-[selected-single=true]:hover:bg-primary data-[selected-single=true]:hover:text-primary-foreground",
        "data-[range-start=true]:bg-primary data-[range-start=true]:text-primary-foreground data-[range-start=true]:hover:bg-primary data-[range-start=true]:hover:text-primary-foreground",
        "data-[range-end=true]:bg-primary data-[range-end=true]:text-primary-foreground data-[range-end=true]:hover:bg-primary data-[range-end=true]:hover:text-primary-foreground",
        "data-[range-middle=true]:bg-primary/10 data-[range-middle=true]:text-primary data-[range-middle=true]:hover:bg-primary/15 data-[range-middle=true]:hover:text-primary",
        modifiers.disabled && "bg-muted/60",
        defaultClassNames.day,
        className,
      )}
      {...props}
    >
      {children}
      {modifiers.today ? (
        <span
          aria-hidden
          className={cn(
            "h-1.5 w-1.5 rounded-full bg-primary",
            (modifiers.selected || modifiers.range_start || modifiers.range_end) && "bg-primary-foreground",
          )}
        />
      ) : null}
    </Button>
  )
}

export { Calendar, CalendarDayButton }
