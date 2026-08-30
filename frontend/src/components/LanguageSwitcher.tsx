import { Languages } from 'lucide-react'
import { Button } from '@/components/ui/button'
import { useI18n } from '@/lib/useI18n'
import { cn } from '@/lib/utils'

type LanguageSwitcherProps = {
  className?: string
}

export function LanguageSwitcher({ className }: LanguageSwitcherProps) {
  const { locale, setLocale, t } = useI18n()
  const nextLocale = locale === 'zh' ? 'en' : 'zh'

  return (
    <Button
      type="button"
      variant="outline"
      size="sm"
      className={cn(
        'h-9 gap-2 border-border/60 bg-background/80 px-3 font-mono text-[10px] uppercase tracking-widest backdrop-blur-xl',
        className,
      )}
      aria-label={locale === 'zh' ? t('lang.switchToEnglish') : t('lang.switchToChinese')}
      title={locale === 'zh' ? t('lang.switchToEnglish') : t('lang.switchToChinese')}
      onClick={() => setLocale(nextLocale)}
    >
      <Languages className="h-4 w-4 text-primary" aria-hidden="true" />
      <span>{locale === 'zh' ? 'EN' : '中文'}</span>
    </Button>
  )
}