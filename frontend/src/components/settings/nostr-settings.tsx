"use client"

import { useEffect, useState } from "react"
import { RadioTower } from "lucide-react"
import { useTranslations } from "next-intl"
import { api, ApiError } from "@/lib/api"
import { getTranslatedApiError } from "@/lib/utils"
import { Button } from "@/components/ui/button"
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from "@/components/ui/card"
import { ErrorDisplay, SuccessDisplay } from "@/components/ui/error-display"
import { Input } from "@/components/ui/input"
import { Label } from "@/components/ui/label"
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from "@/components/ui/select"
import type { NostrDmMode } from "@/lib/api"

export function NostrSettings() {
  const t = useTranslations("settings")

  return (
    <Card>
      <CardHeader>
        <CardTitle className="flex items-center gap-2">
          <RadioTower className="h-5 w-5" />
          {t("nostr.title")}
        </CardTitle>
        <CardDescription>{t("nostr.description")}</CardDescription>
      </CardHeader>
      <CardContent>
        <NostrSettingsContent />
      </CardContent>
    </Card>
  )
}

export function NostrSettingsContent() {
  const t = useTranslations("settings")
  const tApiErrors = useTranslations("errors.api")
  const [senderNpub, setSenderNpub] = useState("")
  const [dmMode, setDmMode] = useState<NostrDmMode>("auto")
  const [recipient, setRecipient] = useState("")
  const [isLoading, setIsLoading] = useState(true)
  const [isSavingMode, setIsSavingMode] = useState(false)
  const [isSending, setIsSending] = useState(false)
  const [error, setError] = useState<string | null>(null)
  const [modeError, setModeError] = useState<string | null>(null)
  const [modeSaved, setModeSaved] = useState(false)
  const [success, setSuccess] = useState(false)
  const [successMode, setSuccessMode] = useState<NostrDmMode | null>(null)

  const translateTestError = (errorCode: string | null | undefined, fallback: string | null) => {
    if (errorCode) {
      try {
        const translated = tApiErrors(errorCode)
        if (translated && translated !== `errors.api.${errorCode}` && translated !== errorCode) {
          return translated
        }
      } catch {
        // Fall back to the backend detail below when the locale does not have this key.
      }
    }

    return fallback || t("nostr.test.error")
  }

  useEffect(() => {
    let isMounted = true

    const fetchSettings = async () => {
      try {
        const settings = await api.getNostrSettings()
        if (isMounted) {
          setSenderNpub(settings.sender_npub)
          setDmMode(settings.dm_mode)
        }
      } catch (err) {
        if (isMounted) {
          setError(err instanceof ApiError ? getTranslatedApiError(err, tApiErrors) : t("nostr.loadFailed"))
        }
      } finally {
        if (isMounted) {
          setIsLoading(false)
        }
      }
    }

    fetchSettings()
    return () => {
      isMounted = false
    }
  }, [t, tApiErrors])

  const handleTestSend = async () => {
    const trimmedRecipient = recipient.trim()
    if (!trimmedRecipient) {
      setError(t("nostr.test.recipientRequired"))
      setSuccess(false)
      return
    }

    setIsSending(true)
    setError(null)
    setSuccess(false)
    setSuccessMode(null)

    try {
      const result = await api.sendTestNostrNotification(trimmedRecipient, dmMode)
      if (result.success) {
        setSuccess(true)
        setSuccessMode(result.dm_mode_used ?? dmMode)
      } else {
        setError(translateTestError(result.error_code, result.error))
      }
    } catch (err) {
      setError(err instanceof ApiError ? getTranslatedApiError(err, tApiErrors) : t("nostr.test.error"))
    } finally {
      setIsSending(false)
    }
  }

  const handleModeChange = async (value: string) => {
    const nextMode = value as NostrDmMode
    const previousMode = dmMode
    setDmMode(nextMode)
    setModeError(null)
    setModeSaved(false)
    setError(null)
    setSuccess(false)
    setSuccessMode(null)
    setIsSavingMode(true)

    try {
      const settings = await api.updateNostrSettings(nextMode)
      setDmMode(settings.dm_mode)
      setModeSaved(true)
    } catch (err) {
      setDmMode(previousMode)
      setModeError(err instanceof ApiError ? getTranslatedApiError(err, tApiErrors) : t("nostr.mode.saveFailed"))
    } finally {
      setIsSavingMode(false)
    }
  }

  return (
    <div className="space-y-6">
          <div className="space-y-2">
            <Label htmlFor="nostr-sender">{t("nostr.senderLabel")}</Label>
            <Input
              id="nostr-sender"
              value={isLoading ? t("nostr.loading") : senderNpub}
              readOnly
              spellCheck={false}
              className="font-mono text-xs"
            />
          </div>

          <div className="space-y-2">
            <Label htmlFor="nostr-dm-mode">{t("nostr.mode.label")}</Label>
            <Select value={dmMode} onValueChange={handleModeChange} disabled={isLoading || isSavingMode}>
              <SelectTrigger id="nostr-dm-mode" className="w-full max-w-xs">
                <SelectValue />
              </SelectTrigger>
              <SelectContent>
                <SelectItem value="auto">{t("nostr.mode.auto")}</SelectItem>
                <SelectItem value="nip17">{t("nostr.mode.nip17")}</SelectItem>
                <SelectItem value="nip04">{t("nostr.mode.nip04")}</SelectItem>
              </SelectContent>
            </Select>
            <p className="text-sm text-muted-foreground">{t(`nostr.mode.description.${dmMode}`)}</p>
            {modeError && <ErrorDisplay message={modeError} variant="inline" />}
            {modeSaved && <SuccessDisplay message={t("nostr.mode.saved")} />}
          </div>

          <div className="space-y-3 border-t pt-4">
            <div className="space-y-2">
              <Label htmlFor="nostr-test-recipient">{t("nostr.test.recipientLabel")}</Label>
              <Input
                id="nostr-test-recipient"
                value={recipient}
                onChange={(event) => {
                  setRecipient(event.target.value)
                  setError(null)
                  setSuccess(false)
                }}
                placeholder={t("nostr.test.recipientPlaceholder")}
                disabled={isSending}
                spellCheck={false}
                autoCapitalize="none"
                autoCorrect="off"
              />
            </div>

            {error && <ErrorDisplay message={error} variant="inline" />}
            {success && (
              <SuccessDisplay
                message={t("nostr.test.successWithMode", {
                  mode: t(`nostr.mode.short.${successMode ?? dmMode}`),
                })}
              />
            )}

            <Button onClick={handleTestSend} disabled={isSending || isLoading || isSavingMode}>
              {isSending ? t("nostr.test.sending") : t("nostr.test.send")}
            </Button>
          </div>
    </div>
  )
}
