"use client"

import Image from "next/image"
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from "@/components/ui/card"
import { Label } from "@/components/ui/label"
import { RadioGroup, RadioGroupItem } from "@/components/ui/radio-group"
import { ExternalLink } from "lucide-react"
import { useTranslations } from "next-intl"
import type { TxExplorerOption } from "@/lib/tx-explorers"

const EXPLORER_LOGOS: Record<string, string> = {
  "mempool-space": "/images/explorers/mempool.svg",
  mempool: "/images/explorers/mempool.svg",
  "bitfeed-public": "/images/explorers/bitfeed.svg",
  bitfeed: "/images/explorers/bitfeed.svg",
  "btc-rpc-explorer-public": "/images/explorers/btc-rpc-explorer.svg",
  "btc-rpc-explorer": "/images/explorers/btc-rpc-explorer.svg",
}

interface TxExplorerSettingsProps {
  explorers: TxExplorerOption[]
  selectedExplorerId: string
  isUpdating: boolean
  onExplorerChange: (explorerId: string) => void
}

export function TxExplorerSettings({
  explorers,
  selectedExplorerId,
  isUpdating,
  onExplorerChange,
}: TxExplorerSettingsProps) {
  const t = useTranslations("settings")
  const publicExplorers = explorers.filter((explorer) => !explorer.isLocal)
  const localExplorers = explorers.filter((explorer) => explorer.isLocal)

  if (explorers.length < 2) {
    return null
  }

  const renderExplorerOption = (explorer: TxExplorerOption) => (
    <div key={explorer.id} className="flex items-start gap-3 rounded-md border p-3">
      <RadioGroupItem value={explorer.id} id={`tx-explorer-${explorer.id}`} className="mt-1" />
      <div className="flex min-w-0 flex-1 items-start gap-3">
        <div className="flex h-8 w-8 shrink-0 items-center justify-center overflow-hidden rounded-md border bg-background">
          {EXPLORER_LOGOS[explorer.id] ? (
            <Image
              src={EXPLORER_LOGOS[explorer.id]}
              alt={`${explorer.name} logo`}
              width={32}
              height={32}
              className="h-full w-full object-contain"
            />
          ) : (
            <ExternalLink className="h-4 w-4 text-muted-foreground" aria-hidden="true" />
          )}
        </div>
        <div className="min-w-0 space-y-1">
          <Label htmlFor={`tx-explorer-${explorer.id}`} className="cursor-pointer">
            {explorer.name}
          </Label>
          <p className="break-all text-sm text-muted-foreground">{explorer.baseUrl}</p>
        </div>
      </div>
    </div>
  )

  return (
    <Card>
      <CardHeader>
        <CardTitle className="flex items-center gap-2">
          <ExternalLink className="h-5 w-5" />
          {t("txExplorer.title")}
        </CardTitle>
        <CardDescription>{t("txExplorer.description")}</CardDescription>
      </CardHeader>
      <CardContent className="space-y-4">
        <RadioGroup
          value={selectedExplorerId}
          onValueChange={onExplorerChange}
          disabled={isUpdating}
          className="space-y-4"
        >
          <div className="space-y-2">
            <p className="text-sm font-medium">{t("txExplorer.publicGroup")}</p>
            <div className="space-y-3">
              {publicExplorers.map(renderExplorerOption)}
            </div>
          </div>

          {localExplorers.length > 0 && (
            <div className="space-y-2">
              <p className="text-sm font-medium">{t("txExplorer.localGroup")}</p>
              <div className="space-y-3">
                {localExplorers.map(renderExplorerOption)}
              </div>
            </div>
          )}
        </RadioGroup>
        <p className="text-sm text-muted-foreground">{t("txExplorer.note")}</p>
      </CardContent>
    </Card>
  )
}
