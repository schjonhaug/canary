"use client"

import { Heart } from "lucide-react"
import { useTranslations } from "next-intl"

export default function ThankYouPage() {
  const t = useTranslations("donation.thankYou")

  return (
    <div className="flex flex-col items-center justify-center min-h-screen text-center px-4">
      <Heart className="h-12 w-12 text-primary mb-6" />
      <h1 className="text-3xl font-bold mb-3">{t("title")}</h1>
      <p className="text-muted-foreground text-lg max-w-md">
        {t("description")}
      </p>
      <p className="text-sm text-muted-foreground mt-6">
        {t("closeTab")}
      </p>
    </div>
  )
}
