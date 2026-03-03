"use client"

import { Heart } from "lucide-react"
import { useTranslations } from "next-intl"
import Link from "next/link"

export default function ThankYouPage() {
  const t = useTranslations("thankYou")

  return (
    <div className="flex flex-col items-center justify-center py-24 text-center">
      <Heart className="h-12 w-12 text-primary mb-6" />
      <h1 className="text-3xl font-bold mb-3">{t("title")}</h1>
      <p className="text-muted-foreground text-lg mb-8 max-w-md">
        {t("description")}
      </p>
      <Link
        href="/settings"
        className="rounded-md bg-primary px-6 py-2.5 text-sm font-medium text-primary-foreground transition-colors hover:bg-primary/90"
      >
        {t("backToSettings")}
      </Link>
    </div>
  )
}
