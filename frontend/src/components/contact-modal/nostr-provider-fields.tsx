"use client"

import { Input } from "@/components/ui/input"
import { Label } from "@/components/ui/label"
import { useTranslations } from "next-intl"

interface NostrProviderFieldsProps {
  recipient: string
  onRecipientChange: (recipient: string) => void
  disabled?: boolean
}

export function NostrProviderFields({
  recipient,
  onRecipientChange,
  disabled = false,
}: NostrProviderFieldsProps) {
  const t = useTranslations("contacts")

  return (
    <div className="mt-2 space-y-2">
      <div>
        <Label htmlFor="nostr-recipient">{t("add.nostr.recipientLabel")}</Label>
        <Input
          id="nostr-recipient"
          value={recipient}
          onChange={(event) => onRecipientChange(event.target.value)}
          placeholder={t("add.nostr.recipientPlaceholder")}
          disabled={disabled}
          spellCheck={false}
          autoCapitalize="none"
          autoCorrect="off"
        />
        <p className="mt-1 text-xs text-muted-foreground">
          {t("add.nostr.recipientHint")}
        </p>
      </div>
    </div>
  )
}
