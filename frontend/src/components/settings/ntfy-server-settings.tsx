"use client"

import { useState, useCallback, type ReactNode } from "react"
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from "@/components/ui/card"
import { Button } from "@/components/ui/button"
import { Label } from "@/components/ui/label"
import { Input } from "@/components/ui/input"
import { RadioGroup, RadioGroupItem } from "@/components/ui/radio-group"
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from "@/components/ui/select"
import { ErrorDisplay, SuccessDisplay } from "@/components/ui/error-display"
import { Bell, Pencil } from "lucide-react"
import { useTranslations } from "next-intl"
import { api } from "@/lib/api"
import type { UserPreferences, NtfyAuthType } from "@/hooks/useUserPreferences"
import { isBrowserSafeNtfyUrl, type NtfyServerOption } from "@/lib/ntfy-servers"

interface NtfyServerSettingsProps {
  ntfyServerUrl: string
  onNtfyServerUrlChange: (url: string) => void
  ntfyServers: NtfyServerOption[]
  selectedNtfyServerId: string
  onNtfyServerChange: (serverId: string) => void

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
  ntfyServers,
  selectedNtfyServerId,
  onNtfyServerChange,
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
  const [isEditingPublicUrl, setIsEditingPublicUrl] = useState(false)
  const [publicUrlBeforeEdit, setPublicUrlBeforeEdit] = useState("")

  const publicServers = ntfyServers.filter((server) => !server.isLocal)
  const localServers = ntfyServers.filter((server) => server.isLocal)
  const publicServer = publicServers[0]
  const isLocalSelection = localServers.some((server) => server.id === selectedNtfyServerId)
  // ntfy.sh supports accounts/private topics, so auth stays available for both public and local servers.
  const showAuthSection = Boolean(ntfyServerUrl || isLocalSelection)
  const hasLocalServers = localServers.length > 0
  const publicServerDisplayUrl = isLocalSelection ? publicServer?.baseUrl ?? "" : ntfyServerUrl
  const startEditingPublicUrl = () => {
    setPublicUrlBeforeEdit(publicServerDisplayUrl)
    setIsEditingPublicUrl(true)
  }
  const cancelEditingPublicUrl = () => {
    onNtfyServerUrlChange(publicUrlBeforeEdit)
    onClearNtfySettingsErrors()
    setIsEditingPublicUrl(false)
  }
  const saveEditingPublicUrl = () => {
    setIsEditingPublicUrl(false)
    if (normalizeEditablePublicUrl(ntfyServerUrl) !== normalizeEditablePublicUrl(publicUrlBeforeEdit)) {
      onNtfySettingsSave()
    }
  }
  const localServerSubtitle = (server: NtfyServerOption) =>
    server.platform === "umbrel" ? t("ntfy.platform.umbrel") : t("ntfy.platform.local")
  const publicServerRow = publicServer ? (
    <div className="space-y-2">
      {hasLocalServers ? (
        <NtfyServerOptionRow
          server={publicServer}
          subtitle={renderPublicServerSubtitle()}
          subtitleAction={renderPublicServerAction()}
        />
      ) : (
        <NtfyServerStaticRow
          server={publicServer}
          subtitle={renderPublicServerSubtitle()}
          subtitleAction={renderPublicServerAction()}
        />
      )}
    </div>
  ) : null

  function renderPublicServerSubtitle() {
    return selectedNtfyServerId === publicServer?.id && isEditingPublicUrl ? (
      <Input
        id="ntfy-server"
        aria-label={t("ntfy.serverLabel")}
        type="url"
        placeholder={t("ntfy.serverPlaceholder")}
        value={ntfyServerUrl}
        onChange={(e) => {
          onNtfyServerUrlChange(e.target.value)
          onClearNtfySettingsErrors()
        }}
        disabled={isUpdatingNtfySettings}
        className="h-8"
      />
    ) : (
      publicServerDisplayUrl || publicServer?.baseUrl || ""
    )
  }

  function renderPublicServerAction() {
    if (selectedNtfyServerId !== publicServer?.id) {
      return null
    }

    return isEditingPublicUrl ? (
      <div className="flex shrink-0 items-center gap-2">
        <Button
          type="button"
          variant="outline"
          size="sm"
          onClick={cancelEditingPublicUrl}
          disabled={isUpdatingNtfySettings}
        >
          {tCommon("cancel")}
        </Button>
        <Button
          type="button"
          size="sm"
          onClick={saveEditingPublicUrl}
          disabled={isUpdatingNtfySettings}
        >
          {isUpdatingNtfySettings ? tCommon("saving") : tCommon("save")}
        </Button>
      </div>
    ) : (
      <Button
        type="button"
        variant="outline"
        size="sm"
        onClick={startEditingPublicUrl}
        disabled={isUpdatingNtfySettings}
        className="shrink-0"
      >
        <Pencil className="h-4 w-4" />
        {tCommon("edit")}
      </Button>
    )
  }

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
          {hasLocalServers ? (
            <RadioGroup
              value={selectedNtfyServerId}
              onValueChange={(serverId) => {
                setIsEditingPublicUrl(false)
                onNtfyServerChange(serverId)
              }}
              disabled={isUpdatingNtfySettings}
              className="space-y-4"
            >
              {publicServerRow}
              <div className="space-y-2">
                <div className="space-y-3">
                  {localServers.map((server) => (
                    <NtfyServerOptionRow key={server.id} server={server} subtitle={localServerSubtitle(server)} />
                  ))}
                </div>
                <p className="text-sm text-muted-foreground">{t("ntfy.localAuthNote")}</p>
              </div>
            </RadioGroup>
          ) : (
            publicServerRow
          )}

          {showAuthSection && (
            <div className="border-t pt-4">
              <Label>{t("ntfy.auth.title")}</Label>
              {isLocalSelection && (
                <p className="text-sm text-muted-foreground mb-3">{t("ntfy.auth.localTokenHint")}</p>
              )}
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
          {ntfySettingsError && <ErrorDisplay message={ntfySettingsError} variant="inline" />}
          {ntfySettingsSuccess && <SuccessDisplay message={tCommon("savedSuccessfully")} variant="compact" />}
          <Button
            onClick={onNtfySettingsSave}
            disabled={!hasAnyNtfyChanges || isUpdatingNtfySettings}
            className="w-full"
          >
            {isUpdatingNtfySettings ? tCommon("saving") : tCommon("save")}
          </Button>

          {/* Test Notification */}
          <TestNotificationSection
            savedServerUrl={ntfyServerUrl || userPreferences?.ntfy_server_url || null}
            hasUnsavedSettings={hasAnyNtfyChanges}
          />
        </div>
      </CardContent>
    </Card>
  )
}

function normalizeEditablePublicUrl(url: string): string {
  return url.trim().replace(/\/+$/, "")
}

function NtfyServerOptionRow({
  server,
  subtitle,
  subtitleAction,
}: {
  server: NtfyServerOption
  subtitle: ReactNode
  subtitleAction?: ReactNode
}) {
  return (
    <div className="flex items-start gap-3 rounded-md border p-3">
      <RadioGroupItem value={server.id} id={`ntfy-server-${server.id}`} className="mt-1" />
      <div className="min-w-0 flex-1 space-y-1">
        <div className="space-y-1">
          <div className="min-w-0">
            <Label
              htmlFor={`ntfy-server-${server.id}`}
              className="cursor-pointer"
            >
              {server.name}
            </Label>
          </div>
          <div className="flex items-center justify-between gap-3">
            <div className="min-w-0 flex-1 break-all text-sm text-muted-foreground">{subtitle}</div>
            {subtitleAction}
          </div>
        </div>
      </div>
    </div>
  )
}

// Separate row components keep non-selectable public-only settings outside RadioGroup semantics.
function NtfyServerStaticRow({
  server,
  subtitle,
  subtitleAction,
}: {
  server: NtfyServerOption
  subtitle: ReactNode
  subtitleAction?: ReactNode
}) {
  return (
    <div className="flex items-start gap-3 rounded-md border p-3">
      <div className="min-w-0 flex-1 space-y-1">
        <div className="space-y-1">
          <div className="min-w-0">
            <Label>{server.name}</Label>
          </div>
          <div className="flex items-center justify-between gap-3">
            <div className="min-w-0 flex-1 break-all text-sm text-muted-foreground">{subtitle}</div>
            {subtitleAction}
          </div>
        </div>
      </div>
    </div>
  )
}

function TestNotificationSection({
  savedServerUrl,
  hasUnsavedSettings,
}: {
  savedServerUrl: string | null
  hasUnsavedSettings: boolean
}) {
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
      // Local integrations can resolve server-side while remaining unreachable from the browser.
      const topicUrl = isBrowserSafeNtfyUrl(serverBase)
        ? `${serverBase.replace(/\/+$/, "")}/${topic.trim()}`
        : undefined
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
      <Label htmlFor="ntfy-test-topic">{t("ntfy.test.title")}</Label>
      <p className="text-sm text-muted-foreground mb-3">
        {t("ntfy.test.description")}
      </p>
      <div className="flex gap-2">
        <Input
          id="ntfy-test-topic"
          type="text"
          placeholder={t("ntfy.test.topicPlaceholder")}
          value={topic}
          onChange={(e) => setTopic(e.target.value)}
          disabled={isSending}
        />
        <Button
          onClick={handleSendTest}
          disabled={!topic.trim() || isSending || hasUnsavedSettings}
          variant="outline"
          className="shrink-0"
        >
          {isSending ? t("ntfy.test.sending") : t("ntfy.test.send")}
        </Button>
      </div>
      {result && (
        result.success ? (
          <SuccessDisplay
            className="mt-2"
            message={
              <>
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
              </>
            }
          />
        ) : (
          <ErrorDisplay message={result.message} variant="inline" className="mt-2" />
        )
      )}
    </div>
  )
}
