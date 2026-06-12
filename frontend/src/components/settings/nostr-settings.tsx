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

export function NostrSettings() {
  const t = useTranslations("settings")
  const tApiErrors = useTranslations("errors.api")
  const [senderNpub, setSenderNpub] = useState("")
  const [recipient, setRecipient] = useState("")
  const [isLoading, setIsLoading] = useState(true)
  const [isSending, setIsSending] = useState(false)
  const [error, setError] = useState<string | null>(null)
  const [success, setSuccess] = useState(false)

  useEffect(() => {
    let isMounted = true

    const fetchSettings = async () => {
      try {
        const settings = await api.getNostrSettings()
        if (isMounted) {
          setSenderNpub(settings.sender_npub)
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

    try {
      const result = await api.sendTestNostrNotification(trimmedRecipient)
      if (result.success) {
        setSuccess(true)
      } else {
        setError(result.error || t("nostr.test.error"))
      }
    } catch (err) {
      setError(err instanceof ApiError ? getTranslatedApiError(err, tApiErrors) : t("nostr.test.error"))
    } finally {
      setIsSending(false)
    }
  }

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
            {success && <SuccessDisplay message={t("nostr.test.success")} />}

            <Button onClick={handleTestSend} disabled={isSending || isLoading}>
              {isSending ? t("nostr.test.sending") : t("nostr.test.send")}
            </Button>
          </div>
        </div>
      </CardContent>
    </Card>
  )
}
