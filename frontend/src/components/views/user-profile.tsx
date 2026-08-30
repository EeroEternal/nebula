import { useEffect, useState } from 'react'
import { UserRound } from 'lucide-react'
import { Badge } from '@/components/ui/badge'
import { Button } from '@/components/ui/button'
import { Input } from '@/components/ui/input'
import { Label } from '@/components/ui/label'
import { authApi } from '@/lib/api'
import type { AuthUser } from '@/lib/types'
import { useI18n } from '@/lib/useI18n'

interface UserProfileViewProps {
  token: string
  user: AuthUser | null
  onProfileUpdated: () => Promise<void>
}

export function UserProfileView({ token, user, onProfileUpdated }: UserProfileViewProps) {
  const { t } = useI18n()
  const [name, setName] = useState('')
  const [email, setEmail] = useState('')
  const [saved, setSaved] = useState(false)
  const [saving, setSaving] = useState(false)
  const [error, setError] = useState<string | null>(null)

  useEffect(() => {
    setName(user?.display_name || user?.username || '')
    setEmail(user?.email || '')
  }, [user])

  const onSave = async () => {
    setSaving(true)
    setError(null)
    try {
      await authApi.updateProfile(
        {
          display_name: name.trim(),
          email: email.trim(),
        },
        token,
      )
      await onProfileUpdated()
      setSaved(true)
      setTimeout(() => setSaved(false), 1200)
    } catch (err) {
      setError(err instanceof Error ? err.message : t('profile.saveFailed'))
    } finally {
      setSaving(false)
    }
  }

  return (
    <div className="max-w-2xl space-y-6 animate-in fade-in slide-in-from-bottom-4 duration-700">
      <div>
        <h2 className="text-2xl font-bold text-foreground">{t('profile.title')}</h2>
        <p className="text-sm text-muted-foreground mt-1">{t('profile.subtitle')}</p>
      </div>

      <div className="bg-card border border-border rounded-2xl p-6 space-y-5">
        <div className="flex items-center justify-between">
          <div className="flex items-center gap-2.5">
            <div className="h-9 w-9 rounded-xl bg-primary/10 text-primary flex items-center justify-center">
              <UserRound className="h-4.5 w-4.5" />
            </div>
            <p className="font-semibold text-foreground">{t('profile.details')}</p>
          </div>
          <Badge variant="outline" className="text-[10px] uppercase">PostgreSQL</Badge>
        </div>

        <div className="space-y-2">
          <Label htmlFor="profile-name">{t('profile.displayName')}</Label>
          <Input id="profile-name" value={name} onChange={(e) => setName(e.target.value)} placeholder={t('profile.namePlaceholder')} />
        </div>

        <div className="space-y-2">
          <Label htmlFor="profile-email">{t('profile.email')}</Label>
          <Input id="profile-email" type="email" value={email} onChange={(e) => setEmail(e.target.value)} placeholder={t('profile.emailPlaceholder')} />
        </div>

        {error && <p className="text-sm text-destructive">{error}</p>}

        <div className="flex items-center justify-between pt-2">
          <p className="text-xs text-muted-foreground">{t('profile.persistHint')}</p>
          <Button onClick={onSave} className="rounded-xl" disabled={saving}>
            {saving ? t('profile.saving') : saved ? t('profile.saved') : t('profile.saveChanges')}
          </Button>
        </div>
      </div>
    </div>
  )
}
