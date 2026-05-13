"use client"

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
}

export function NtfyProviderFields({
  topic,
  onTopicChange,
  defaultTopicPlaceholder,
  disabled = false,
  ntfyServerUrl,
  ntfyServerIsBrowserSafe = true,
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
    <div className="mt-2 space-y-2">
      <div>
        <Label htmlFor="ntfy-topic">{t('add.ntfy.topicLabel')}</Label>
        <Input
          id="ntfy-topic"
          value={topic}
          onChange={(e) => onTopicChange(e.target.value)}
          placeholder={defaultTopicPlaceholder}
          disabled={disabled}
        />
        <p className="text-xs text-muted-foreground mt-1">
          {t('add.ntfy.topicHint', { server: serverDisplay })}
        </p>
      </div>
    </div>
  )
}
