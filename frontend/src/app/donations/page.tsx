"use client"

import { Heart } from "lucide-react"
import { Card, CardContent } from "@/components/ui/card"
import { useTranslations } from "next-intl"

export default function DonationsPage() {
  const t = useTranslations("donation")

  return (
    <div className="max-w-2xl mx-auto py-16">
      <div className="text-center mb-10">
        <Heart className="h-10 w-10 text-primary mx-auto mb-4" />
        <h1 className="text-3xl font-bold mb-3">{t("title")}</h1>
        <p className="text-muted-foreground text-lg">
          {t("description")}
        </p>
      </div>

      <div className="grid gap-4 sm:grid-cols-2">
        <Card>
          <CardContent className="pt-6">
            <h2 className="font-semibold text-lg mb-2">{t("oneTimeTitle")}</h2>
            <p className="text-sm text-muted-foreground mb-4">
              {t("oneTimeDescription")}
            </p>
            <a
              href="https://btcpay.enogtjue.no/api/v1/invoices?storeId=DeKGGFNsD2aTzRrxHVNXHycinXw8KGaZY5xDCUUa7xJz"
              target="_blank"
              rel="noopener noreferrer"
              className="inline-block w-full rounded-md border border-border bg-background px-4 py-2.5 text-center text-sm font-medium transition-colors hover:bg-accent"
            >
              {t("oneTime")}
            </a>
          </CardContent>
        </Card>

        <Card>
          <CardContent className="pt-6">
            <h2 className="font-semibold text-lg mb-2">{t("recurringTitle")}</h2>
            <p className="text-sm text-muted-foreground mb-4">
              {t("recurringDescription")}
            </p>
            <a
              href="https://btcpay.enogtjue.no/plan-checkout/plancheckout_2rLnKuMUnyCrDbKoxv"
              target="_blank"
              rel="noopener noreferrer"
              className="inline-block w-full rounded-md bg-primary px-4 py-2.5 text-center text-sm font-medium text-primary-foreground transition-colors hover:bg-primary/90"
            >
              {t("recurring")}
            </a>
          </CardContent>
        </Card>
      </div>

      <p className="text-sm text-muted-foreground text-center mt-8">
        {t("paymentNote")}
      </p>
    </div>
  )
}
