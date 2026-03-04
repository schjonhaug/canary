"use client"

import { Heart } from "lucide-react"
import { useTranslations } from "next-intl"
import { useSearchParams } from "next/navigation"

export default function ThankYouPage() {
  const t = useTranslations("donation.thankYou")
  const searchParams = useSearchParams()
  const isRecurring = searchParams.has("checkoutPlanId")
  const variant = isRecurring ? "recurring" : "oneTime"

  return (
    <div className="flex flex-col items-center justify-center min-h-screen text-center px-4">
      <Heart className="h-12 w-12 text-primary mb-6" />
      <h1 className="text-3xl font-bold mb-3">{t(`${variant}.title`)}</h1>
      <p className="text-muted-foreground text-lg max-w-md">
        {t(`${variant}.description`)}
      </p>
      <p className="text-sm text-muted-foreground mt-6">
        {t("closeTab")}
      </p>
    </div>
  )
}
