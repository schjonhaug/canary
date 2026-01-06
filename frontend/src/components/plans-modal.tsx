"use client"

import { useState } from "react"
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog"
import { Badge } from "@/components/ui/badge"
import { Zap } from "lucide-react"
import { getWalletLimit, getContactLimit } from "@/lib/utils"
import { PlanComparison } from "./plan-comparison"
import { api } from "@/lib/api"
import { useAuth } from "@/contexts/auth-context"
import { useTranslations } from "next-intl"

// Map tier slug to translation key (handles selfhosted -> selfHosted and lowercasing)
function getTierTranslationKey(tier: string): string {
  const lowerTier = tier.toLowerCase()
  return lowerTier === 'selfhosted' ? 'selfHosted' : lowerTier
}

interface PlansModalProps {
  isOpen: boolean
  onClose: () => void
  currentTier: string
  currentWalletCount?: number
  currentContactCount?: number
  limitType?: 'wallets' | 'contacts'
  isTrialUser?: boolean
  billingStatus?: {
    subscription_status: string
    stripe_customer_id?: string
  }
}

export function PlansModal({
  isOpen,
  onClose,
  currentTier,
  currentWalletCount = 0,
  currentContactCount = 0,
  limitType = 'wallets',
  isTrialUser = false,
  billingStatus,
}: PlansModalProps) {
  const t = useTranslations('upgrade')
  const tBilling = useTranslations('billing')
  const { isAuthenticated, refreshBillingStatus } = useAuth()
  const [isUpgrading, setIsUpgrading] = useState(false)
  const [upgradingTier, setUpgradingTier] = useState<string | null>(null)

  // Get the appropriate limit based on limitType
  const currentLimit = limitType === 'wallets' ? getWalletLimit(currentTier) : getContactLimit(currentTier)
  const currentCount = limitType === 'wallets' ? currentWalletCount : currentContactCount

  const handleUpgrade = async (targetTier: string, isYearly: boolean = false) => {
    if (!isAuthenticated) {
      // Redirect to sign up if not authenticated
      window.location.href = '/sign-up'
      return
    }

    try {
      setIsUpgrading(true)
      setUpgradingTier(targetTier)

      // Create checkout session
      const { url } = await api.createCheckoutSession(targetTier, isYearly)
      
      // Refresh billing status after user returns (in background)
      setTimeout(() => {
        refreshBillingStatus()
      }, 1000)

      // Redirect to Stripe checkout
      window.location.href = url

    } catch (error) {
      console.error('Failed to create checkout session:', error)
      alert('Failed to start checkout. Please try again.')
    } finally {
      setIsUpgrading(false)
      setUpgradingTier(null)
    }
  }

  const handleManageSubscription = async () => {
    if (!billingStatus?.stripe_customer_id) return

    try {
      setIsUpgrading(true)
      const { url } = await api.createCustomerPortalSession(window.location.href)
      window.location.href = url
    } catch (error) {
      console.error('Failed to open customer portal:', error)
      alert('Failed to open subscription management. Please try again.')
    } finally {
      setIsUpgrading(false)
    }
  }

  // Determine if user has an active paid subscription
  const hasPaidSubscription = billingStatus?.subscription_status === 'active' && !!billingStatus?.stripe_customer_id

  // Use customer portal for paid users, checkout for trial/new users
  const handleSubscriptionAction = hasPaidSubscription ? handleManageSubscription : handleUpgrade


  return (
    <Dialog open={isOpen} onOpenChange={onClose}>
      <DialogContent 
        className="!max-w-none !w-[85vw] max-h-[90vh] overflow-y-auto p-8"
        style={{ width: '85vw !important', maxWidth: 'none !important' }}
      >
        <DialogHeader>
          <DialogTitle className="flex items-center gap-2">
            <Zap className="h-5 w-5 text-amber-500" />
            {isTrialUser ? t('choosePlan') : t(`${limitType === 'wallets' ? 'walletLimit' : 'contactLimit'}.title`)}
          </DialogTitle>
          <DialogDescription>
            {isTrialUser ? (
              t('trialDescription')
            ) : (
              <>
                {t('limitReachedDescription', {
                  limitType: limitType === 'wallets' ? tBilling('plans.personal.features.wallets').split(' ')[1] : tBilling('plans.personal.features.contacts').split(' ')[1],
                  limit: currentLimit,
                  tierName: tBilling(`plans.${getTierTranslationKey(currentTier)}.name`)
                })}
              </>
            )}
          </DialogDescription>
        </DialogHeader>

        <div className="space-y-6">
          {!isTrialUser && (
            <div className="text-sm text-muted-foreground text-center">
              {t('currentUsage', { count: currentCount, limit: currentLimit })}
            </div>
          )}

          <PlanComparison
            currentTier={currentTier.toLowerCase()}
            onUpgrade={handleSubscriptionAction}
            highlightUpgrades={!isTrialUser}
            showPricing={true}
            isModal={true}
            showAllTiers={isTrialUser}
            isTrialUser={isTrialUser}
            isLoading={isUpgrading}
            loadingTier={upgradingTier}
            hasPaidSubscription={hasPaidSubscription}
          />
        </div>
      </DialogContent>
    </Dialog>
  )
}