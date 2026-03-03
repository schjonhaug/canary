"use client"

import { Heart } from "lucide-react"
import { useTranslations } from "next-intl"

export default function DonationsPage() {
  const t = useTranslations("donation")

  return (
    <div className="flex flex-col items-center justify-center py-24 text-center">
      <Heart className="h-12 w-12 text-primary mb-6" />
      <h1 className="text-3xl font-bold mb-3">{t("title")}</h1>
      <p className="text-muted-foreground text-lg mb-8 max-w-md">
        {t("description")}
      </p>
      <div className="flex items-center gap-3">
        <a
          href="https://btcpay.enogtjue.no/api/v1/invoices?storeId=DeKGGFNsD2aTzRrxHVNXHycinXw8KGaZY5xDCUUa7xJz"
          target="_blank"
          rel="noopener noreferrer"
          className="rounded-md border border-border bg-background px-4 py-2 text-sm font-medium transition-colors hover:bg-accent"
        >
          {t("oneTime")}
        </a>
        <a
          href="https://btcpay.enogtjue.no/plan-checkout/plancheckout_2rLnKuMUnyCrDbKoxv"
          target="_blank"
          rel="noopener noreferrer"
          className="rounded-md border border-border bg-background px-4 py-2 text-sm font-medium transition-colors hover:bg-accent"
        >
          {t("recurring")}
        </a>
      </div>
    </div>
  )
}
