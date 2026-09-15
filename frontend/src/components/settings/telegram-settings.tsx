"use client"

import { useEffect, useState } from "react"
import { useTranslations } from "next-intl"
import { api, ApiError } from "@/lib/api"
import { getTranslatedApiError } from "@/lib/utils"
import { Button } from "@/components/ui/button"
import { ErrorDisplay, SuccessDisplay } from "@/components/ui/error-display"
import { Input } from "@/components/ui/input"
import { Label } from "@/components/ui/label"
import { validateTelegramChatId } from "@/components/contact-modal/telegram-provider-fields"

export function TelegramSettingsContent() {
  const t = useTranslations("settings")
  const tCommon = useTranslations("common")
  const tApiErrors = useTranslations("errors.api")
  const [configured, setConfigured] = useState(false)
  const [token, setToken] = useState("")
  const [chatId, setChatId] = useState("")
  const [isLoading, setIsLoading] = useState(true)
  const [isSaving, setIsSaving] = useState(false)
  const [isSending, setIsSending] = useState(false)
  const [error, setError] = useState<string | null>(null)
  const [saved, setSaved] = useState(false)
  const [testResult, setTestResult] = useState<{ success: boolean; message: string } | null>(null)

  const hasTokenDraft = token.trim().length > 0
  const canTest = configured && !hasTokenDraft && validateTelegramChatId(chatId)

  useEffect(() => {
    let isMounted = true
    const load = async () => {
      try {
        const settings = await api.getTelegramSettings()
        if (isMounted) setConfigured(settings.configured)
      } catch (err) {
        if (isMounted) {
          setError(err instanceof ApiError ? getTranslatedApiError(err, tApiErrors) : t("telegram.loadFailed"))
        }
      } finally {
        if (isMounted) setIsLoading(false)
      }
    }
    void load()
    return () => {
      isMounted = false
    }
  }, [t, tApiErrors])

  const saveToken = async (nextToken: string) => {
    setIsSaving(true)
    setError(null)
    setSaved(false)
    setTestResult(null)
    try {
      const settings = await api.updateTelegramSettings(nextToken)
      setConfigured(nextToken.trim().length > 0 ? true : settings.configured)
      setToken("")
      setSaved(true)
    } catch (err) {
      setError(err instanceof ApiError ? getTranslatedApiError(err, tApiErrors) : t("telegram.saveFailed"))
    } finally {
      setIsSaving(false)
    }
  }

  const handleTest = async () => {
    if (!canTest || isSending) return
    setIsSending(true)
    setTestResult(null)
    try {
      const response = await api.sendTestTelegramNotification(chatId.trim())
      setTestResult({
        success: response.success,
        message: response.success
          ? t("telegram.test.success")
          : t("telegram.test.error", { detail: response.error || t("telegram.test.unknownError") }),
      })
    } catch (err) {
      setTestResult({
        success: false,
        message: t("telegram.test.error", {
          detail: err instanceof ApiError ? getTranslatedApiError(err, tApiErrors) : t("telegram.test.unknownError"),
        }),
      })
    } finally {
      setIsSending(false)
    }
  }

  return (
    <div className="space-y-4">
      <p className="text-sm text-muted-foreground">{t("telegram.help")}</p>
      <div className="space-y-2">
        <Label htmlFor="telegram-bot-token">{t("telegram.tokenLabel")}</Label>
        <Input
          id="telegram-bot-token"
          type="password"
          autoComplete="off"
          placeholder={configured ? "••••••••" : t("telegram.tokenPlaceholder")}
          value={token}
          onChange={(event) => {
            setToken(event.target.value)
            setSaved(false)
            setError(null)
          }}
          disabled={isLoading || isSaving}
        />
      </div>
      {error && <ErrorDisplay message={error} variant="inline" />}
      {saved && <SuccessDisplay message={tCommon("savedSuccessfully")} variant="compact" />}
      <Button
        onClick={() => void saveToken(token.trim())}
        disabled={!hasTokenDraft || isSaving}
        className="w-full"
      >
        {isSaving ? tCommon("saving") : tCommon("save")}
      </Button>
      {configured && (
        <Button
          type="button"
          variant="ghost"
          className="w-full"
          onClick={() => void saveToken("")}
          disabled={isSaving}
        >
          {t("telegram.removeToken")}
        </Button>
      )}

      <div className="border-t pt-4">
        <Label htmlFor="telegram-test-chat">{t("telegram.test.chatIdLabel")}</Label>
        <p className="mb-3 text-sm text-muted-foreground">{t("telegram.test.description")}</p>
        <div className="flex gap-2">
          <Input
            id="telegram-test-chat"
            placeholder={t("telegram.test.chatIdPlaceholder")}
            value={chatId}
            onChange={(event) => {
              setChatId(event.target.value)
              setTestResult(null)
            }}
            disabled={isSending}
          />
          <Button
            type="button"
            variant="outline"
            className="shrink-0"
            onClick={() => void handleTest()}
            disabled={!canTest || isSending}
          >
            {isSending ? t("telegram.test.sending") : t("telegram.test.send")}
          </Button>
        </div>
        {testResult &&
          (testResult.success ? (
            <SuccessDisplay className="mt-2" message={testResult.message} variant="compact" />
          ) : (
            <ErrorDisplay className="mt-2" message={testResult.message} variant="inline" />
          ))}
      </div>
    </div>
  )
}
