"use client"

import { useState } from "react"
import { ChevronDown, RotateCcw } from "lucide-react"
import { useTranslations } from "next-intl"

import { Badge } from "@/components/ui/badge"
import { Button } from "@/components/ui/button"
import { Checkbox } from "@/components/ui/checkbox"
import {
  Collapsible,
  CollapsibleContent,
  CollapsibleTrigger,
} from "@/components/ui/collapsible"
import type { NotificationContentFields } from "@/types"

const GROUPS = [
  { key: "general", fields: ["wallet_name", "event_type"] },
  { key: "transactions", fields: ["transaction_amount", "transaction_balance"] },
  {
    key: "balanceAlerts",
    fields: ["balance_alert_condition", "balance_alert_threshold", "balance_alert_balance"],
  },
] as const satisfies ReadonlyArray<{
  key: string
  fields: ReadonlyArray<keyof NotificationContentFields>
}>

const CONTENT_FIELD_KEYS = GROUPS.flatMap((group) => group.fields)
const FINANCIAL_FIELD_KEYS = [
  "transaction_amount",
  "transaction_balance",
  "balance_alert_threshold",
  "balance_alert_balance",
] as const satisfies ReadonlyArray<keyof NotificationContentFields>

export const DEFAULT_NOTIFICATION_CONTENT_FIELDS: NotificationContentFields = {
  wallet_name: true,
  event_type: true,
  transaction_amount: false,
  transaction_balance: false,
  balance_alert_condition: false,
  balance_alert_threshold: false,
  balance_alert_balance: false,
}

export type NotificationContentMode = "recommended" | "activityOnly" | "custom"

export function getNotificationContentMode(
  fields: NotificationContentFields
): NotificationContentMode {
  if (
    CONTENT_FIELD_KEYS.every(
      (field) => fields[field] === DEFAULT_NOTIFICATION_CONTENT_FIELDS[field]
    )
  ) {
    return "recommended"
  }
  if (CONTENT_FIELD_KEYS.every((field) => !fields[field])) {
    return "activityOnly"
  }
  return "custom"
}

export function NotificationContentFieldsControl({
  value,
  onChange,
  disabled = false,
}: {
  value: NotificationContentFields
  onChange: (value: NotificationContentFields) => void
  disabled?: boolean
}) {
  const t = useTranslations("walletNotifications")
  const [isExpanded, setIsExpanded] = useState(false)
  const mode = getNotificationContentMode(value)
  const includedCount = CONTENT_FIELD_KEYS.filter((field) => value[field]).length
  const includesFinancialValues = FINANCIAL_FIELD_KEYS.some((field) => value[field])
  const summaryPrimary =
    mode === "recommended"
      ? t("content.summaryRecommendedPrimary")
      : mode === "activityOnly"
        ? t("content.summaryActivityOnlyPrimary")
        : t("content.summaryCustomPrimary", { count: includedCount })
  const summarySecondary =
    mode === "activityOnly"
      ? t("content.activityOnlyHidden")
      : includesFinancialValues
        ? t("content.valuesIncluded")
        : t("content.valuesHidden")
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
    <Collapsible open={isExpanded} onOpenChange={setIsExpanded} className="rounded-md border">
      <div className="space-y-3 p-3">
        <div className="flex flex-wrap items-center gap-2">
          <h4 className="text-sm font-medium">{t("content.title")}</h4>
          <Badge variant={mode === "recommended" ? "secondary" : "outline"}>
            {t(`content.badges.${mode === "recommended" ? "recommended" : "custom"}`)}
          </Badge>
        </div>
        <div className="space-y-0.5">
          <p className="text-sm">{summaryPrimary}</p>
          <p className="text-xs text-muted-foreground">{summarySecondary}</p>
        </div>
        <CollapsibleTrigger asChild>
          <Button
            type="button"
            variant="ghost"
            className="h-auto w-full justify-between p-0 text-left font-normal text-foreground hover:bg-transparent hover:text-foreground"
          >
            {isExpanded ? t("content.hide") : t("content.customize")}
            <ChevronDown
              className={`h-4 w-4 text-muted-foreground transition-transform ${isExpanded ? "rotate-180" : ""}`}
              aria-hidden="true"
            />
          </Button>
        </CollapsibleTrigger>
      </div>

      <CollapsibleContent>
        <div className="space-y-4 border-t p-3">
          <p className="text-xs text-muted-foreground">{t("content.description")}</p>
          <div className="grid gap-4 lg:grid-cols-3">
            {GROUPS.map((group) => (
              <div key={group.key} className="space-y-2">
                <h5 className="text-xs font-semibold uppercase text-muted-foreground">
                  {t(`content.groups.${group.key}`)}
                </h5>
                {group.fields.map((field) => (
                  <label key={field} className="flex items-start gap-2 text-sm">
                    <Checkbox
                      checked={value[field]}
                      disabled={disabled}
                      onCheckedChange={(checked) =>
                        onChange({ ...value, [field]: checked === true })
                      }
                    />
                    <span className="space-y-0.5">
                      <span className="block font-medium leading-none">
                        {t(`content.fields.${field}.label`)}
                      </span>
                      {(field === "event_type" || field.startsWith("balance_alert_")) && (
                        <span className="block text-xs leading-snug text-muted-foreground">
                          {t(`content.fields.${field}.description`)}
                        </span>
                      )}
                    </span>
                  </label>
                ))}
              </div>
            ))}
          </div>
          <div className="grid gap-3 sm:grid-cols-2">
            {[
              ["transaction", transactionLines],
              ["balanceAlert", balanceLines],
            ].map(([key, lines]) => (
              <div key={key as string} className="rounded-md bg-muted/50 p-3">
                <p className="mb-2 text-xs font-medium text-muted-foreground">
                  {t(`content.preview.${key as string}Title`)}
                </p>
                <pre className="whitespace-pre-wrap font-sans text-xs leading-relaxed">
                  {(lines as string[]).join("\n")}
                </pre>
              </div>
            ))}
          </div>
          <Button
            variant="ghost"
            size="sm"
            onClick={() => onChange({ ...DEFAULT_NOTIFICATION_CONTENT_FIELDS })}
            disabled={disabled || mode === "recommended"}
          >
            <RotateCcw className="h-4 w-4" />
            {t("content.reset")}
          </Button>
        </div>
      </CollapsibleContent>
    </Collapsible>
  )
}
