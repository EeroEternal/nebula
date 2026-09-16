import { useState } from "react"
import { Globe, Shield, UserRound, Users } from "lucide-react"
import { PageShell } from "@/components/layout/page-shell"
import { PageContainer } from "@/components/layout/page-container"
import { PageHeader } from "@/components/layout/page-header"
import { SectionCard } from "@/common/section-card"
import { SettingsSectionNav, type SettingsSection } from "@/components/settings/SettingsSectionNav"
import { Button } from "@/components/ui/button"
import { Badge } from "@/components/ui/badge"
import { Select } from "@/components/ui/select"
import { Label } from "@/components/ui/label"
import { useI18n } from "@/lib/useI18n"
import { useAuthStore } from "@/store/useAuthStore"
import { toast } from "sonner"
import { UserProfileView } from "@/components/views/user-profile"
import { AccountSettingsView } from "@/components/views/account-settings"

export function SettingsView() {
    const { t, locale, setLocale } = useI18n()
    const { logout, user, token } = useAuthStore()
    const [section, setSection] = useState("session")

    const sections: SettingsSection[] = [
        { id: "session", label: t("settings.identityAccess"), icon: Shield },
        { id: "profile", label: t("profile.title"), icon: UserRound },
        { id: "account", label: t("account.title"), icon: Users },
        { id: "locale", label: t("settings.localizationProtocol"), icon: Globe },
    ]

    return (
        <PageShell className="overflow-y-auto">
            <PageContainer className="max-w-[1100px] pb-8">
                <PageHeader title={t("settings.title")} />
                <SettingsSectionNav sections={sections} activeSection={section} onSectionChange={setSection} />

                {section === "session" && (
                    <SectionCard title={t("settings.identityAccess")} headerExtra={<Badge variant="outline">{t("settings.activeSession")}</Badge>}>
                        <div className="space-y-4">
                            <div className="flex items-center justify-between">
                                <div>
                                    <p className="text-sm text-muted-foreground">{t("settings.authorizedUser")}</p>
                                    <p className="text-base font-medium">{user?.username || "—"}</p>
                                </div>
                                <Badge>{user?.role || "admin"}</Badge>
                            </div>
                            <div className="flex gap-2">
                                <Button variant="outline" onClick={() => setSection("profile")}>{t("settings.updateCredentials")}</Button>
                                <Button
                                    variant="destructive"
                                    onClick={() => {
                                        logout()
                                        toast.success(t("settings.sessionTerminated"))
                                    }}
                                >
                                    {t("settings.terminateSession")}
                                </Button>
                            </div>
                        </div>
                    </SectionCard>
                )}

                {section === "profile" && token ? (
                    <UserProfileView token={token} user={user} onProfileUpdated={async () => undefined} />
                ) : null}

                {section === "account" && token ? (
                    <AccountSettingsView token={token} user={user} onOpenSecuritySettings={() => setSection("session")} />
                ) : null}

                {section === "locale" && (
                    <SectionCard title={t("settings.localizationProtocol")}>
                        <div className="max-w-sm space-y-2">
                            <Label>{t("settings.interfaceLanguage")}</Label>
                            <Select
                                value={locale}
                                onChange={(v) => setLocale(v as "en" | "zh")}
                                options={[
                                    { value: "en", label: t("settings.englishUs") },
                                    { value: "zh", label: t("settings.simplifiedChinese") },
                                ]}
                            />
                        </div>
                    </SectionCard>
                )}
            </PageContainer>
        </PageShell>
    )
}
