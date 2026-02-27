"use client"

import { useState } from "react"
import { ChevronDown, Copy, Check, Trash2 } from "lucide-react"
import { Collapsible, CollapsibleContent, CollapsibleTrigger } from "@/components/ui/collapsible"
import { Button } from "@/components/ui/button"
import { useTranslations } from "next-intl"
import { useFormatters } from "@/hooks/useFormatters"
import { useRelativeTime } from "@/hooks/useRelativeTime"
import { getDescriptorScriptType, getDescriptorSigningType, getAddressScriptType } from "@/lib/constants"
import type { Wallet } from "@/types"

interface WalletDetailsSectionProps {
  wallet: Wallet
  onDeleteClick?: () => void
}

function CopyButton({ text }: { text: string }) {
  const [copied, setCopied] = useState(false)
  const t = useTranslations("wallets")

  const handleCopy = async () => {
    await navigator.clipboard.writeText(text)
    setCopied(true)
    setTimeout(() => setCopied(false), 2000)
  }

  return (
    <Button
      variant="ghost"
      size="sm"
      onClick={handleCopy}
      className="h-6 w-6 p-0 text-muted-foreground hover:text-foreground"
      title={copied ? t("detail.copied") : undefined}
    >
      {copied ? <Check className="h-3 w-3" /> : <Copy className="h-3 w-3" />}
    </Button>
  )
}

/**
 * Extract the raw address from an addr() descriptor wrapper.
 * "addr(mgL4QMkqR79LqkbzbMESGRaTkmwWBEbNsH)#yn7mnvvj" → "mgL4QMkqR79LqkbzbMESGRaTkmwWBEbNsH"
 */
function extractRawAddress(descriptor: string): string {
  const match = descriptor.match(/^addr\(([^)]+)\)/)
  return match ? match[1] : descriptor
}

function getScriptType(wallet: Wallet): string {
  if (wallet.wallet_type === "address") {
    return getAddressScriptType(extractRawAddress(wallet.descriptor))
  }
  return getDescriptorScriptType(wallet.descriptor)
}

/**
 * Parse a SQLite UTC timestamp string to Unix timestamp (seconds).
 * Handles "YYYY-MM-DD HH:MM:SS.mmm" format (UTC without timezone indicator).
 */
function parseToUnixTimestamp(dateStr: string): number | undefined {
  // SQLite timestamps are UTC but without timezone indicator
  const date = dateStr.match(/^\d{4}-\d{2}-\d{2} \d{2}:\d{2}:\d{2}/)
    ? new Date(dateStr + " UTC")
    : new Date(dateStr)
  const ts = date.getTime()
  return isNaN(ts) ? undefined : Math.floor(ts / 1000)
}

export function WalletDetailsSection({ wallet, onDeleteClick }: WalletDetailsSectionProps) {
  const [isOpen, setIsOpen] = useState(false)
  const t = useTranslations("wallets")
  const { formatDateTime } = useFormatters()
  const lastSyncedUnix = wallet.last_synced_at
    ? parseToUnixTimestamp(wallet.last_synced_at)
    : undefined
  const lastSyncedRelative = useRelativeTime(lastSyncedUnix, 30000)

  const scriptType = getScriptType(wallet)
  const isAddress = wallet.wallet_type === "address"
  const signingType = !isAddress ? getDescriptorSigningType(wallet.descriptor) : null

  // For address wallets, show the raw address; for descriptors, show the full descriptor
  const displayValue = isAddress
    ? extractRawAddress(wallet.descriptor)
    : wallet.descriptor

  // Determine the label for the descriptor/address field
  const descriptorLabel = isAddress
    ? t("detail.addressLabel")
    : t("detail.descriptorLabel")

  // Short script type labels without address hints
  const scriptTypeLabels: Record<string, string> = {
    p2wpkh: t("detail.scriptTypeLabels.p2wpkh"),
    p2sh: t("detail.scriptTypeLabels.p2sh"),
    p2pkh: t("detail.scriptTypeLabels.p2pkh"),
    p2tr: t("detail.scriptTypeLabels.p2tr"),
    p2wsh: t("detail.scriptTypeLabels.p2wsh"),
  }

  return (
    <Collapsible open={isOpen} onOpenChange={setIsOpen}>
      <CollapsibleTrigger asChild>
        <Button
          variant="ghost"
          className="flex items-center justify-between w-full p-0 h-auto font-normal"
        >
          <span className="text-sm font-medium text-muted-foreground">
            {t("detail.walletDetails")}
          </span>
          <ChevronDown
            className={`h-4 w-4 text-muted-foreground transition-transform duration-200 ${isOpen ? "rotate-180" : ""}`}
          />
        </Button>
      </CollapsibleTrigger>
      <CollapsibleContent className="space-y-3 pt-3">
        {/* Descriptor / Address */}
        <div>
          <div className="text-xs text-muted-foreground mb-1">
            {descriptorLabel}
          </div>
          <div className="flex items-start gap-1">
            <span className="font-mono text-xs break-all flex-1">
              {displayValue}
            </span>
            <CopyButton text={displayValue} />
          </div>
        </div>

        {/* Script Type */}
        {scriptType && (
          <div>
            <div className="text-xs text-muted-foreground mb-1">
              {t("detail.scriptType")}
            </div>
            <div className="text-sm">
              {scriptTypeLabels[scriptType] || scriptType}
            </div>
          </div>
        )}

        {/* Signing Type */}
        {!isAddress && (
          <div>
            <div className="text-xs text-muted-foreground mb-1">
              {t("detail.signing")}
            </div>
            <div className="text-sm">
              {signingType
                ? t("detail.multisig", { scheme: signingType })
                : t("detail.singleSig")}
            </div>
          </div>
        )}

        {/* Added */}
        <div>
          <div className="text-xs text-muted-foreground mb-1">
            {t("detail.added")}
          </div>
          <div className="text-sm">{formatDateTime(wallet.created_at)}</div>
        </div>

        {/* Last Synced */}
        {wallet.last_synced_at && lastSyncedRelative && (
          <div>
            <div className="text-xs text-muted-foreground mb-1">
              {t("detail.lastSync")}
            </div>
            <div
              className="text-sm"
              title={formatDateTime(wallet.last_synced_at)}
            >
              {lastSyncedRelative}
            </div>
          </div>
        )}

        {/* Delete Wallet */}
        {onDeleteClick && (
          <div className="pt-2 border-t">
            <Button
              variant="ghost"
              size="sm"
              onClick={onDeleteClick}
              className="text-muted-foreground hover:text-red-600 gap-1.5 px-0"
            >
              <Trash2 size={14} />
              {t("delete.title")}
            </Button>
          </div>
        )}
      </CollapsibleContent>
    </Collapsible>
  )
}
