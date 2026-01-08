"use client"

import { Card, CardHeader, CardTitle, CardDescription } from "@/components/ui/card"
import { AlertTriangle } from "lucide-react"
import { PlanComparison } from "./plan-comparison"
import { getTierDisplayName, getWalletLimit, getContactLimit } from "@/lib/utils"
import { useTranslations } from "next-intl"

interface UpgradePromptProps {
  limitType: 'wallets' | 'contacts'
  currentTier: string
  onUpgrade: (targetTier: string, isYearly?: boolean) => void
  isLoading?: boolean
  loadingTier?: string | null
  hasPaidSubscription?: boolean
}

export function UpgradePrompt({
  limitType,
  currentTier,
  onUpgrade,
  isLoading = false,
  loadingTier = null,
  hasPaidSubscription = false,
}: UpgradePromptProps) {
  const t = useTranslations('wallets')

  const limit = limitType === 'wallets'
    ? getWalletLimit(currentTier)
    : getContactLimit(currentTier)

  return (
    <div className="space-y-6">
      <Card className="border-amber-200 bg-amber-50 dark:border-amber-900 dark:bg-amber-950/30">
        <CardHeader>
          <CardTitle className="flex items-center gap-2 text-amber-800 dark:text-amber-400">
            <AlertTriangle className="h-5 w-5" />
            {t('add.limitReached.title')}
          </CardTitle>
          <CardDescription className="text-amber-700 dark:text-amber-500">
            {t('add.limitReached.description', { tier: getTierDisplayName(currentTier), count: limit })}
          </CardDescription>
        </CardHeader>
      </Card>

      <PlanComparison
        currentTier={currentTier}
        onUpgrade={onUpgrade}
        highlightUpgrades={true}
        showPricing={true}
        isModal={false}
        isLoading={isLoading}
        loadingTier={loadingTier}
        hasPaidSubscription={hasPaidSubscription}
      />
    </div>
  )
}
