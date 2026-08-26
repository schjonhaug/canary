"use client"

import { ChevronDown, Plus, Target, Trash2, TrendingDown, TrendingUp } from "lucide-react"
import { useState } from "react"
import { useTranslations } from "next-intl"

import { Button } from "@/components/ui/button"
import {
  Collapsible,
  CollapsibleContent,
  CollapsibleTrigger,
} from "@/components/ui/collapsible"
import { Input } from "@/components/ui/input"
import { RadioGroup, RadioGroupItem } from "@/components/ui/radio-group"
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from "@/components/ui/select"
import { api, ApiError } from "@/lib/api"
import { btcToSats, getTranslatedApiError, parseBtcInput, satsToBtc } from "@/lib/utils"
import type { BalanceDraft } from "./types"

const TYPES = [
  { value: "above", icon: TrendingUp },
  { value: "equals", icon: Target },
  { value: "below", icon: TrendingDown },
] as const

export function formatBalanceDraft(alert: BalanceDraft): string {
  if (alert.threshold_currency && alert.threshold_fiat_amount) {
    return `${alert.threshold_fiat_amount} ${alert.threshold_currency}`
  }
  return `${satsToBtc(alert.threshold_sats ?? 0)} BTC`
}

export function BalanceDraftControls({
  walletChecksum,
  value,
  onChange,
  preferredFiatCurrency,
  disabled = false,
  defaultOpen = false,
}: {
  walletChecksum: string
  value: BalanceDraft[]
  onChange: (value: BalanceDraft[]) => void
  preferredFiatCurrency: string
  disabled?: boolean
  defaultOpen?: boolean
}) {
  const t = useTranslations("walletNotifications")
  const tApiErrors = useTranslations("errors.api")
  const [open, setOpen] = useState(defaultOpen)
  const [type, setType] = useState<BalanceDraft["alert_type"]>("below")
  const [amount, setAmount] = useState("")
  const [currency, setCurrency] = useState("BTC")
  const [error, setError] = useState<string | null>(null)
  const [validating, setValidating] = useState(false)
  const fiatCurrency = preferredFiatCurrency || "USD"

  const add = async () => {
    setError(null)
    let next: BalanceDraft
    if (currency === "BTC") {
      const btc = parseBtcInput(amount)
      if (btc === null) {
        setError(t("balance.errors.invalidBtc"))
        return
      }
      next = {
        id: crypto.randomUUID(),
        persisted: false,
        alert_type: type,
        threshold_sats: btcToSats(btc),
      }
    } else {
      const fiat = Number.parseFloat(amount)
      if (!Number.isFinite(fiat) || fiat <= 0) {
        setError(t("balance.errors.invalidFiat"))
        return
      }
      next = {
        id: crypto.randomUUID(),
        persisted: false,
        alert_type: type,
        threshold_currency: currency,
        threshold_fiat_amount: fiat,
      }
    }

    if (value.some((alert) =>
      alert.alert_type === next.alert_type &&
      alert.threshold_sats === next.threshold_sats &&
      alert.threshold_currency === next.threshold_currency &&
      alert.threshold_fiat_amount === next.threshold_fiat_amount
    )) {
      setError(tApiErrors("duplicate_alert"))
      return
    }

    setValidating(true)
    try {
      await api.validateBalanceAlert(walletChecksum, {
        alert_type: next.alert_type,
        threshold_sats: next.threshold_sats,
        threshold_currency: next.threshold_currency,
        threshold_fiat_amount: next.threshold_fiat_amount,
      })
      onChange([...value, next])
      setAmount("")
    } catch (caught) {
      setError(
        caught instanceof ApiError
          ? getTranslatedApiError(caught, tApiErrors)
          : t("balance.errors.validationFailed")
      )
    } finally {
      setValidating(false)
    }
  }

  return (
    <Collapsible open={open} onOpenChange={setOpen} className="rounded-md border">
      <CollapsibleTrigger asChild>
        <Button type="button" variant="ghost" className="h-auto w-full justify-between p-3 text-left">
          <span>
            <span className="block font-medium">{t("balance.title")}</span>
            <span className="mt-1 block text-xs font-normal text-muted-foreground">
              {value.length === 0
                ? t("balance.none")
                : t("balance.configured", { count: value.length })}
            </span>
          </span>
          <ChevronDown className={`h-4 w-4 transition-transform ${open ? "rotate-180" : ""}`} aria-hidden="true" />
        </Button>
      </CollapsibleTrigger>
      <CollapsibleContent>
        <div className="space-y-4 border-t p-3">
          {value.length > 0 && (
            <div className="space-y-2">
              {value.map((alert) => (
                <div key={alert.id} className="flex items-center justify-between gap-3 rounded-md border px-3 py-2 text-sm">
                  <span>{t(`alertTypes.${alert.alert_type}`)} {formatBalanceDraft(alert)}</span>
                  <Button
                    type="button"
                    variant="ghost"
                    size="icon"
                    className="h-7 w-7"
                    onClick={() => onChange(value.filter((item) => item.id !== alert.id))}
                    aria-label={t("balance.remove")}
                    disabled={disabled}
                  >
                    <Trash2 className="h-4 w-4" />
                  </Button>
                </div>
              ))}
            </div>
          )}
          <div className="flex flex-wrap items-center gap-3">
            <RadioGroup
              value={type}
              onValueChange={(next) => { setType(next as typeof type); setError(null) }}
              className="flex flex-wrap items-center gap-4"
              aria-label={t("balance.condition")}
              disabled={disabled}
            >
              {TYPES.map(({ value: option, icon: Icon }) => (
                <label key={option} className="flex items-center gap-2 text-sm">
                  <RadioGroupItem value={option} />
                  <Icon className="h-4 w-4 text-muted-foreground" aria-hidden="true" />
                  {t(`thresholdTypes.${option}`)}
                </label>
              ))}
            </RadioGroup>
            <Input
              value={amount}
              onChange={(event) => { setAmount(event.target.value); setError(null) }}
              placeholder={currency === "BTC" ? "0.10" : "10000"}
              className="w-[120px]"
              aria-label={t("balance.amount")}
              disabled={disabled || validating}
            />
            <Select
              value={currency}
              onValueChange={(next) => { setCurrency(next); setError(null) }}
              disabled={disabled || validating}
            >
              <SelectTrigger className="w-[120px]" aria-label={t("balance.currency")}>
                <SelectValue />
              </SelectTrigger>
              <SelectContent>
                <SelectItem value="BTC">BTC</SelectItem>
                <SelectItem value={fiatCurrency}>{fiatCurrency}</SelectItem>
              </SelectContent>
            </Select>
            <Button type="button" onClick={add} disabled={disabled || validating || !amount.trim()}>
              <Plus className="h-4 w-4" />
              {validating ? t("balance.validating") : t("balance.add")}
            </Button>
          </div>
          {error && <p role="alert" className="text-sm text-destructive">{error}</p>}
        </div>
      </CollapsibleContent>
    </Collapsible>
  )
}
