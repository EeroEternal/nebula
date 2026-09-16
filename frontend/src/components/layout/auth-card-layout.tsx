import type { ReactNode } from "react"
import { Toaster } from "@/components/ui/sonner"
import { LanguageSwitcher } from "@/components/layout/language-switcher"
import { t, useI18n } from "@/lib/i18n"

const revealStyle = (delayMs: number) => ({
  animationDelay: `${delayMs}ms`,
  animationFillMode: "both" as const,
})

interface AuthCardLayoutProps {
  title: string
  children: ReactNode
  footer?: ReactNode
}

function BrandMark() {
  return (
    <div className="flex h-10 w-10 items-center justify-center rounded-lg bg-primary text-primary-foreground">
      <svg
        className="h-5 w-5"
        viewBox="0 0 24 24"
        fill="none"
        stroke="currentColor"
        strokeWidth="2.2"
        strokeLinecap="round"
        strokeLinejoin="round"
        aria-hidden="true"
      >
        <path d="M4 7h16" />
        <path d="M4 12h10" />
        <path d="M4 17h7" />
        <path d="M19 14v6" />
        <path d="M16 17h6" />
      </svg>
    </div>
  )
}

export function AuthCardLayout({ title, children, footer }: AuthCardLayoutProps) {
  useI18n()
  const loginStats = [
    { value: "etcd", label: t("auth.statRules") },
    { value: "Rust", label: t("auth.statSkills") },
    { value: "HA", label: t("auth.statScaffold") },
  ]

  return (
    <div className="flex min-h-screen overflow-hidden bg-background">
      <div className="absolute right-4 top-4 z-50">
        <LanguageSwitcher />
      </div>

      <div className="relative hidden items-center justify-center overflow-hidden bg-muted/30 lg:flex lg:w-1/2">
        <div className="absolute inset-0 z-0">
          <div className="absolute right-[-10%] top-[10%] h-[60%] w-[60%] rounded-full bg-primary/10 blur-[120px]" />
          <div className="absolute bottom-[-10%] left-[-10%] h-[40%] w-[40%] rounded-full bg-accent/5 blur-[100px]" />
        </div>

        <div className="relative z-10 px-12 xl:px-24">
          <div
            className="mb-10 flex items-center gap-3 animate-in fade-in slide-in-from-bottom-5 duration-700"
            style={revealStyle(0)}
          >
            <BrandMark />
            <span className="text-xl font-semibold tracking-tight text-foreground">
              {t("auth.productName")}
            </span>
          </div>

          <h1
            className="mb-4 text-3xl font-semibold leading-tight text-foreground animate-in fade-in slide-in-from-bottom-5 duration-700"
            style={revealStyle(100)}
          >
            {t("auth.brandingTitle")} <br />
            <span className="text-primary">{t("auth.brandingSubtitle")}</span>
          </h1>

          <p
            className="mb-16 max-w-lg text-body-md leading-relaxed text-muted-foreground animate-in fade-in slide-in-from-bottom-5 duration-700"
            style={revealStyle(200)}
          >
            {t("auth.brandingDescription")}
          </p>

          <div
            className="flex gap-12 animate-in fade-in slide-in-from-bottom-5 duration-700"
            style={revealStyle(300)}
          >
            {loginStats.map((item) => (
              <div key={item.label} className="flex flex-col gap-1">
                <div className="text-metric text-foreground">{item.value}</div>
                <div className="text-meta-sm text-muted-foreground">{item.label}</div>
              </div>
            ))}
          </div>
        </div>
      </div>

      <div className="relative flex flex-1 items-center justify-center overflow-y-auto bg-background p-6">
        <div className="w-full max-w-[440px]">
          <div className="rounded-lg border border-border bg-card p-8 shadow-sm">
            <div className="animate-in fade-in slide-in-from-right-4 duration-300">
              <div className="mb-6 text-center">
                <h2 className="text-page-title text-foreground">{title}</h2>
              </div>
              {children}
            </div>

            {footer ? <div className="mt-6 text-center">{footer}</div> : null}
          </div>
        </div>
      </div>
      <Toaster position="top-center" richColors />
    </div>
  )
}
