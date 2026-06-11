"use client"

import type { ReactNode } from "react"
import { Input } from "@/components/ui/input"
import { Label } from "@/components/ui/label"
import { useTranslations } from "next-intl"

interface NtfyProviderFieldsProps {
  topic: string
  onTopicChange: (topic: string) => void
  defaultTopicPlaceholder: string
  disabled?: boolean
  ntfyServerUrl?: string
  ntfyServerIsBrowserSafe?: boolean
  containerClassName?: string
  leadingControl?: ReactNode
  inline?: boolean
}

export function NtfyProviderFields({
  topic,
  onTopicChange,
  defaultTopicPlaceholder,
  disabled = false,
  ntfyServerUrl,
  ntfyServerIsBrowserSafe = true,
  containerClassName = "mt-2 space-y-2",
  leadingControl,
  inline = false,
}: NtfyProviderFieldsProps) {
  const t = useTranslations('contacts')

  // Extract hostname for display (e.g., "https://ntfy.example.com" → "ntfy.example.com")
  let serverDisplay = 'ntfy.sh'
  if (!ntfyServerIsBrowserSafe) {
    serverDisplay = t('add.ntfy.localServer')
  } else if (ntfyServerUrl) {
    try {
      serverDisplay = new URL(ntfyServerUrl).hostname
    } catch {
      serverDisplay = ntfyServerUrl.replace(/^https?:\/\//, '').replace(/\/+$/, '')
    }
  }

  return (
    <div className={containerClassName}>
      <div>
        {!inline && <Label htmlFor="ntfy-topic">{t('add.ntfy.topicLabel')}</Label>}
        <div className={inline ? "flex items-start gap-2" : undefined}>
          {inline && leadingControl}
          <Input
            id="ntfy-topic"
            value={topic}
            onChange={(e) => onTopicChange(e.target.value)}
            placeholder={defaultTopicPlaceholder}
            disabled={disabled}
            aria-label={inline ? t('add.ntfy.topicLabel') : undefined}
            className={inline ? "min-w-0 flex-1" : undefined}
          />
        </div>
        <p className="text-xs text-muted-foreground mt-1">
          {t('add.ntfy.topicHint', { server: serverDisplay })}
        </p>
      </div>
    </div>
  )
}
