import { useState } from "react"
import { Navigate, useNavigate } from "react-router-dom"
import { Eye, EyeOff } from "lucide-react"
import { Button } from "@/components/ui/button"
import { Input } from "@/components/ui/input"
import { Label } from "@/components/ui/label"
import { AuthCardLayout } from "@/components/layout/auth-card-layout"
import { authApi } from "@/lib/api"
import { t, useI18n } from "@/lib/i18n"
import { useAuthStore } from "@/store/useAuthStore"
import { toast } from "sonner"

export function LoginView() {
  useI18n()
  const navigate = useNavigate()
  const token = useAuthStore((s) => s.token)
  const setAuth = useAuthStore((s) => s.setAuth)
  const [username, setUsername] = useState("admin")
  const [password, setPassword] = useState("admin123")
  const [showPassword, setShowPassword] = useState(false)
  const [pending, setPending] = useState(false)

  if (token) return <Navigate to="/" replace />

  async function submit() {
    setPending(true)
    try {
      const result = await authApi.login(username.trim(), password)
      setAuth(result.token, result.user)
      localStorage.setItem("nebula_token", result.token)
      toast.success(t("login.success"))
      navigate("/", { replace: true })
    } catch (err) {
      toast.error(err instanceof Error ? err.message : t("login.failed"))
    } finally {
      setPending(false)
    }
  }

  return (
    <AuthCardLayout
      title={t("login.signIn")}
      footer={
        <p className="text-meta-sm text-muted-foreground">
          {t("login.noAccount")}
          <span className="ml-2 font-medium text-foreground">{t("login.contactAdmin")}</span>
          <span className="mt-2 block">{t("login.demo")}</span>
        </p>
      }
    >
      <form
        className="space-y-4"
        onSubmit={(e) => {
          e.preventDefault()
          void submit()
        }}
      >
        <div className="space-y-2">
          <Label htmlFor="login-username">{t("login.email")}</Label>
          <Input
            id="login-username"
            name="username"
            autoComplete="username"
            value={username}
            onChange={(e) => setUsername(e.target.value)}
            placeholder={t("login.identityPlaceholder")}
            required
            autoFocus
          />
        </div>
        <div className="space-y-2">
          <Label htmlFor="login-password">{t("login.password")}</Label>
          <div className="relative">
            <Input
              id="login-password"
              name="password"
              type={showPassword ? "text" : "password"}
              autoComplete="current-password"
              value={password}
              onChange={(e) => setPassword(e.target.value)}
              className="pr-10"
              required
            />
            <button
              type="button"
              onClick={() => setShowPassword((open) => !open)}
              className="absolute right-3 top-1/2 -translate-y-1/2 p-1 text-muted-foreground hover:text-foreground"
              aria-label={showPassword ? t("auth.hidePassword") : t("auth.showPassword")}
            >
              {showPassword ? <EyeOff className="h-4 w-4" /> : <Eye className="h-4 w-4" />}
            </button>
          </div>
        </div>
        <Button type="submit" className="w-full" disabled={pending}>
          {pending ? t("login.signingIn") : t("login.signIn")}
        </Button>
      </form>
    </AuthCardLayout>
  )
}
