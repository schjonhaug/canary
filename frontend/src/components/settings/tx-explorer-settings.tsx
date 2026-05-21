"use client"

import Image from "next/image"
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from "@/components/ui/card"
import { Button } from "@/components/ui/button"
import { Input } from "@/components/ui/input"
import { Label } from "@/components/ui/label"
import { RadioGroup } from "@/components/ui/radio-group"
import { ErrorDisplay } from "@/components/ui/error-display"
import { EndpointOption } from "@/components/settings/endpoint-option"
import { Blocks, Link } from "lucide-react"
import { useTranslations } from "next-intl"
import {
  CUSTOM_TX_EXPLORER_ID,
  isValidCustomTxExplorerTemplate,
  type TxExplorerOption,
} from "@/lib/tx-explorers"

const EXPLORER_LOGOS: Record<string, string> = {
  mempool: "/images/explorers/mempool.svg",
  bitfeed: "/images/explorers/bitfeed.svg",
  "btc-rpc-explorer": "/images/explorers/btc-rpc-explorer.svg",
}

interface TxExplorerSettingsProps {
  explorers: TxExplorerOption[]
  selectedExplorerId: string
  savedExplorerId: string
  customExplorerUrl: string
  savedCustomExplorerUrl: string
  settingsError: string | null
  isUpdating: boolean
  onExplorerChange: (explorerId: string) => void
  onCustomExplorerUrlChange: (url: string) => void
  onCustomExplorerSave: () => Promise<boolean>
}

interface ExplorerProvider {
  key: string
  name: string
  logo?: string
  options: TxExplorerOption[]
}

export function TxExplorerSettings({
  explorers,
  selectedExplorerId,
  savedExplorerId,
  customExplorerUrl,
  savedCustomExplorerUrl,
  settingsError,
  isUpdating,
  onExplorerChange,
  onCustomExplorerUrlChange,
  onCustomExplorerSave,
}: TxExplorerSettingsProps) {
  const t = useTranslations("settings")
  const tCommon = useTranslations("common")
  const platformLabels: Record<string, string> = {
    mynode: t("txExplorer.platform.mynode"),
    umbrel: t("txExplorer.platform.umbrel"),
    startos: t("txExplorer.platform.startos"),
  }
  const providerGroups = groupExplorersByProvider(explorers)
  const selectedCustom = selectedExplorerId === CUSTOM_TX_EXPLORER_ID
  const customUrlIsSaved =
    savedExplorerId === CUSTOM_TX_EXPLORER_ID &&
    customExplorerUrl.trim() === savedCustomExplorerUrl.trim()
  const canSaveCustomUrl =
    selectedCustom && !isUpdating && isValidCustomTxExplorerTemplate(customExplorerUrl) && !customUrlIsSaved
  const showCustomPreview = selectedCustom && isValidCustomTxExplorerTemplate(customExplorerUrl)

  if (explorers.length === 0) {
    return null
  }

  const endpointLabel = (explorer: TxExplorerOption) => {
    if (!explorer.isLocal) {
      return explorer.baseUrl
    }

    return explorer.platform
      ? (platformLabels[explorer.platform] ?? t("txExplorer.platform.local"))
      : t("txExplorer.platform.local")
  }

  return (
    <Card>
      <CardHeader>
        <CardTitle className="flex items-center gap-2">
          <Blocks className="h-5 w-5" />
          {t("txExplorer.title")}
        </CardTitle>
        <CardDescription>{t("txExplorer.description")}</CardDescription>
      </CardHeader>
      <CardContent className="space-y-4">
        <RadioGroup
          value={selectedExplorerId}
          onValueChange={onExplorerChange}
          disabled={isUpdating}
          className="space-y-3"
        >
          {providerGroups.map((provider) => (
            <div key={provider.key} className="rounded-md border p-3">
              <div className="flex items-start gap-3">
                <ExplorerIcon logo={provider.logo} name={provider.name} />
                <div className="min-w-0 flex-1 space-y-2">
                  <Label>{provider.name}</Label>
                  <div className="space-y-2 pt-1">
                    {provider.options.map((explorer) => (
                      <EndpointOption
                        key={explorer.id}
                        id={`tx-explorer-${explorer.id}`}
                        value={explorer.id}
                        label={endpointLabel(explorer)}
                      />
                    ))}
                  </div>
                </div>
              </div>
            </div>
          ))}

          <div className="rounded-md border p-3">
            <div className="flex items-start gap-3">
              <div className="flex h-8 w-8 shrink-0 items-center justify-center rounded-md border bg-background">
                <Link className="h-4 w-4 text-muted-foreground" aria-hidden="true" />
              </div>
              <div className="min-w-0 flex-1 space-y-2">
                <Label>{t("txExplorer.custom.title")}</Label>
                <div className="space-y-2 pt-1">
                  <EndpointOption
                    id="tx-explorer-custom"
                    value={CUSTOM_TX_EXPLORER_ID}
                    label={t("txExplorer.custom.endpointLabel")}
                  />
                  <div className="ml-6 flex flex-col gap-2 sm:flex-row">
                    <Input
                      aria-label={t("txExplorer.custom.label")}
                      aria-invalid={Boolean(settingsError)}
                      type="url"
                      placeholder={t("txExplorer.custom.placeholder")}
                      value={customExplorerUrl}
                      onFocus={() => onExplorerChange(CUSTOM_TX_EXPLORER_ID)}
                      onChange={(event) => onCustomExplorerUrlChange(event.target.value)}
                      disabled={isUpdating}
                    />
                    <Button
                      type="button"
                      variant="outline"
                      onClick={onCustomExplorerSave}
                      disabled={!canSaveCustomUrl}
                      className="shrink-0"
                    >
                      {isUpdating ? tCommon("saving") : tCommon("save")}
                    </Button>
                  </div>
                  {showCustomPreview && (
                    <p className="ml-6 break-all text-sm text-muted-foreground">
                      {t("txExplorer.custom.preview", {
                        url: customExplorerUrl.trim().replaceAll("{txid}", "<transaction-id>"),
                      })}
                    </p>
                  )}
                </div>
              </div>
            </div>
          </div>
        </RadioGroup>
        {settingsError && <ErrorDisplay message={settingsError} variant="inline" />}
        <p className="text-sm text-muted-foreground">{t("txExplorer.note")}</p>
      </CardContent>
    </Card>
  )
}

function groupExplorersByProvider(explorers: TxExplorerOption[]): ExplorerProvider[] {
  const groups = new Map<string, ExplorerProvider>()

  for (const explorer of explorers) {
    const key = explorerProviderKey(explorer)
    const existingGroup = groups.get(key)
    if (existingGroup) {
      existingGroup.options.push(explorer)
    } else {
      groups.set(key, {
        key,
        name: explorerProviderName(key, explorer),
        logo: EXPLORER_LOGOS[key],
        options: [explorer],
      })
    }
  }

  return Array.from(groups.values())
}

function explorerProviderKey(explorer: TxExplorerOption): string {
  const normalizedId = explorer.id.toLowerCase()
  const normalizedName = explorer.name.toLowerCase()

  if (normalizedId.includes("mempool") || normalizedName.includes("mempool")) return "mempool"
  if (normalizedId.includes("bitfeed") || normalizedName.includes("bitfeed")) return "bitfeed"
  if (normalizedId.includes("btc-rpc") || normalizedName.includes("btc rpc")) return "btc-rpc-explorer"
  return normalizedId
}

function explorerProviderName(key: string, explorer: TxExplorerOption): string {
  if (key === "mempool") return "Mempool"
  return explorer.name
}

function ExplorerIcon({ logo, name }: { logo?: string; name: string }) {
  return (
    <div className="flex h-8 w-8 shrink-0 items-center justify-center overflow-hidden rounded-md border bg-background">
      {logo ? (
        <Image
          src={logo}
          alt={`${name} logo`}
          width={32}
          height={32}
          className="h-full w-full object-contain"
        />
      ) : (
        <Link className="h-4 w-4 text-muted-foreground" aria-hidden="true" />
      )}
    </div>
  )
}
