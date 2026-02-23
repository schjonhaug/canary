"use client"

import { useState, useCallback } from "react"
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from "@/components/ui/card"
import { Button } from "@/components/ui/button"
import { Label } from "@/components/ui/label"
import { Input } from "@/components/ui/input"
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from "@/components/ui/select"
import { Bell } from "lucide-react"
import { useTranslations } from "next-intl"
import { api } from "@/lib/api"
import type { UserPreferences, NtfyAuthType } from "@/hooks/useUserPreferences"

interface NtfyServerSettingsProps {
  ntfyServerUrl: string
  onNtfyServerUrlChange: (url: string) => void

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

  // Consolidated save
  hasAnyNtfyChanges: boolean
  isUpdatingNtfySettings: boolean
  ntfySettingsError: string | null
  ntfySettingsSuccess: boolean
  onNtfySettingsSave: () => void
  onClearNtfySettingsErrors: () => void
}

export function NtfyServerSettings({
  ntfyServerUrl,
  onNtfyServerUrlChange,
  userPreferences,
  ntfyAuthType,
  onNtfyAuthTypeChange,
  ntfyAccessToken,
  onNtfyAccessTokenChange,
  ntfyUsername,
  onNtfyUsernameChange,
  ntfyPassword,
  onNtfyPasswordChange,
  hasAnyNtfyChanges,
  isUpdatingNtfySettings,
  ntfySettingsError,
  ntfySettingsSuccess,
  onNtfySettingsSave,
  onClearNtfySettingsErrors,
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
            <Input
              id="ntfy-server"
              type="url"
              placeholder={t("ntfy.serverPlaceholder")}
              value={ntfyServerUrl}
              onChange={(e) => {
                onNtfyServerUrlChange(e.target.value)
                onClearNtfySettingsErrors()
              }}
              disabled={isUpdatingNtfySettings}
              className="mt-1"
            />
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
                    onClearNtfySettingsErrors()
                  }}
                  disabled={isUpdatingNtfySettings}
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
                        onClearNtfySettingsErrors()
                      }}
                      disabled={isUpdatingNtfySettings}
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
                          onClearNtfySettingsErrors()
                        }}
                        disabled={isUpdatingNtfySettings}
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
                          onClearNtfySettingsErrors()
                        }}
                        disabled={isUpdatingNtfySettings}
                        className="mt-1"
                      />
                    </div>
                  </>
                )}
              </div>
            </div>
          )}

          {/* Consolidated save button - always visible */}
          {ntfySettingsError && <p className="text-sm text-red-500">{ntfySettingsError}</p>}
          {ntfySettingsSuccess && <p className="text-sm text-green-500">{tCommon("savedSuccessfully")}</p>}
          <Button
            onClick={onNtfySettingsSave}
            disabled={!hasAnyNtfyChanges || isUpdatingNtfySettings}
            className="w-full"
          >
            {isUpdatingNtfySettings ? tCommon("saving") : tCommon("save")}
          </Button>

          {/* Test Notification */}
          <TestNotificationSection savedServerUrl={userPreferences?.ntfy_server_url || null} />
        </div>
      </CardContent>
    </Card>
  )
}

function TestNotificationSection({ savedServerUrl }: { savedServerUrl: string | null }) {
  const t = useTranslations("settings")
  const [topic, setTopic] = useState("canary-test")
  const [isSending, setIsSending] = useState(false)
  const [result, setResult] = useState<{ success: boolean; message: string; topicUrl?: string } | null>(null)

  const handleSendTest = useCallback(async () => {
    setIsSending(true)
    setResult(null)
    try {
      const response = await api.sendTestNtfyNotification(topic)
      const serverBase = savedServerUrl || "https://ntfy.sh"
      const topicUrl = `${serverBase.replace(/\/+$/, "")}/${topic.trim()}`
      if (response.success) {
        setResult({ success: true, message: t("ntfy.test.success"), topicUrl })
      } else if (response.error) {
        setResult({ success: false, message: t("ntfy.test.errorWithDetail", { detail: response.error }) })
      } else {
        setResult({ success: false, message: t("ntfy.test.error") })
      }
    } catch {
      setResult({ success: false, message: t("ntfy.test.error") })
    } finally {
      setIsSending(false)
    }
  }, [topic, savedServerUrl, t])

  return (
    <div className="border-t pt-4">
      <Label>{t("ntfy.test.title")}</Label>
      <p className="text-sm text-muted-foreground mb-3">
        {t("ntfy.test.description")}
      </p>
      <div className="flex gap-2">
        <Input
          type="text"
          placeholder={t("ntfy.test.topicPlaceholder")}
          value={topic}
          onChange={(e) => setTopic(e.target.value)}
          disabled={isSending}
        />
        <Button
          onClick={handleSendTest}
          disabled={!topic.trim() || isSending}
          variant="outline"
          className="shrink-0"
        >
          {isSending ? t("ntfy.test.sending") : t("ntfy.test.send")}
        </Button>
      </div>
      {result && (
        <p className={`text-sm mt-2 ${result.success ? "text-green-500" : "text-red-500"}`}>
          {result.message}
          {result.topicUrl && (
            <>
              {" "}
              <a
                href={result.topicUrl}
                target="_blank"
                rel="noopener noreferrer"
                className="text-primary hover:underline"
              >
                {result.topicUrl}
              </a>
            </>
          )}
        </p>
      )}
    </div>
  )
}
