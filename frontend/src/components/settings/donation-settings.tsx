"use client"

import { Card, CardContent, CardDescription, CardHeader, CardTitle } from "@/components/ui/card"
import { Heart } from "lucide-react"
import { useTranslations } from "next-intl"

export function DonationSettings() {
  const t = useTranslations("settings")
  const tDonation = useTranslations("donation")

  return (
    <Card>
      <CardHeader>
        <CardTitle className="flex items-center gap-2">
          <Heart className="h-5 w-5" />
          {t("donation.title")}
        </CardTitle>
        <CardDescription>{t("donation.description")}</CardDescription>
      </CardHeader>
      <CardContent>
        <div className="flex items-center gap-3">
          <a
            href="https://btcpay.enogtjue.no/api/v1/invoices?storeId=DeKGGFNsD2aTzRrxHVNXHycinXw8KGaZY5xDCUUa7xJz"
            target="_blank"
            rel="noopener noreferrer"
            className="rounded-md border border-border bg-background px-4 py-2 text-sm font-medium transition-colors hover:bg-accent"
          >
            {tDonation("oneTime")}
          </a>
          <a
            href="https://pay.schjonhaug.dev/apps/PAYReq/crowdfund-recurring"
            target="_blank"
            rel="noopener noreferrer"
            className="rounded-md border border-border bg-background px-4 py-2 text-sm font-medium transition-colors hover:bg-accent"
          >
            {tDonation("recurring")}
          </a>
        </div>
      </CardContent>
    </Card>
  )
}
