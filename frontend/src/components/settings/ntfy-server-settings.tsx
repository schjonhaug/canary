"use client"

import { useState, useCallback, useEffect } from "react"
import Image from "next/image"
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from "@/components/ui/card"
import { Button } from "@/components/ui/button"
import { Label } from "@/components/ui/label"
import { Input } from "@/components/ui/input"
import { RadioGroup } from "@/components/ui/radio-group"
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from "@/components/ui/select"
import { ErrorDisplay, SuccessDisplay } from "@/components/ui/error-display"
import { EndpointOption } from "@/components/settings/endpoint-option"
import { Bell } from "lucide-react"
import { useTranslations } from "next-intl"
import { api } from "@/lib/api"
import type { UserPreferences, NtfyAuthType } from "@/hooks/useUserPreferences"
import { isBrowserSafeNtfyUrl, type NtfyServerOption } from "@/lib/ntfy-servers"

const NTFY_CUSTOM_ENDPOINT_ID = "ntfy-custom"

export interface NtfyServerSettingsProps {
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
  onNtfySettingsSave: () => boolean | Promise<boolean> | void | Promise<void>
  onClearNtfySettingsErrors: () => void
}

export function NtfyServerSettings(props: NtfyServerSettingsProps) {
  const t = useTranslations("settings")

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
        <NtfyServerSettingsContent {...props} />
      </CardContent>
    </Card>
  )
}

export function NtfyServerSettingsContent({
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
  showEndpointProviderFrame = true,
}: NtfyServerSettingsProps & { showEndpointProviderFrame?: boolean }) {
  const t = useTranslations("settings")
  const tCommon = useTranslations("common")
  if (ntfyServers.length === 0) {
    return null
  }

  const publicServers = ntfyServers.filter((server) => !server.isLocal)
  const localServers = ntfyServers.filter((server) => server.isLocal)
  const publicServer = publicServers[0]
  const selectedServer = ntfyServers.find((server) => server.id === selectedNtfyServerId)
  const isLocalSelection = localServers.some((server) => server.id === selectedNtfyServerId)
  const isManagedAuthSelection = selectedServer?.managedAuth === true
  const platformLabels: Record<string, string> = {
    umbrel: t("ntfy.platform.umbrel"),
    startos: t("ntfy.platform.startos"),
  }
  // ntfy.sh supports accounts/private topics, so auth stays available for both public and local servers.
  const showAuthSection = Boolean(ntfyServerUrl || isLocalSelection) && !isManagedAuthSelection
  const localServerSubtitle = (server: NtfyServerOption) =>
    server.platform ? (platformLabels[server.platform] ?? t("ntfy.platform.local")) : t("ntfy.platform.local")
  const publicBaseUrl = publicServer?.baseUrl ?? "https://ntfy.sh"
  const providerName = publicServer?.name ?? localServers[0]?.name ?? "ntfy"
  const selectedEndpointId = isLocalSelection
    ? selectedNtfyServerId
    : normalizeEditablePublicUrl(ntfyServerUrl) === normalizeEditablePublicUrl(publicBaseUrl)
      ? (publicServer?.id ?? NTFY_CUSTOM_ENDPOINT_ID)
      : NTFY_CUSTOM_ENDPOINT_ID

  const handleEndpointChange = (endpointId: string) => {
    onClearNtfySettingsErrors()
    if (endpointId === NTFY_CUSTOM_ENDPOINT_ID) {
      onNtfyServerChange(publicServer?.id ?? NTFY_CUSTOM_ENDPOINT_ID)
      if (
        isLocalSelection ||
        normalizeEditablePublicUrl(ntfyServerUrl) === normalizeEditablePublicUrl(publicBaseUrl)
      ) {
        onNtfyServerUrlChange("")
      }
      return
    }

    const server = ntfyServers.find((option) => option.id === endpointId)
    if (!server) return

    onNtfyServerChange(server.id)
    if (!server.isLocal) {
      onNtfyServerUrlChange(server.baseUrl)
    }
  }

  const endpointOptions = (
    <div className="space-y-2 pt-1">
      {localServers.map((server) => (
        <EndpointOption
          key={server.id}
          id={`ntfy-server-${server.id}`}
          value={server.id}
          label={localServerSubtitle(server)}
        />
      ))}
      {publicServer ? (
        <EndpointOption
          id={`ntfy-server-${publicServer.id}`}
          value={publicServer.id}
          label={publicServer.baseUrl}
        />
      ) : null}
      <div className="space-y-2">
        <EndpointOption
          id="ntfy-server-custom"
          value={NTFY_CUSTOM_ENDPOINT_ID}
          label={t("ntfy.customUrl")}
        />
        {selectedEndpointId === NTFY_CUSTOM_ENDPOINT_ID && (
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
            className="ml-6 max-w-xl"
          />
        )}
      </div>
    </div>
  )

  return (
    <div className="space-y-6">
      {publicServer || localServers.length > 0 ? (
        <RadioGroup
          value={selectedEndpointId}
          onValueChange={handleEndpointChange}
          disabled={isUpdatingNtfySettings}
          className="space-y-3"
        >
          {showEndpointProviderFrame ? (
            <div className="rounded-md border p-3">
              <div className="flex items-start gap-3">
                <div className="flex h-8 w-8 shrink-0 items-center justify-center overflow-hidden rounded-md border bg-background">
                  <Image
                      src="/images/notifications/ntfy.svg"
                    alt="ntfy logo"
                    width={32}
                    height={32}
                    className="h-full w-full object-contain"
                  />
                </div>
                <div className="min-w-0 flex-1 space-y-2">
                  <Label>{providerName}</Label>
                  {endpointOptions}
                </div>
              </div>
            </div>
          ) : (
            endpointOptions
          )}
        </RadioGroup>
      ) : null}

      {isLocalSelection && !isManagedAuthSelection && (
        <p className="text-sm text-muted-foreground">{t("ntfy.localAuthNote")}</p>
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
            defaultTopic={selectedServer?.defaultTopic}
          />
    </div>
  )
}

function normalizeEditablePublicUrl(url: string): string {
  return url.trim().replace(/\/+$/, "")
}

function TestNotificationSection({
  savedServerUrl,
  hasUnsavedSettings,
  defaultTopic,
}: {
  savedServerUrl: string | null
  hasUnsavedSettings: boolean
  defaultTopic?: string
}) {
  const t = useTranslations("settings")
  const [topic, setTopic] = useState(defaultTopic || "canary-test")
  const [hasUserEditedTopic, setHasUserEditedTopic] = useState(false)
  const [isSending, setIsSending] = useState(false)
  const [result, setResult] = useState<{ success: boolean; message: string; topicUrl?: string } | null>(null)

  useEffect(() => {
    if (!hasUserEditedTopic) {
      setTopic(defaultTopic || "canary-test")
    }
  }, [defaultTopic, hasUserEditedTopic])

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
          onChange={(e) => {
            setHasUserEditedTopic(true)
            setTopic(e.target.value)
          }}
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
