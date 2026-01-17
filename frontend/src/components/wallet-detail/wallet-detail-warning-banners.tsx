"use client"

import Link from "next/link"
import { AlertCircle, AlertTriangle } from "lucide-react"
import { Button } from "@/components/ui/button"
import { Alert, AlertDescription, AlertTitle } from "@/components/ui/alert"
import { useFormatters } from "@/hooks/useFormatters"
import type { Wallet } from "@/types"

interface WalletDetailWarningBannersProps {
  wallet: Wallet
  isConnected: boolean
  error: string | null
  lastUpdate: number | null
  isCloudMode: boolean
  billingStatus: { subscription_status: string } | null
  t: (key: string, params?: Record<string, string | number>) => string
}

export function WalletDetailWarningBanners({
  wallet,
  isConnected,
  error,
  lastUpdate,
  isCloudMode,
  billingStatus,
  t,
}: WalletDetailWarningBannersProps) {
  const { locale } = useFormatters()
  return (
    <>
      {/* Connection Warning Banner */}
      {(!isConnected || (error && wallet)) && (
        <Alert variant="destructive" className="mb-6">
          <AlertCircle className="h-4 w-4" />
          <AlertTitle>{t("connectionLost.title")}</AlertTitle>
          <AlertDescription>
            {t("connectionLost.description")}
            {lastUpdate && (
              <span className="block mt-1 text-xs">
                {t("connectionLost.lastUpdated", {
                  time: new Date(lastUpdate * 1000).toLocaleString(locale),
                })}
              </span>
            )}
          </AlertDescription>
        </Alert>
      )}

      {/* Inactive Wallet Warning Banner - only in cloud mode */}
      {isCloudMode && wallet.is_active === false && (
        <Alert className="mb-6 border-orange-200 bg-orange-50">
          <AlertTriangle className="h-4 w-4 text-orange-600" />
          <AlertTitle className="text-orange-700">
            {t("detail.inactive.title")}
          </AlertTitle>
          <AlertDescription className="text-orange-600">
            {billingStatus?.subscription_status === "expired"
              ? t("detail.inactive.descriptionExpired")
              : t("detail.inactive.descriptionTierLimit")}{" "}
            {t("detail.inactive.outdatedWarning")}
            <span className="block mt-2">
              <Link href="/subscription">
                <Button
                  size="sm"
                  className="bg-orange-600 hover:bg-orange-700 text-white"
                >
                  {t("detail.inactive.upgradePlan")}
                </Button>
              </Link>
            </span>
          </AlertDescription>
        </Alert>
      )}
    </>
  )
}
