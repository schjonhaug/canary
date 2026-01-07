"use client"

import { Card, CardContent, CardDescription, CardHeader, CardTitle } from "@/components/ui/card"
import { Button } from "@/components/ui/button"
import { Label } from "@/components/ui/label"
import { Input } from "@/components/ui/input"
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from "@/components/ui/select"
import { Bell } from "lucide-react"
import { useTranslations } from "next-intl"
import type { UserPreferences, NtfyAuthType } from "@/hooks/useUserPreferences"

interface NtfyServerSettingsProps {
  ntfyServerUrl: string
  onNtfyServerUrlChange: (url: string) => void
  hasNtfyChanges: boolean
  isUpdatingNtfy: boolean
  ntfyError: string | null
  ntfySuccess: boolean
  onNtfyServerSave: () => void
  onClearNtfyErrors: () => void

  // Auth section
  userPreferences: UserPreferences | null
  ntfyAuthType: NtfyAuthType
  onNtfyAuthTypeChange: (type: NtfyAuthType) => void
  ntfyAccessToken: string
  onNtfyAccessTokenChange: (token: string) => void
  ntfyUsername: string
  onNtfyUsernameChange: (username: string) => void
  ntfyPassword: string
  onNtfyPasswordChange: (password: string) => void
  isUpdatingNtfyAuth: boolean
  ntfyAuthError: string | null
  ntfyAuthSuccess: boolean
  onNtfyAuthSave: () => void
  onClearNtfyAuthErrors: () => void
}

export function NtfyServerSettings({
  ntfyServerUrl,
  onNtfyServerUrlChange,
  hasNtfyChanges,
  isUpdatingNtfy,
  ntfyError,
  ntfySuccess,
  onNtfyServerSave,
  onClearNtfyErrors,
  userPreferences,
  ntfyAuthType,
  onNtfyAuthTypeChange,
  ntfyAccessToken,
  onNtfyAccessTokenChange,
  ntfyUsername,
  onNtfyUsernameChange,
  ntfyPassword,
  onNtfyPasswordChange,
  isUpdatingNtfyAuth,
  ntfyAuthError,
  ntfyAuthSuccess,
  onNtfyAuthSave,
  onClearNtfyAuthErrors,
}: NtfyServerSettingsProps) {
  const t = useTranslations("settings")
  const tCommon = useTranslations("common")

  const showAuthSection = ntfyServerUrl && ntfyServerUrl !== "https://ntfy.sh"

  return (
    <Card>
      <CardHeader>
        <CardTitle className="flex items-center gap-2">
          <Bell className="h-5 w-5" />
          {t("ntfy.title")}
        </CardTitle>
        <CardDescription>{t("ntfy.description")}</CardDescription>
      </CardHeader>
      <CardContent>
        <div className="space-y-6">
          {/* Server URL */}
          <div>
            <Label htmlFor="ntfy-server">{t("ntfy.serverLabel")}</Label>
            <div className="flex gap-2 mt-1">
              <Input
                id="ntfy-server"
                type="url"
                placeholder={t("ntfy.serverPlaceholder")}
                value={ntfyServerUrl}
                onChange={(e) => {
                  onNtfyServerUrlChange(e.target.value)
                  onClearNtfyErrors()
                }}
                disabled={isUpdatingNtfy}
                className="flex-1"
              />
              <Button onClick={onNtfyServerSave} disabled={isUpdatingNtfy || !hasNtfyChanges}>
                {isUpdatingNtfy ? tCommon("saving") : tCommon("save")}
              </Button>
            </div>
            {ntfyError && <p className="text-sm text-red-500 mt-1">{ntfyError}</p>}
            {ntfySuccess && <p className="text-sm text-green-500 mt-1">{tCommon("savedSuccessfully")}</p>}
            <p className="text-sm text-muted-foreground mt-2">
              {t("ntfy.serverNoteBefore")}
              <a
                href="https://ntfy.sh/docs/install/"
                target="_blank"
                rel="noopener noreferrer"
                className="text-primary hover:underline"
              >
                {t("ntfy.selfHostLink")}
              </a>
              {t("ntfy.serverNoteAfter")}
            </p>
          </div>

          {/* Authentication - only show if custom server URL is set */}
          {showAuthSection && (
            <div className="border-t pt-4">
              <Label>{t("ntfy.auth.title")}</Label>
              <p className="text-sm text-muted-foreground mb-3">
                {userPreferences?.ntfy_has_access_token
                  ? t("ntfy.auth.configured.token")
                  : userPreferences?.ntfy_has_credentials
                    ? t("ntfy.auth.configured.credentials", { username: userPreferences.ntfy_username ?? "" })
                    : t("ntfy.auth.configured.none")}
              </p>

              <div className="space-y-3">
                <Select
                  value={ntfyAuthType}
                  onValueChange={(value: NtfyAuthType) => {
                    onNtfyAuthTypeChange(value)
                    onClearNtfyAuthErrors()
                  }}
                  disabled={isUpdatingNtfyAuth}
                >
                  <SelectTrigger>
                    <SelectValue />
                  </SelectTrigger>
                  <SelectContent>
                    <SelectItem value="none">{t("ntfy.auth.type.none")}</SelectItem>
                    <SelectItem value="token">{t("ntfy.auth.type.token")}</SelectItem>
                    <SelectItem value="basic">{t("ntfy.auth.type.basic")}</SelectItem>
                  </SelectContent>
                </Select>

                {ntfyAuthType === "token" && (
                  <div>
                    <Label htmlFor="ntfy-token">{t("ntfy.auth.tokenLabel")}</Label>
                    <Input
                      id="ntfy-token"
                      type="password"
                      placeholder={
                        userPreferences?.ntfy_has_access_token ? "••••••••" : t("ntfy.auth.tokenPlaceholder")
                      }
                      value={ntfyAccessToken}
                      onChange={(e) => {
                        onNtfyAccessTokenChange(e.target.value)
                        onClearNtfyAuthErrors()
                      }}
                      disabled={isUpdatingNtfyAuth}
                      className="mt-1"
                    />
                  </div>
                )}

                {ntfyAuthType === "basic" && (
                  <>
                    <div>
                      <Label htmlFor="ntfy-username">{t("ntfy.auth.usernameLabel")}</Label>
                      <Input
                        id="ntfy-username"
                        type="text"
                        placeholder={t("ntfy.auth.usernamePlaceholder")}
                        value={ntfyUsername}
                        onChange={(e) => {
                          onNtfyUsernameChange(e.target.value)
                          onClearNtfyAuthErrors()
                        }}
                        disabled={isUpdatingNtfyAuth}
                        className="mt-1"
                      />
                    </div>
                    <div>
                      <Label htmlFor="ntfy-password">{t("ntfy.auth.passwordLabel")}</Label>
                      <Input
                        id="ntfy-password"
                        type="password"
                        placeholder={
                          userPreferences?.ntfy_has_credentials ? "••••••••" : t("ntfy.auth.passwordPlaceholder")
                        }
                        value={ntfyPassword}
                        onChange={(e) => {
                          onNtfyPasswordChange(e.target.value)
                          onClearNtfyAuthErrors()
                        }}
                        disabled={isUpdatingNtfyAuth}
                        className="mt-1"
                      />
                    </div>
                  </>
                )}

                <Button onClick={onNtfyAuthSave} disabled={isUpdatingNtfyAuth} className="w-full">
                  {isUpdatingNtfyAuth ? tCommon("saving") : t("ntfy.auth.saveAuth")}
                </Button>

                {ntfyAuthError && <p className="text-sm text-red-500">{ntfyAuthError}</p>}
                {ntfyAuthSuccess && <p className="text-sm text-green-500">{t("ntfy.auth.authSaved")}</p>}
              </div>
            </div>
          )}
        </div>
      </CardContent>
    </Card>
  )
}
