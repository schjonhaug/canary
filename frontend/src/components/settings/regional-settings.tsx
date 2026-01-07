"use client"

import { Card, CardContent, CardDescription, CardHeader, CardTitle } from "@/components/ui/card"
import { Label } from "@/components/ui/label"
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from "@/components/ui/select"
import { Globe } from "lucide-react"
import { SUPPORTED_CURRENCIES } from "@/lib/currencies"
import { locales, localeNames, type Locale } from "@/i18n/config"
import { useTranslations } from "next-intl"

interface RegionalSettingsProps {
  currentLocale: Locale
  selectedCurrency: string
  isUpdatingCurrency: boolean
  isDisabled: boolean
  onLanguageChange: (locale: Locale) => void
  onCurrencyChange: (currency: string) => void
}

export function RegionalSettings({
  currentLocale,
  selectedCurrency,
  isUpdatingCurrency,
  isDisabled,
  onLanguageChange,
  onCurrencyChange,
}: RegionalSettingsProps) {
  const t = useTranslations("settings")

  return (
    <Card>
      <CardHeader>
        <CardTitle className="flex items-center gap-2">
          <Globe className="h-5 w-5" />
          {t("regional.title")}
        </CardTitle>
        <CardDescription>{t("regional.description")}</CardDescription>
      </CardHeader>
      <CardContent>
        <div className="space-y-4">
          <div>
            <Label htmlFor="language">{t("regional.languageLabel")}</Label>
            <Select
              value={currentLocale}
              onValueChange={(value) => onLanguageChange(value as Locale)}
              disabled={isDisabled}
            >
              <SelectTrigger id="language" className="w-full max-w-xs">
                <SelectValue />
              </SelectTrigger>
              <SelectContent>
                {[...locales]
                  .sort((a, b) => localeNames[a].localeCompare(localeNames[b]))
                  .map((locale) => (
                    <SelectItem key={locale} value={locale}>
                      {localeNames[locale]}
                    </SelectItem>
                  ))}
              </SelectContent>
            </Select>
          </div>
          <div>
            <Label htmlFor="currency">{t("regional.currencyLabel")}</Label>
            <Select
              value={selectedCurrency}
              onValueChange={onCurrencyChange}
              disabled={isUpdatingCurrency || isDisabled}
            >
              <SelectTrigger id="currency" className="w-full max-w-xs">
                <SelectValue placeholder={t("regional.currencyPlaceholder")} />
              </SelectTrigger>
              <SelectContent>
                {SUPPORTED_CURRENCIES.map((currency) => (
                  <SelectItem key={currency.code} value={currency.code}>
                    <span className="flex items-center gap-2">
                      <span className="font-mono text-sm">{currency.code}</span>
                      <span>{t(`currencies.${currency.code}`)}</span>
                      <span className="text-muted-foreground">({currency.symbol})</span>
                    </span>
                  </SelectItem>
                ))}
              </SelectContent>
            </Select>
            <p className="text-sm text-muted-foreground mt-2">{t("regional.currencyNote")}</p>
          </div>
        </div>
      </CardContent>
    </Card>
  )
}
