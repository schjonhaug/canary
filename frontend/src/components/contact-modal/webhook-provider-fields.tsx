"use client"

import { useEffect, useMemo, useRef, useState, type MouseEvent } from "react"
import { CheckCircle2, Loader2, Send } from "lucide-react"
import { Input } from "@/components/ui/input"
import { Button } from "@/components/ui/button"
import { Label } from "@/components/ui/label"
import { api } from "@/lib/api"
import { useTranslations } from "next-intl"

interface WebhookProviderFieldsProps {
  url: string
  onUrlChange: (url: string) => void
  disabled?: boolean
  showTest?: boolean
}

export function validateWebhookUrl(value: string): boolean {
  const trimmed = value.trim()
  if (!trimmed || trimmed.length > 2048) return false
  const authority = trimmed.split("://", 2)[1]
  if (!authority || authority.startsWith("/")) return false
  try {
    const parsed = new URL(trimmed)
    return (
      (parsed.protocol === "http:" || parsed.protocol === "https:") &&
      Boolean(parsed.hostname) &&
      !parsed.username &&
      !parsed.password &&
      !parsed.hash
    )
  } catch {
    return false
  }
}

export function WebhookProviderFields({
  url,
  onUrlChange,
  disabled = false,
  showTest = true,
}: WebhookProviderFieldsProps) {
  const t = useTranslations("contacts")
  const [isTesting, setIsTesting] = useState(false)
  const [result, setResult] = useState<{ success: boolean; message: string } | null>(null)
  const isValid = useMemo(() => validateWebhookUrl(url), [url])
  const currentUrlRef = useRef(url)

  useEffect(() => {
    currentUrlRef.current = url
    setResult(null)
  }, [url])

  const testWebhook = async (event: MouseEvent<HTMLButtonElement>) => {
    event.preventDefault()
    event.stopPropagation()
    if (!isValid || isTesting) return

    setIsTesting(true)
    setResult(null)
    const testedUrl = url.trim()
    try {
      const response = await api.sendTestWebhookNotification(testedUrl)
      if (currentUrlRef.current.trim() !== testedUrl) return
      setResult({
        success: response.success,
        message: response.success
          ? t("add.webhook.testSuccess")
          : t("add.webhook.testError", { detail: response.error || t("add.webhook.unknownError") }),
      })
    } catch (error) {
      if (currentUrlRef.current.trim() !== testedUrl) return
      setResult({
        success: false,
        message: t("add.webhook.testError", {
          detail: error instanceof Error ? error.message : t("add.webhook.unknownError"),
        }),
      })
    } finally {
      setIsTesting(false)
    }
  }

  return (
    <div className="mt-2 space-y-2">
      <div>
        <Label htmlFor="webhook-url">{t("add.webhook.urlLabel")}</Label>
        <div className="flex items-center gap-2">
          <Input
            id="webhook-url"
            type="url"
            value={url}
            onChange={(event) => onUrlChange(event.target.value)}
            placeholder={t("add.webhook.urlPlaceholder")}
            disabled={disabled}
            spellCheck={false}
            autoCapitalize="none"
            autoCorrect="off"
            aria-invalid={Boolean(url.trim()) && !isValid}
          />
          {showTest && (
            <Button
              type="button"
              variant="outline"
              size="sm"
              onClick={testWebhook}
              disabled={disabled || isTesting || !isValid}
            >
              {isTesting ? (
                <Loader2 className="mr-1 h-4 w-4 animate-spin" aria-hidden="true" />
              ) : (
                <Send className="mr-1 h-4 w-4" aria-hidden="true" />
              )}
              {isTesting ? t("add.webhook.testing") : t("add.webhook.test")}
            </Button>
          )}
        </div>
        <p className="mt-1 text-xs text-muted-foreground">{t("add.webhook.urlHint")}</p>
        {url.trim() && !isValid && (
          <p className="mt-1 text-xs text-destructive" role="alert">
            {t("add.webhook.invalidUrl")}
          </p>
        )}
      </div>
      {showTest && result && (
        <p
          className={`flex items-center gap-1 text-xs ${result.success ? "text-green-600" : "text-destructive"}`}
          role={result.success ? "status" : "alert"}
        >
          {result.success && <CheckCircle2 className="h-3.5 w-3.5" aria-hidden="true" />}
          {result.message}
        </p>
      )}
    </div>
  )
}
