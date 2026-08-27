"use client"

import { ChevronDown } from "lucide-react"
import { useState } from "react"
import { useTranslations } from "next-intl"

import { Badge } from "@/components/ui/badge"
import { Button } from "@/components/ui/button"
import { Checkbox } from "@/components/ui/checkbox"
import {
  Collapsible,
  CollapsibleContent,
  CollapsibleTrigger,
} from "@/components/ui/collapsible"
import { RadioGroup, RadioGroupItem } from "@/components/ui/radio-group"
import type { NotificationContentFields } from "@/types"
import { CONTENT_FIELD_KEYS, fieldsForPreset, getContentPreset } from "./utils"

export function ContentPresetControls({
  value,
  onChange,
  hasBalanceAlerts,
  disabled = false,
}: {
  value: NotificationContentFields
  onChange: (value: NotificationContentFields) => void
  hasBalanceAlerts: boolean
  disabled?: boolean
}) {
  const t = useTranslations("walletNotifications")
  const [customOpen, setCustomOpen] = useState(false)
  const preset = getContentPreset(value)
  const transactionLines = [
    t("content.preview.detected"),
    value.wallet_name ? t("content.preview.wallet") : null,
    value.event_type ? t("content.preview.transactionEvent") : null,
    value.transaction_amount ? t("content.preview.amount") : null,
    value.transaction_balance ? t("content.preview.transactionBalance") : null,
  ].filter(Boolean) as string[]
  const balanceLines = [
    t("content.preview.detected"),
    value.wallet_name ? t("content.preview.wallet") : null,
    value.event_type ? t("content.preview.balanceEvent") : null,
    value.balance_alert_condition ? t("content.preview.condition") : null,
    value.balance_alert_threshold ? t("content.preview.threshold") : null,
    value.balance_alert_balance ? t("content.preview.alertBalance") : null,
  ].filter(Boolean) as string[]

  return (
    <div className="space-y-4">
      <RadioGroup
        value={preset === "custom" ? "custom" : preset}
        onValueChange={(next) => {
          if (next === "custom") {
            setCustomOpen(true)
            return
          }
          onChange(fieldsForPreset(next as "useful" | "private" | "detailed"))
        }}
        className="grid gap-3 md:grid-cols-3"
        disabled={disabled}
        aria-label={t("privacy.title")}
      >
        {(["useful", "private", "detailed"] as const).map((option) => (
          <label key={option} className="flex items-start gap-3 rounded-md border p-3 text-sm">
            <RadioGroupItem value={option} aria-label={t(`privacy.presets.${option}.label`)} />
            <span className="space-y-1">
              <span className="flex flex-wrap items-center gap-2 font-medium leading-none">
                {t(`privacy.presets.${option}.label`)}
                {option === "useful" && <Badge variant="secondary">{t("privacy.recommended")}</Badge>}
              </span>
              <span className="block text-xs leading-snug text-muted-foreground">
                {t(`privacy.presets.${option}.description`)}
              </span>
            </span>
          </label>
        ))}
      </RadioGroup>

      {preset === "custom" && (
        <p className="text-sm text-muted-foreground">
          <Badge variant="outline">{t("privacy.presets.custom.label")}</Badge>{" "}
          {t("privacy.presets.custom.description")}
        </p>
      )}

      <Collapsible open={customOpen} onOpenChange={setCustomOpen} className="rounded-md border">
        <CollapsibleTrigger asChild>
          <Button type="button" variant="ghost" className="w-full justify-between p-3">
            {t("privacy.customize")}
            <ChevronDown
              className={`h-4 w-4 transition-transform ${customOpen ? "rotate-180" : ""}`}
              aria-hidden="true"
            />
          </Button>
        </CollapsibleTrigger>
        <CollapsibleContent>
          <div className="grid gap-3 border-t p-3 sm:grid-cols-2">
            {CONTENT_FIELD_KEYS.map((field) => (
              <label key={field} className="flex items-start gap-2 text-sm">
                <Checkbox
                  checked={value[field]}
                  disabled={disabled}
                  onCheckedChange={(checked) => onChange({ ...value, [field]: checked === true })}
                />
                <span>{t(`content.fields.${field}.label`)}</span>
              </label>
            ))}
          </div>
        </CollapsibleContent>
      </Collapsible>

      <div className={`grid gap-3 ${hasBalanceAlerts ? "sm:grid-cols-2" : ""}`}>
        <Preview title={t("content.preview.transactionTitle")} lines={transactionLines} />
        {hasBalanceAlerts && (
          <Preview title={t("content.preview.balanceAlertTitle")} lines={balanceLines} />
        )}
      </div>
    </div>
  )
}

function Preview({ title, lines }: { title: string; lines: string[] }) {
  return (
    <div className="rounded-md bg-muted/50 p-3">
      <p className="mb-2 text-xs font-medium text-muted-foreground">{title}</p>
      <pre className="whitespace-pre-wrap font-sans text-xs leading-relaxed">{lines.join("\n")}</pre>
    </div>
  )
}
