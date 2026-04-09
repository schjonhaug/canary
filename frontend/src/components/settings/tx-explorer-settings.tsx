"use client"

import { Card, CardContent, CardDescription, CardHeader, CardTitle } from "@/components/ui/card"
import { Label } from "@/components/ui/label"
import { RadioGroup, RadioGroupItem } from "@/components/ui/radio-group"
import { ExternalLink } from "lucide-react"
import { useTranslations } from "next-intl"
import type { TxExplorerOption } from "@/lib/tx-explorers"

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

  if (explorers.length < 2) {
    return null
  }

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
        >
          {explorers.map((explorer) => (
            <div key={explorer.id} className="flex items-start gap-3 rounded-md border p-3">
              <RadioGroupItem value={explorer.id} id={`tx-explorer-${explorer.id}`} className="mt-1" />
              <div className="space-y-1">
                <Label htmlFor={`tx-explorer-${explorer.id}`} className="cursor-pointer">
                  {explorer.name}
                </Label>
                <p className="text-sm text-muted-foreground">{explorer.baseUrl}</p>
              </div>
            </div>
          ))}
        </RadioGroup>
        <p className="text-sm text-muted-foreground">{t("txExplorer.note")}</p>
      </CardContent>
    </Card>
  )
}
