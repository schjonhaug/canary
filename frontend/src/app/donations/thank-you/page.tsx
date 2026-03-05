"use client"

import { Suspense } from "react"
import { Heart } from "lucide-react"
import { useTranslations } from "next-intl"
import { useSearchParams } from "next/navigation"

function ThankYouContent() {
  const t = useTranslations("donation.thankYou")
  const searchParams = useSearchParams()
  // BTCPay Server appends ?checkoutPlanId=... when redirecting from a plan
  // (recurring) checkout. One-time invoice redirects have no such param.
  const isRecurring = searchParams.has("checkoutPlanId")
  const variant = isRecurring ? "recurring" : "oneTime"

  return (
    <>
      <h1 className="text-3xl font-bold mb-3">{t(`${variant}.title`)}</h1>
      <p className="text-muted-foreground text-lg max-w-md">
        {t(`${variant}.description`)}
      </p>
      <p className="text-sm text-muted-foreground mt-6">
        {t("closeTab")}
      </p>
    </>
  )
}

export default function ThankYouPage() {
  return (
    <div className="flex flex-col items-center justify-center min-h-screen text-center px-4">
      <Heart className="h-12 w-12 text-primary mb-6" />
      <Suspense>
        <ThankYouContent />
      </Suspense>
    </div>
  )
}
