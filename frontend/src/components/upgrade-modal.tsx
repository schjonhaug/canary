"use client"

import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog"
import { Badge } from "@/components/ui/badge"
import { Zap } from "lucide-react"
import { getWalletLimit, getTierDisplayName } from "@/lib/utils"
import { PlanComparison } from "./plan-comparison"

interface UpgradeModalProps {
  isOpen: boolean
  onClose: () => void
  currentTier: string
  currentWalletCount: number
}

export function UpgradeModal({
  isOpen,
  onClose,
  currentTier,
  currentWalletCount,
}: UpgradeModalProps) {
  const currentLimit = getWalletLimit(currentTier)

  const handleUpgrade = (targetTier: string) => {
    // TODO: Implement actual upgrade flow - could redirect to billing page
    console.log(`Upgrade to ${targetTier} requested`)
    
    // For now, show an alert (in a real app, this would redirect to billing)
    alert(`Upgrade to ${getTierDisplayName(targetTier)} plan requested. This would redirect to billing in a production app.`)
    onClose()
  }

  const handleContactSales = () => {
    // TODO: Implement contact sales flow - could open contact form or redirect to email
    console.log('Contact sales requested')
    
    // For now, open email client
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
            Wallet Limit Reached
          </DialogTitle>
          <DialogDescription>
            You've reached your wallet limit of {currentLimit} wallet{currentLimit !== 1 ? 's' : ''} on the{' '}
            <Badge variant="outline" className="mx-1">
              {getTierDisplayName(currentTier)}
            </Badge>
            plan. Compare plans and upgrade to add more wallets.
          </DialogDescription>
        </DialogHeader>

        <div className="space-y-6">
          <div className="text-sm text-muted-foreground text-center">
            Current usage: {currentWalletCount} / {currentLimit} wallets
          </div>

          <PlanComparison
            currentTier={currentTier.toLowerCase()}
            onUpgrade={handleUpgrade}
            onContactSales={handleContactSales}
            highlightUpgrades={true}
            showPricing={true}
            showBillingToggle={true}
            isModal={true}
          />
        </div>
      </DialogContent>
    </Dialog>
  )
}