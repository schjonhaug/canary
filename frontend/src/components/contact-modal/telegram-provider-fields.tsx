"use client"

import { useEffect, useMemo, useRef, useState, type MouseEvent } from "react"
import { CheckCircle2, Loader2, Send } from "lucide-react"
import { Input } from "@/components/ui/input"
import { Button } from "@/components/ui/button"
import { Label } from "@/components/ui/label"
import { api } from "@/lib/api"
import { useTranslations } from "next-intl"

interface TelegramProviderFieldsProps {
  chatId: string
  onChatIdChange: (chatId: string) => void
  disabled?: boolean
  showTest?: boolean
}

export function validateTelegramChatId(value: string): boolean {
  const chatId = value.trim()
  if (!chatId) return false
  if (chatId.startsWith("@")) {
    const username = chatId.slice(1)
    return (
      username.length >= 5 &&
      username.length <= 32 &&
      /^[A-Za-z][A-Za-z0-9_]*$/.test(username)
    )
  }
  return /^-?\d{1,20}$/.test(chatId)
}

export function TelegramProviderFields({
  chatId,
  onChatIdChange,
  disabled = false,
  showTest = true,
}: TelegramProviderFieldsProps) {
  const t = useTranslations("contacts")
  const [isTesting, setIsTesting] = useState(false)
  const [result, setResult] = useState<{ success: boolean; message: string } | null>(null)
  const isValid = useMemo(() => validateTelegramChatId(chatId), [chatId])
  const currentChatIdRef = useRef(chatId)

  useEffect(() => {
    currentChatIdRef.current = chatId
    setResult(null)
  }, [chatId])

  const testTelegram = async (event: MouseEvent<HTMLButtonElement>) => {
    event.preventDefault()
    event.stopPropagation()
    if (!isValid || isTesting) return

    setIsTesting(true)
    setResult(null)
    const testedChatId = chatId.trim()
    try {
      const response = await api.sendTestTelegramNotification(testedChatId)
      if (currentChatIdRef.current.trim() !== testedChatId) return
      setResult({
        success: response.success,
        message: response.success
          ? t("add.telegram.testSuccess")
          : t("add.telegram.testError", { detail: response.error || t("add.telegram.unknownError") }),
      })
    } catch (error) {
      if (currentChatIdRef.current.trim() !== testedChatId) return
      setResult({
        success: false,
        message: t("add.telegram.testError", {
          detail: error instanceof Error ? error.message : t("add.telegram.unknownError"),
        }),
      })
    } finally {
      setIsTesting(false)
    }
  }

  return (
    <div className="mt-2 space-y-2">
      <div>
        <Label htmlFor="telegram-chat-id">{t("add.telegram.chatIdLabel")}</Label>
        <div className="flex items-center gap-2">
          <Input
            id="telegram-chat-id"
            value={chatId}
            onChange={(event) => onChatIdChange(event.target.value)}
            placeholder={t("add.telegram.chatIdPlaceholder")}
            disabled={disabled}
            autoComplete="off"
            spellCheck={false}
            autoCapitalize="none"
            autoCorrect="off"
            aria-invalid={Boolean(chatId.trim()) && !isValid}
          />
          {showTest && (
            <Button
              type="button"
              variant="outline"
              size="sm"
              onClick={testTelegram}
              disabled={disabled || isTesting || !isValid}
            >
              {isTesting ? (
                <Loader2 className="mr-1 h-4 w-4 animate-spin" aria-hidden="true" />
              ) : (
                <Send className="mr-1 h-4 w-4" aria-hidden="true" />
              )}
              {isTesting ? t("add.telegram.testing") : t("add.telegram.test")}
            </Button>
          )}
        </div>
        <p className="mt-1 text-xs text-muted-foreground">{t("add.telegram.chatIdHint")}</p>
        {chatId.trim() && !isValid && (
          <p className="mt-1 text-xs text-destructive" role="alert">
            {t("add.telegram.invalidChatId")}
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
