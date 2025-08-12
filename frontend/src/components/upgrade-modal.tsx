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
import { getWalletLimit, getContactLimit, getTierDisplayName } from "@/lib/utils"
import { PlanComparison } from "./plan-comparison"
import { api } from "@/lib/api"
import { useAuth } from "@/contexts/auth-context"

interface UpgradeModalProps {
  isOpen: boolean
  onClose: () => void
  currentTier: string
  currentWalletCount?: number
  currentContactCount?: number
  limitType?: 'wallets' | 'contacts'
}

export function UpgradeModal({
  isOpen,
  onClose,
  currentTier,
  currentWalletCount = 0,
  currentContactCount = 0,
  limitType = 'wallets',
}: UpgradeModalProps) {
  const { isAuthenticated, refreshBillingStatus } = useAuth()
  const [isUpgrading, setIsUpgrading] = useState(false)
  const [upgradingTier, setUpgradingTier] = useState<string | null>(null)
  
  // Get the appropriate limit based on limitType
  const currentLimit = limitType === 'wallets' ? getWalletLimit(currentTier) : getContactLimit(currentTier)
  const currentCount = limitType === 'wallets' ? currentWalletCount : currentContactCount
  const limitTypeText = limitType === 'wallets' ? 'wallet' : 'contact'

  const handleUpgrade = async (targetTier: string, isYearly: boolean = true) => {
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

  const handleContactSales = () => {
    // Open email client for business plan inquiries
    window.location.href = 'mailto:sales@canarybitcoin.com?subject=Business Plan Inquiry&body=Hi, I am interested in the Business plan for Canary. Please contact me to discuss.'
    onClose()
  }

  return (
    <Dialog open={isOpen} onOpenChange={onClose}>
      <DialogContent 
        className="!max-w-none !w-[85vw] max-h-[90vh] overflow-y-auto p-8"
        style={{ width: '85vw !important', maxWidth: 'none !important' }}
      >
        <DialogHeader>
          <DialogTitle className="flex items-center gap-2">
            <Zap className="h-5 w-5 text-amber-500" />
            {limitType === 'wallets' ? 'Wallet' : 'Contact'} Limit Reached
          </DialogTitle>
          <DialogDescription>
            You&apos;ve reached your {limitTypeText} limit of {currentLimit} {limitTypeText}{currentLimit !== 1 ? 's' : ''} on the{' '}
            <Badge variant="outline" className="mx-1">
              {getTierDisplayName(currentTier)}
            </Badge>
            plan. Compare plans and upgrade to add more {limitTypeText}s.
          </DialogDescription>
        </DialogHeader>

        <div className="space-y-6">
          <div className="text-sm text-muted-foreground text-center">
            Current usage: {currentCount} / {currentLimit} {limitTypeText}s
          </div>

          <PlanComparison
            currentTier={currentTier.toLowerCase()}
            onUpgrade={handleUpgrade}
            onContactSales={handleContactSales}
            highlightUpgrades={true}
            showPricing={true}
            showBillingToggle={true}
            isModal={true}
            isLoading={isUpgrading}
            loadingTier={upgradingTier}
          />
        </div>
      </DialogContent>
    </Dialog>
  )
}